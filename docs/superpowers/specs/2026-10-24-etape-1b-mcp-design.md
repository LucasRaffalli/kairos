# Kairos, étape 1b : le serveur MCP

Date : 24 octobre 2026. Source de vérité du produit : la page Notion « Kairos » (en particulier « Claude et Kairos »). Ce document ne décrit que le sous-projet **1b**. Il s'appuie sur le 1a (le cœur et la ligne de commande, fusionnés sur `main`) ; les hooks de Claude Code et la détection du lot sont le 1c.

## But

Claude Code et Claude Desktop pilotent Kairos : lancer, changer, arrêter, corriger une journée, préparer la ligne à saisir dans Monday. Le serveur MCP **n'ajoute aucune règle métier** : chaque outil appelle le même service `Kairos` que la ligne de commande, donc donne les mêmes refus et les mêmes messages.

Hors 1b : `week` (la vue Semaine arrive à l'étape 3, le cœur n'a pas encore ce calcul), les hooks et la détection du projet ou du lot (1c), l'écran Réglages de l'app (étape 2), l'installation de `kairos` dans le PATH (étape 2).

## Décisions prises

- **Autonomie réglable.** Le réglage `claude_autonomy` vaut `ask` (par défaut) ou `auto`. En `ask`, Claude doit te demander avant les trois actions difficiles à défaire ; en `auto`, il agit seul. Toi : `auto`. Une collègue qui veut garder la main : `ask`.
- **Claude ne peut pas changer ce réglage.** Aucun outil MCP ne lit ni n'écrit les réglages : seuls la ligne de commande (`kairos config claude_autonomy auto`) et l'app le modifient. Un outil qui permettrait à Claude de s'accorder l'autonomie à lui-même viderait le réglage de son sens.
- **`week` reporté** à l'étape 3.

## Où ça vit

Une sous-commande `kairos mcp` du binaire `kairos` (`crates/kairos-cli`), module `mcp.rs`. Transport **stdio** : Claude lance le processus, lui parle en JSON-RPC sur l'entrée et la sortie standard, et il s'arrête quand l'entrée se ferme. Bibliothèque : `rmcp`. La sortie standard ne porte **que** du protocole ; tout journal part sur la sortie d'erreur.

Le module `time_input` (lecture des heures `13:30`, `AAAA-MM-JJTHH:MM`, RFC 3339) reste partagé avec la ligne de commande. L'horloge est la même (`SystemClock`, ou `KAIROS_NOW` / `KAIROS_TZ_OFFSET` pour les tests).

La base est **ouverte à chaque appel d'outil**, pas une fois pour toute la vie du processus : si l'app met la base à jour, ou si elle est verrouillée un instant, chaque appel le constate lui-même (`db_newer`, `db_unavailable`) au lieu de s'appuyer sur une connexion périmée.

## Les 14 outils

Les paramètres suivent les options de la ligne de commande. Les heures sont des chaînes ; les dates sont `AAAA-MM-JJ` (aujourd'hui par défaut). Aucun outil n'expose `source` : le serveur la déduit du nom du client MCP annoncé à l'ouverture (`claude-code` si le nom contient « code », sinon `claude-desktop`).

| Outil             | Paramètres                                                 | Rôle                                                                     |
| ----------------- | ---------------------------------------------------------- | ------------------------------------------------------------------------ |
| `start`           | `parent`, `label`, `name`, `subtask`, `subtask_id`, `at`   | Lance un lot. Sans `subtask` : le poste principal.                       |
| `stop`            | `at`                                                       | Arrête le lot en cours.                                                  |
| `pause`, `resume` | aucun                                                      | Suspend ou reprend sans fermer le lot.                                   |
| `switch`          | comme `start`                                              | Arrête le lot actuel et lance le suivant à la même seconde.              |
| `status`          | aucun                                                      | Ce que fait Kairos, durée, total du jour.                                |
| `today`           | `date`                                                     | Les entrées d'une journée, leurs numéros, les bornes.                    |
| `report`          | `date`                                                     | La ventilation, l'écart, le texte à saisir, les temps sans ID détaillés. |
| `add_entry`       | `parent`, `label`, `name`, `subtask`, `from`, `to`, `date` | Ajoute un temps oublié.                                                  |
| `edit_entry`      | `id`, `from`, `to`, `parent`, `name`, `subtask`, `date`    | Corrige une entrée.                                                      |
| `delete_entry`    | `id`, `confirm`                                            | Supprime une entrée. **Soumis à l'autonomie.**                           |
| `close_day`       | `date`, `stop`, `confirm`                                  | Clôture une journée. **Soumis à l'autonomie.**                           |
| `mark_declared`   | `date`, `confirm`                                          | Note la journée saisie dans Monday. **Soumis à l'autonomie.**            |
| `set_subtask_id`  | `parent`, `subtask`, `id`                                  | Mémorise l'ID Monday collé à la main.                                    |

Les descriptions sont courtes et disent **quand** s'en servir (c'est ce que Claude lit pour choisir) ; celles des trois outils soumis à l'autonomie rappellent la règle de confirmation.

## Ce que Claude reçoit

- **Succès** : un texte contenant le même JSON que `kairos --json <commande>` (les types du cœur sérialisés, `state` pour `status`, `text` et `no_id` pour `report`). Un seul bloc de texte par réponse.
- **Erreur métier** : un résultat marqué `isError` dont le texte est `{"error":{"code","message"}}`, avec le message français du cœur (« Chevauche 9 h 00 – 10 h 00 sur 12406850311 : raccourcis cette entrée ou l'autre. »). Claude se corrige.
- **Erreur de protocole** (outil inconnu, paramètre obligatoire absent ou du mauvais type) : une erreur JSON-RPC standard. Le serveur ne s'arrête jamais pour une erreur d'un outil.

## L'autonomie

Soumis à `claude_autonomy` : `delete_entry`, `close_day`, `mark_declared`. Les autres outils ne demandent jamais de confirmation.

| Réglage | Appel sans `confirm: true`                                                                                                                                                                                                                                 | Appel avec `confirm: true`                |
| ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- |
| `ask`   | Rien n'est fait. Réponse (succès, pas erreur) : `{"needs_confirmation":true,"action":"delete_entry","would":"supprimer l'entrée #3 (Front sur 12406850311, 9 h 00 → 10 h 00)","how":"Demande à l'utilisateur, puis rappelle l'outil avec confirm: true."}` | L'action est exécutée.                    |
| `auto`  | L'action est exécutée tout de suite.                                                                                                                                                                                                                       | L'action est exécutée (`confirm` ignoré). |

La phrase `would` décrit précisément ce qui sera fait : l'entrée (numéro, lot, heures) pour `delete_entry` ; la date, le total et le nombre de lignes pour `close_day` ; la date pour `mark_declared`. Quand l'utilisateur dit lui-même « c'est saisi », c'est sa confirmation : Claude rappelle avec `confirm: true`. Le paramètre `confirm` doit être un vrai booléen : `"true"` (texte) est une erreur de protocole.

## Ajouts au cœur

- **Réglage `claude_autonomy`** : valeurs permises `ask` et `auto`, défaut `ask` (migration 2 : `INSERT INTO settings`). `config_set` refuse toute autre valeur (`invalid_setting`). `kairos config claude_autonomy auto` le pose.
- **`Kairos::entry_view(id) -> Result<EntryView>`** : une entrée avec son lot, pour décrire ce que `delete_entry` supprimerait. Lecture seule ; `entry_not_found` si le numéro n'existe pas.

## Branchement

- **Claude Code** : `claude mcp add kairos --scope user -- <chemin>/kairos mcp`. Au 1b, le chemin est celui du binaire compilé (`target/debug/kairos`) ; l'app l'installera dans le PATH à l'étape 2.
- **Claude Desktop** : une entrée `kairos` sous `mcpServers` dans `~/Library/Application Support/Claude/claude_desktop_config.json` (`command` et `args: ["mcp"]`), puis redémarrage.
- **Règle à mettre dans le CLAUDE.md** : nouvelle tâche, `switch` avec son ID ; ID inconnu, le demander sans jamais le deviner ; sous-tâche non précisée, prendre le poste principal ; ne jamais arrêter un lot en fin de réponse.

## Tests

On lance vraiment `kairos mcp` en sous-processus, avec une base temporaire et une heure figée, et on lui parle en JSON-RPC ligne à ligne (`initialize`, `tools/list`, `tools/call`).

- **Liste** : exactement les 14 outils, chacun avec une description non vide et un schéma de paramètres ; aucun outil de réglage.
- **Aller-retour** : `start`, `status`, `pause`, `resume`, `switch`, `stop`, `add_entry`, `edit_entry`, `today`, `report` rendent le JSON de la ligne de commande.
- **Erreurs métier** : deuxième `start` → `isError` avec le code `already_running` et le message français ; la session continue.
- **Autonomie** : en `ask`, `delete_entry`, `close_day` et `mark_declared` sans `confirm` ne changent rien et renvoient `needs_confirmation` avec la bonne phrase `would` ; avec `confirm: true` ils agissent. En `auto`, ils agissent sans `confirm`. `confirm: "true"` est refusé.
- **Source** : un client qui se présente comme `claude-code` donne des lots `claude-code`, un autre `claude-desktop`.
- **Robustesse** : outil inconnu, paramètre absent, paramètre du mauvais type, ligne JSON invalide, réponse à une requête annulée : le serveur répond proprement et reste en vie ; la fermeture de l'entrée le termine sans plantage ; la sortie standard ne contient aucune ligne qui ne soit pas du protocole.
- **Base** : verrouillée plus de 5 secondes → `db_unavailable` ; plus récente → `db_newer` ; les deux, en résultat d'erreur, jamais en plantage.
- **Deux serveurs** sur la même base : un seul `start` gagne.
- **Cœur** : `claude_autonomy` (valeurs, défaut, refus), `entry_view` (entrée existante, numéro inconnu).

## Cas limites qui doivent être traités (et testés)

1. L'entrée se ferme **au milieu** d'un appel : le serveur termine l'appel en cours si possible, sinon s'arrête sans corrompre la base (chaque appel est une transaction).
2. Un nom de tâche de 500 caractères ou avec retour à la ligne : le cœur le refuse (`invalid_name`), le serveur le rend comme une erreur métier.
3. Deux appels d'outil **rapprochés** (Claude en enchaîne plusieurs) : traités l'un après l'autre, dans l'ordre reçu.
4. `delete_entry` sur un numéro inconnu en mode `ask` : `entry_not_found` tout de suite, sans demander de confirmer une suppression impossible.
5. Un client qui n'annonce pas de nom : la source est `claude-desktop`.

## Ordre de travail (esquisse, le plan le détaille)

1. Cœur : réglage `claude_autonomy` (migration 2) et `entry_view`.
2. Squelette de `kairos mcp` : `initialize`, `tools/list`, arrêt propre, sortie standard réservée au protocole.
3. Les outils de lecture et de chronomètre (`status`, `today`, `report`, `start`, `stop`, `pause`, `resume`, `switch`).
4. Les outils de correction (`add_entry`, `edit_entry`, `set_subtask_id`).
5. Les trois outils soumis à l'autonomie.
6. Tests de robustesse, de concurrence, documentation (README, page Notion « Claude et Kairos »).
