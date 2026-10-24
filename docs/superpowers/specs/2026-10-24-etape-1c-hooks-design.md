# Kairos, étape 1c : les hooks de Claude Code et la détection du lot

Date : 24 octobre 2026. Source de vérité du produit : la page Notion « Kairos » (en particulier « Claude et Kairos »). Ce document ne décrit que le sous-projet **1c**. Il s'appuie sur le 1a (cœur et ligne de commande) et le 1b (serveur MCP), tous deux sur `main` et publiés dans la 0.2.0.

## But

Quand Lucas travaille dans Claude Code, Kairos suit le bon lot sans qu'il y pense : il lance ou bascule tout seul quand l'ID Monday est **sûr**, et **dans le doute il n'agit pas : Claude demande la vérité à l'utilisateur**. On ne devine jamais un ID.

Hors 1c :

- plusieurs projets et dossiers liés à un projet (il n'y a que le projet par défaut) : étape 2 ;
- le hook `Stop` et la « dernière activité » : étape 2, avec l'app qui juge quand le travail s'arrête ;
- « Annuler la bascule » dans l'icône (l'app) : étape 2. En 1c la bascule est réversible par `kairos switch <ancien lot>`, dont la commande est écrite dans le message visible ;
- Claude Desktop : pas de hooks possibles ; la règle va dans les instructions d'un projet Desktop (déjà documenté).

## Décisions prises

- **Revenir sur un lot précédent sans ID** (« go retourne sur Bundle », « on reprend ») : c'est Claude qui comprend la phrase, pas le hook, et il n'y a aucune phrase préenregistrée. Pour cela, deux nouveaux outils (`recent`, `back`) et une règle : lancer un lot avec un libellé. Voir « Revenir sur un lot précédent ».
- **Périmètre** : hooks `SessionStart` et `UserPromptSubmit`, détection du lot, installation, commande `/kairos`. Un seul projet.
- **Autonomie** : le hook suit le réglage `claude_autonomy` (lu à chaque appel, comme le MCP). `auto` : il agit seul quand il est sûr. `ask` (défaut) : il n'agit jamais, il informe Claude. Dans les deux modes, un doute devient une question posée à l'utilisateur.
- **Installation** : `kairos hooks install`, jamais un copier-coller ; rien ne se modifie sans que la personne lance la commande.
- **Sous-tâche** : poste principal par défaut, comme la règle du CLAUDE.md. L'ID d'un commit est un **parent** (99 % des cas) ; s'il correspond à une sous-tâche déjà mémorisée par `set_subtask_id`, le hook retrouve son parent et sa sous-tâche.

## Où ça vit

- `crates/kairos-core/src/detect.rs` : fonctions **pures** (sans entrée-sortie, sans horloge) : extraire les IDs d'un texte, décider. Testées en tableau.
- `crates/kairos-core/src/service.rs` : `Kairos::resolve_monday_id(id) -> Result<KnownId>`, lecture seule, pour savoir si un ID est un parent connu, une sous-tâche mémorisée, ou inconnu.
- `crates/kairos-cli/src/hook/` : `input.rs` (lit le JSON de Claude Code), `sources.rs` (branche Git, commits, `.kairos.json`), `output.rs` (le JSON rendu), `mod.rs` (l'enchaînement) et `install.rs`.
- `Kairos::recent_lots(limit) -> Result<Vec<RecentLot>>` et `Kairos::back() -> Result<SwitchResult>` dans le cœur ; les outils MCP `recent` et `back` (16 outils au total, le test du 1b qui comptait 14 est mis à jour) et les commandes `kairos recent` / `kairos back` les exposent.
- `kairos hook <session-start|user-prompt>` et `kairos hooks <install|uninstall>` : deux sous-commandes du binaire existant. Même base, même horloge, même `KAIROS_DB`.

## La détection du lot

Les sources, **par ordre de fiabilité**. On prend la première qui donne **un seul** parent ; les suivantes ne sont lues que si la précédente ne donne rien.

1. **Le message** de l'utilisateur.
2. **La branche Git** (`git -C <cwd> rev-parse --abbrev-ref HEAD`).
3. **Les derniers commits** : sujet et corps des 10 derniers (`git log -n 10`).
4. **`.kairos.json`** : cherché en remontant depuis le dossier de travail jusqu'à la racine du dépôt ; `{"parent": "…", "label": "…", "subtask": "…"}`, seul `parent` est obligatoire.

Chaque lecture Git a un délai de 2 secondes ; un échec (pas un dépôt, Git absent, délai) vaut « pas de source », jamais une erreur.

**Aucun format imposé.** Chacun écrit ses commits, ses branches et ses messages à sa façon (`[Monday #A, #B]`, `Monday: A`, `fix(bundle) A B C`, `closes #A`, un lien, rien d'autre qu'un numéro…). Kairos ne cherche donc **pas un gabarit** : il cherche la **forme d'un ID Monday**, n'importe où dans le texte (sujet, corps, nom de branche, message) :

- un **lien Monday** (`monday.com/…/pulses/<id>` ou `…/items/<id>`) : l'ID est celui du lien ;
- sinon un **nombre de 9 à 12 chiffres**, sans zéro initial, délimité par autre chose que des chiffres ou des lettres (donc pas un morceau d'un identifiant plus long).

Autant d'IDs que le texte en contient, dans n'importe quel ordre et avec n'importe quel séparateur ; les doublons comptent pour un. Les nombres de moins de 9 ou plus de 12 chiffres (version, date, numéro d'issue GitHub) sont ignorés.

**Un ID est sûr** dans l'un de ces cas :

- il vient d'un **lien Monday** ou de `.kairos.json` ;
- Kairos le **reconnaît** (`resolve_monday_id` : parent connu, ou sous-tâche mémorisée) ;
- il porte un **repère** juste devant lui, sur la même ligne et à moins de 12 caractères : le mot « monday » (sans tenir compte de la casse), « ticket », « tâche », « task », « item » ou « pulse », ou un `#`.

Un **numéro nu** sans repère que Kairos ne reconnaît pas n'est pas sûr : c'est un doute (ça peut être un téléphone, un horodatage, un numéro de commande). Un ID sûr que Kairos ne connaissait pas est un **nouveau parent** (le message visible l'indique).

## Ce que fait `UserPromptSubmit`

| Situation                                      | `auto`                                 | `ask` (défaut) |
| ---------------------------------------------- | -------------------------------------- | -------------- |
| Rien en cours, ID sûr                          | lance le lot                           | informe Claude |
| Autre lot en cours, ID sûr **dans le message** | bascule                                | informe Claude |
| **Doute** (ci-dessous)                         | ne fait rien, dit à Claude de demander | idem           |
| Même lot, ou aucun ID                          | rien                                   | rien           |

**Cas de doute :**

- plusieurs parents dans le **message** ;
- rien en cours, et la branche ou les commits citent plusieurs parents (voir « Plusieurs tâches sur une même branche ») ;
- un lot est en cours, mais il n'est pas parmi les parents que citent la branche et les commits ;
- la branche et les commits pointent vers deux parents différents ;
- un numéro nu sans repère que Kairos ne reconnaît pas.

Quand le hook **informe**, la ligne ajoutée au contexte de Claude (`additionalContext`) dit : « ID détecté <id> (<source>). Appelle `switch` (ou `start`) si c'est bien ça. » Quand il y a **doute** : « Doute : <ce qui est en conflit>. Demande à l'utilisateur lequel avant d'appeler `start` ou `switch`. Ne devine pas. » Quand le hook **agit seul**, un `systemMessage` visible dit ce qui a changé et comment revenir en arrière (`kairos switch <ancien lot>`) ; si le parent est nouveau pour Kairos, il l'écrit (« nouveau parent »). Aucune ligne n'est ajoutée quand il n'y a rien à dire.

Deux sessions Claude Code en parallèle : le lot suit la dernière qui parle (le dernier message gagne), puisque chaque hook lit l'état courant.

## Plusieurs tâches sur une même branche

Cas fréquent : une branche, trois tâches, trois IDs Monday dans les commits, écrits comme chacun en a l'habitude. La branche et les commits donnent alors un **ensemble** de parents, pas un seul, et c'est normal :

- **Un lot est en cours et il est dans l'ensemble** : tout est cohérent, **rien à dire**, même s'il y a trois parents. Ce n'est pas un doute.
- **Rien en cours** : doute, la question liste les candidats (du commit le plus récent au plus ancien) pour que Claude demande sur laquelle tu travailles.
- **Un lot est en cours mais n'est pas dans l'ensemble** : doute (« Kairos suit X, la branche concerne A, B, C »).
- **Tu passes à la tâche suivante** : tu donnes son ID dans le message (source sûre) et le hook bascule. Un commit qui cite `B` ne bascule jamais le lot tout seul, puisqu'il ne dit pas ce que tu fais maintenant.
- **Le rapport du soir** garde une ligne par parent : chaque `switch` coupe l'entrée, donc le temps de A, B et C est bien séparé.

Les commits sont d'ordinaire un par tâche, donc un ID chacun ; il arrive qu'un commit en cite plusieurs (il touche plusieurs tâches). Ses IDs entrent simplement dans l'ensemble, sans ordre entre eux : un commit à trois IDs donne le même ensemble que trois commits. **Kairos ne répartit jamais le temps entre les IDs d'un commit** : il ne sait pas combien de temps chaque tâche a pris. Le lot en cours reste celui que tu as annoncé (ou le doute demande lequel) ; si tu as travaillé sur les trois, tu bascules entre eux, ou tu corriges ensuite avec `add` / `edit`.

Le dernier commit n'est qu'un indice affiché dans la question (« dernier commit : #C »), jamais une décision.

**Un même doute ne revient pas à chaque message** : une fois posé dans une session, le même doute (même texte) n'est plus rappelé tant qu'il ne change pas, pour que Claude ne te redemande pas toutes les minutes. Le hook garde pour cela un petit fichier `hook-state.json` à côté de la base (les 50 dernières sessions, par `session_id`) ; s'il est illisible ou impossible à écrire, le hook ignore le fichier et peut redemander, sans jamais échouer.

## Ce que fait `SessionStart`

Rend à Claude l'état de Kairos (rien en cours, ou le lot et sa durée) et les règles : ne jamais arrêter un lot en fin de réponse ; ne jamais deviner un ID, le demander ; si aucun lot ne tourne et qu'un travail sur une tâche commence, demander l'ID ; sous-tâche non précisée : poste principal. Ces règles dispensent d'un bloc dans le CLAUDE.md.

## Revenir sur un lot précédent

Le scénario type : Lucas est sur la feature Bundle, une urgence arrive, il donne l'ID de l'urgence (le hook bascule, ou prévient Claude en `ask`), Claude crée sa branche et la traite ; une fois la PR validée, Lucas dit « go retourne sur Bundle ». La phrase ne contient pas d'ID : le hook ne fait rien, **Claude** la comprend et appelle Kairos. Pour qu'il le puisse sans phrase préenregistrée :

- **`recent`** (outil MCP et `kairos recent [--limit N]`, lecture seule) : les lots des 7 derniers jours, au plus 8, du plus récent au plus ancien, un par lot distinct : libellé, ID du parent, sous-tâche, heure de la dernière fin, total du jour, et si c'est le lot en cours. Claude l'appelle quand l'utilisateur parle d'un lot sans donner d'ID, y retrouve le bon, puis appelle `switch` avec son ID. Zéro candidat plausible ou plusieurs : il demande.
- **`back`** (outil MCP et `kairos back`) : revient au lot juste avant l'actuel (celui de la dernière entrée fermée dont le lot est différent de celui en cours). Rien en cours : reprend le dernier lot fermé. Deux `back` de suite font l'aller-retour entre deux lots. Erreur `no_previous_lot` s'il n'y en a pas. Comme `start` et `switch`, il n'est pas soumis à `claude_autonomy`.
- **Le libellé** : la règle du `SessionStart` demande à Claude de passer `label` (le nom de la feature en deux ou trois mots) à chaque `start` ou `switch` quand il le connaît, sinon `recent` n'afficherait qu'un numéro.

**Le temps** : revenir sur un lot ouvre une **nouvelle entrée** sur le même lot ; le temps déjà fait est conservé et le total du jour s'additionne (le rapport du soir regroupe tout sur la même ligne). L'urgence garde le sien.

**Au retour, côté branche** : si Claude change de branche vers celle de Bundle sans avoir fait `back`, le hook voit une branche qui pointe vers un autre parent que le lot en cours : c'est un doute, Claude demande.

## Le format et la robustesse

- **Entrée** : le JSON de Claude Code sur l'entrée standard (`session_id`, `cwd`, `hook_event_name`, `prompt`). Les champs inconnus sont ignorés ; le format exact est contrôlé contre la documentation de Claude Code à l'implémentation.
- **Sortie** : un JSON sur la sortie standard (`hookSpecificOutput.additionalContext`, `systemMessage`), rien d'autre.
- **Un hook ne fait jamais échouer le message de l'utilisateur** : JSON invalide, entrée vide ou énorme (plus de 1 Mo : seul le début est lu), base verrouillée, base plus récente, Git absent → il se tait et sort avec le code 0. Le détail part sur la sortie d'erreur.
- **Confidentialité** : le texte du message n'est lu que pour y chercher des IDs, puis jeté. Il n'est jamais écrit dans la base ni dans un journal.
- **Source** des lots lancés : `claude-code`.

## L'installation

- `kairos hooks install [--dry-run]` : ajoute les deux hooks dans `~/.claude/settings.json` (ou `$CLAUDE_CONFIG_DIR/settings.json` si défini), avec le **chemin absolu du binaire courant**, pour ne pas dépendre du PATH de l'application qui lance Claude Code ; délai du hook : 5 secondes. Écrit aussi `commands/kairos.md` (la commande `/kairos` : sans argument, l'état ; avec un ID, un `switch`).
- Avant d'écrire, sauvegarde `settings.json.kairos-backup-<horodatage>`. Les réglages existants (autres hooks, permissions) sont conservés tels quels. Relancer la commande ne crée pas de doublon : les hooks de Kairos sont reconnus à leur commande (`… hook session-start` / `… hook user-prompt`).
- Un `settings.json` qui n'est pas du JSON valide est **refusé** : rien n'est touché, le message dit quoi corriger.
- `kairos hooks uninstall` retire uniquement les hooks de Kairos et `/kairos`.
- Un fichier `commands/kairos.md` différent de celui de Kairos n'est jamais écrasé sans l'avoir signalé : la commande s'arrête et dit de le déplacer.

## Cas limites à traiter (et tester)

1. Message sans ID, avec un horodatage de 10 chiffres, un numéro de téléphone, une date : pas de lot, ou doute si le numéro est de la bonne forme.
2. Plusieurs IDs dans un message : doute, la question liste les candidats.
3. Même ID écrit deux fois dans le message : un seul candidat, pas un doute.
4. ID dans un bloc de code ou une citation : traité comme du texte ordinaire (pas d'analyse de Markdown).
5. Le lot en cours est déjà le bon : rien, même si le message cite l'ID.
6. `claude_autonomy` change entre deux messages : pris en compte immédiatement.
7. Dépôt sans commits, tête détachée, dossier hors dépôt : pas de source, pas d'erreur.
8. `.kairos.json` invalide ou sans `parent` : ignoré, avec une ligne sur la sortie d'erreur.
9. Un ID de commit qui est la sous-tâche mémorisée d'un parent connu : le lot est ce parent et cette sous-tâche.
10. Un commit qui cite trois IDs, de n'importe quelle façon (un seul crochet, plusieurs, virgules, espaces, `&`, retours à la ligne, `closes #A #B`, lien) : les trois entrent dans l'ensemble. Des nombres de moins de 9 ou plus de 12 chiffres (version, date, numéro d'issue) sont ignorés. Un nombre collé à des lettres (`abc1234567890xyz`) n'est pas un ID.
11. Une branche avec trois parents dans les commits : lot en cours dans l'ensemble → silence ; rien en cours → une seule question, pas répétée au message suivant ; lot hors ensemble → doute.
12. « Retourne sur Bundle » avec deux lots dont le libellé contient « bundle » : `recent` en rend deux, Claude demande lequel.
13. `back` sans lot précédent, ou quand le lot précédent est déjà le lot en cours : `no_previous_lot`.
14. `settings.json` absent (créé), vide (réécrit), invalide (refusé), avec des hooks d'autres outils (conservés).

## Tests

Les règles de détection se testent sans entrée-sortie ; le reste sur le vrai binaire avec une base temporaire, une heure figée et un faux `HOME` / `CLAUDE_CONFIG_DIR`.

- **Cœur** : extraction des IDs en tableau (liens de plusieurs formes, tous les gabarits de commit ci-dessus, numéros nus avec et sans repère, faux positifs : version, date, numéro d'issue, téléphone, horodatage), décision pour toute la matrice `auto` / `ask` × situation, `resolve_monday_id` (parent, sous-tâche, inconnu).
- **Hook** : un JSON valide dans chaque situation du tableau ; JSON invalide, vide, énorme, champs en trop ; sortie standard qui ne contient que du JSON ; code de sortie toujours 0 ; base verrouillée ou plus récente.
- **Plusieurs tâches par branche** : les trois cas de la section ci-dessus ; le doute qui ne revient pas ; `hook-state.json` illisible, absent, impossible à écrire.
- **Sources** : dépôt Git temporaire (branche avec ID, commits écrits de plusieurs façons, tête détachée), `.kairos.json` valide et invalide.
- **Installation** : dans un faux `HOME` (création, relance sans doublon, conservation des réglages existants, sauvegarde, refus d'un JSON invalide, désinstallation, `--dry-run` qui n'écrit rien).
- **Retour** : `recent_lots` (ordre, fenêtre de 7 jours, un seul exemplaire par lot, total du jour, lot en cours marqué) ; `back` (aller-retour, rien en cours, aucun précédent, temps additionné sur le même lot) ; les deux outils et les deux commandes sur le vrai binaire.
- **Bout en bout** : un vrai échange `UserPromptSubmit` en `auto` qui lance puis bascule un lot réel dans la base temporaire.

## Ordre de travail (esquisse, le plan le détaille)

1. Cœur : `detect.rs` (extraction et décision), `resolve_monday_id`, `recent_lots` et `back`, puis leurs outils MCP et commandes.
2. `kairos hook user-prompt` et `session-start` : entrée, sources Git et `.kairos.json`, sortie, robustesse.
3. `kairos hooks install` / `uninstall` et la commande `/kairos`.
4. Bout en bout, README, mise à jour de la page Notion « Claude et Kairos » (Stop et projets reportés).
5. Essai réel dans Claude Code, sur une copie de ta configuration (`CLAUDE_CONFIG_DIR`), puis sur la vraie sur ton feu vert.
