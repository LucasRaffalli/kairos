# Kairos, étape 2a : le chronomètre dans la barre de menus

Date : 24 octobre 2026. Source de vérité du produit : la page Notion « Kairos » (cahier des charges F1 à F7, F18, F35, F37) et les maquettes Figma « Kairos · v2 » (fichier `st5ct8bxzDkzwnY1UEH1YS`). Ce document ne décrit que le sous-projet **2a**. Il s'appuie sur les étapes 0 à 1c, sur `main` et publiées jusqu'à la 0.2.0.

## But

Une vraie app qui chronomètre depuis la barre de menus, sur **la même base** que la ligne de commande et Claude : ce que fait Claude apparaît dans l'app dans la seconde, et inversement. L'app lance, met en pause, reprend, arrête et change de lot, avec les mêmes règles, les mêmes refus et les mêmes messages que partout.

Hors 2a, chacun a son sous-projet :

- **2b, la journée** : modifier, ajouter, supprimer une entrée, clôturer, le texte à copier, « saisie » et « à resaisir », le temps sans ID, l'arrondi. En 2a, le bouton calendrier de l'en-tête et le lien « Clôturer la journée » ne sont **pas affichés**.
- **2c, les réglages** : toute la fenêtre Réglages (donc l'interrupteur d'intégration IA, désactivé par défaut), les raccourcis globaux, les tâches parents épinglées, le premier lancement, l'onglet Mises à jour. En 2a, l'engrenage et l'indication « ⌘K » du pied ne sont **pas affichés**.
- **2d, présence et notifications** : la minuterie (heure de fin, « Fin dans 30 min, 1 h, 2 h », la sonnerie, l'écran « fin atteinte »), les absences, le lot oublié, la veille, les quatre notifications. En 2a, les champs **Fin** et « Fin dans… » du formulaire sont **cachés**.
- **2e, projets multiples**, plus tard.

## Décisions prises

- **Le pont app ↔ base passe par des commandes Tauri qui appellent `kairos-core` directement.** Pas de processus `kairos` lancé à chaque clic, pas de connexion longue : une base ouverte par appel, comme le MCP.
- **Aucune action ne ferme le popover.** Lancer, changer de lot, reprendre, arrêter, mettre en pause, une erreur : le popover reste ouvert et montre le nouvel état. Il ne se ferme que si on clique ailleurs (comportement habituel d'un popover macOS, déjà en place) ou avec Échap.
- **Les heures de fin sont cachées** jusqu'à 2d.
- **Le dossier de travail est enregistré** (voir « Ajouts au cœur ») pour afficher « Lancé par Claude Code à 13:34 · ~/Beeside/main ».
- **L'interrupteur d'intégration IA n'est pas dans 2a.** Il arrive avec les réglages (2c).

## Ajouts au cœur

1. **Migration 3 : `entries.source` et `entries.workdir`** (deux colonnes texte, vides pour les entrées existantes). La source d'un lot (`lots.source`) est celle de sa création ; « lancé par » doit dire qui a lancé **cette entrée**. `workdir` est le dossier de travail au lancement.
2. `StartArgs.workdir: Option<String>` : écrit dans l'entrée créée par `start`, `switch_to` et `back`. Une entrée créée par `resume` ou par la coupure de minuit **hérite** de la source et du dossier de l'entrée qu'elle continue.
3. `Entry.source: Option<Source>` et `Entry.workdir: Option<String>` (sérialisés dans le JSON) ; `Status::Running` et `Status::Paused` portent `launched_by: Option<Source>` et `workdir: Option<String>` (ceux de l'entrée ouverte, ou de la dernière entrée pour une pause).
4. `Kairos::subtask_choices(parent_monday_id: Option<&str>) -> Result<SubtaskChoices>` avec `SubtaskChoices { main: String, base: Vec<String>, own: Vec<String> }` : le poste principal, les sous-tâches de base, celles propres à ce parent (vides si le parent est inconnu). Lecture seule.
5. `Db::data_version(&self) -> Result<i64>` : la valeur de `PRAGMA data_version`, qui change quand **une autre connexion** modifie la base.
6. **Qui remplit `workdir`** : le hook (le `cwd` que Claude Code envoie), et la ligne de commande (`kairos start`, `switch`, `back` : le dossier courant). Le MCP et l'app n'en ont pas : le champ reste vide et l'écran n'affiche alors que « Lancé par … à … ».

## Le pont (Rust, `src-tauri`)

Des commandes fines, qui ouvrent la base, appellent le cœur et rendent ses types ; une erreur sort sous la forme `{ "code", "message" }` (le message français du cœur, tel quel).

| Commande                            | Rôle                                                                                                                                                                                                                                                   |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `snapshot()`                        | Tout ce qu'il faut pour dessiner : `status`, `today` (les entrées du jour), `recent` (jusqu'à 8 lots récents), `choices` (`SubtaskChoices` du lot en cours ou du poste principal), `server_time` (secondes). Un seul appel, donc pas de scintillement. |
| `subtask_choices(parent)`           | Les puces de sous-tâche quand on change de parent dans le formulaire.                                                                                                                                                                                  |
| `start(request)`, `switch(request)` | `request` : `parent_monday_id`, `parent_label`, `name`, `subtask`, `subtask_monday_id`, `at` (secondes, calculées par l'interface depuis « HH:MM »). Source `manual`, pas de dossier.                                                                  |
| `stop()`, `pause()`, `resume()`     | Comme dans le cœur.                                                                                                                                                                                                                                    |
| (supprimée) `set_popover_height`    | Remplacée par une hauteur fixe, voir l'amendement du 25 octobre.                                                                                                                                                                                       |
| `quit(stop_lot: bool)`              | Quitte l'app, en arrêtant d'abord le lot si demandé.                                                                                                                                                                                                   |

Le temps est celui du système (`SystemClock`) ; l'interface formate les heures dans le fuseau du système, comme le cœur.

## Rester synchronisé

- **Côté Rust**, un fil ouvre une connexion et lit `Db::data_version` chaque seconde. Quand la valeur change (la ligne de commande, Claude, ou l'app elle-même), il émet l'événement `kairos://changed` ; si la base est verrouillée ou illisible, il réessaie à la seconde suivante sans bruit.
- **Côté interface**, l'événement déclenche un `snapshot`. Le chrono grandit par un minuteur local d'une seconde calculé depuis l'heure de départ (corrigée de l'écart entre `server_time` et l'horloge du navigateur) : aucun aller-retour par seconde.
- **Barre de menus** : le titre de l'icône est remis à jour à chaque `kairos://changed` et à chaque minute pile.

## Les écrans (Angular)

Trois vues dans le popover (340 px de large, reprises des maquettes `220:81`, `220:179`, `223:304`), un bandeau d'erreur, et le dialogue « Quitter ». L'interface suit la maquette ; les valeurs (couleurs, rayons, espacements, polices) viennent des tokens déjà exportés (`src/styles/_tokens.scss`), pas du code généré par Figma.

- **Lancer** (rien en cours). En-tête : logo, « Kairos », « lun. 20 oct. · rien en cours ». Trois blocs numérotés :
  1. **Tâche parent** : champs « Nom » et « ID Monday », et les « Récents » (pastilles ID + nom, issues de `recent`) qui remplissent les deux champs.
  2. **Sous-tâche** : les puces (socle et propres au parent, la première sélectionnée = le poste principal), « + Autre », puis « Nom » et « ID Monday (facultatif) ».
  3. **Temps** : « Début » facultatif (`HH:MM`).

  Puis le récapitulatif « Les heures iront sur <nom> › <sous-tâche> · Chrono libre : jusqu'à ce que tu l'arrêtes », le bouton **Lancer** (⌘↵), la ligne « Ou depuis Claude : kairos start » et la liste **Reprendre** (les trois lots les plus récents, un clic les lance sur-le-champ).

- **Minuteur lancé** (un lot tourne). Le chrono `HH:MM:SS`, le nom du lot, le badge de sous-tâche en capitales, « parent <ID> · sous-tâche <ID> », la ligne « Lancé par <qui> à <heure> · <dossier> » (« par toi » devient « Lancé à 13:34 » sans « par »), puis **Arrêter** (⌘S), **Pause** et ⇄ (ouvre « Changer de lot »). En dessous « AUJOURD'HUI · 3 h 54 » et la liste des entrées du jour, **en lecture seule** (modifier est en 2b) : un carré de la couleur du parent, le nom, « ID · sous-tâche · début → fin », la durée (celle de l'entrée en cours en vert).
- **En pause** : le chrono figé, la mention « En pause depuis <heure> », **Reprendre** à la place de Pause ; mêmes boutons Arrêter et ⇄. (Le nœud Figma de cet état est à relever avant l'implémentation.)
- **Changer de lot** : au-dessus du formulaire « Lancer », un rappel du lot en cours (nom, ID, « s'arrête au changement », chrono) ; le bouton s'appelle **Changer de lot** et la ligne dit « Le nouveau lot démarre à la même seconde ».
- **Erreur** : un bandeau rouge sous l'en-tête avec le message du cœur et **Réessayer** (« Le lot n'a pas démarré : la base de Kairos est inaccessible… »). Les erreurs de champ du cœur (`invalid_monday_id`, `invalid_name`, `in_future`, `same_lot`…) s'affichent sous le champ concerné quand on peut le déterminer, sinon dans le bandeau.
- **Validation** : il faut un ID Monday **ou** un nom de tâche (maquette `223:399`), sinon « Lancer » est inactif avec l'explication.
- **Reprendre** et **Lancer** ne ferment jamais le popover : après l'action, la vue passe à « Minuteur lancé ».

Composants communs à construire une fois (section « Kairos · composants » de Figma) : Bouton (variantes primaire, secondaire, danger), Champ de saisie, Puce et Puce de choix, Pastille récente, Reprise rapide, Titre de section numéroté, Alerte, En-tête et pied de popover, Badge, Bascule (pour 2c).

La date de l'en-tête est celle de l'ordinateur. Tous les textes passent par `I18nService` (fr et en), comme l'existant.

## Raccourcis dans le popover

⌘↵ lance (ou change de lot), ⌘S arrête (⌘. et, sur AZERTY, ⌘⇧ + la touche du point aussi), Échap ferme. Les raccourcis **globaux** (ouvrir depuis n'importe où, lancer, pause) sont en 2c.

## Fenêtre

340 px de large, 560 px de haut (fixe, voir l'amendement), avec défilement interne des zones longues. Le popover se place sous l'icône (`placement.rs`, existant). Perdre le focus le ferme, comme aujourd'hui ; le brouillon du formulaire (champs, sous-tâche choisie) est gardé en mémoire de l'interface pour la réouverture.

### Amendement du 25 octobre

Après un essai en local, Lucas a choisi une **fenêtre qui reste ouverte** à la place du popover qui se ferme sur perte de focus. Ce qui change dans cette section :

- C'est une vraie fenêtre macOS opaque (`titleBarStyle: "Overlay"`, titre caché, `alwaysOnTop: false`, toujours sans Dock) avec les boutons natifs en haut à gauche. Une fine barre de 28 px (`data-tauri-drag-region`) sert à la déplacer.
- Rouge : range la fenêtre dans la barre de menus, l'app continue de tourner, la fenêtre n'est jamais détruite. Jaune : désactivé (Tauri 2.12 n'émet aucun événement de réduction à intercepter, et sans Dock une fenêtre réduite n'aurait nulle part où aller). Vert : désactivé (largeur fixe, hauteur qui suit le contenu).
- Elle ne se ferme plus à la perte du focus. Seuls le bouton rouge, Échap, ⌘W (« Fermer la fenêtre » du menu de l'app, qui masque) et un clic sur l'icône quand elle est au premier plan la rangent. Aucune action du chronomètre ne la ferme.
- Clic sur l'icône : fenêtre rangée, on l'affiche ; visible mais derrière d'autres fenêtres, on la ramène devant ; visible et au premier plan, on la range.
- Position : centrée sous l'icône au tout premier affichage, puis gardée en mémoire pour la session (rien sur disque).
- Le titre de l'icône affiche les secondes (`H:MM:SS`), à la demande de Lucas : une durée à la minute donnait l'impression d'un compteur bloqué. Le réglage « Durée dans la barre de menus » (2c) pourra l'adapter.
- Réglage interne `menu_title_seconds` (`true` par défaut, `false` pour `H:MM`), dans la table `settings` du cœur et modifiable par `kairos config menu_title_seconds <true|false>`. Sans ligne en base, la valeur est le défaut (aucune migration : les anciennes versions restent compatibles) ; une valeur invalide retombe sur le défaut côté app et est refusée par `config`. L'écran Réglages (2c) l'exposera.
- **Hauteur fixe de 560 pt** pour toutes les vues (largeur 340 pt inchangée), au lieu de la hauteur qui suivait le contenu. Une seule source : `height` de la fenêtre dans `tauri.conf.json` ; le style prend `100vh`. Le premier affichage la ramène à l'écran moins 80 px sur un écran trop petit. L'en-tête et les boutons restent en place ; seules la liste du jour, le formulaire de lancement et « Reprendre » défilent. La commande `set_popover_height` et la mesure de hauteur côté interface sont supprimées.
- Les zones qui défilent ont une barre fine de 4 px (pseudo-éléments WebKit seulement : `scrollbar-width` et `scrollbar-color` faisaient revenir la barre native, énorme) et un fondu (directive `kScrollFade`, masque) du côté où il reste du contenu.
- **⌘S arrête le lot** (second raccourci, affiché sur le bouton « Arrêter ») : ⌘. demande Maj sur un clavier AZERTY et est aussi un raccourci système de macOS. Mêmes garde-fous que ⌘. : un lot tourne ou est en pause, pas de dialogue « Quitter » ouvert, vue non grisée ; ni Maj, ni Ctrl, ni Alt avec ⌘S.
- **Raccourcis globaux en accord** (`tauri-plugin-global-shortcut`, module `chord.rs`) : ⌘K, puis dans les 2 secondes S (arrêter), ↵ (relancer le dernier lot, comme « Reprendre » ; sans lot récent ou si un lot tourne, la fenêtre s'ouvre sans rien changer), P (pause ou reprise), Échap (annuler). Machine d'états pure et testée ; ⌘K enregistré en permanence, les quatre autres touches seulement pendant l'armement et toujours rendues (échec d'armement, expiration, annulation, désactivation). Touches physiques : mêmes places en AZERTY et QWERTY. Les actions passent par les fonctions du pont et `InFlight`. Titre « ⌘K… » pendant l'attente. Réglage interne `global_shortcuts` (`true` par défaut), relu avec le statut et appliqué sans relance.

## La barre de menus

Le titre à côté de l'icône : la durée du lot en cours en `H:MM:SS`, « Pause · H:MM:SS » en pause, rien au repos. Les glyphes précis par état et les états « journée à clôturer » et « à resaisir » sont lus sur les maquettes `231:115`, `231:140` ; ces deux derniers arrivent avec 2b. La couleur ne porte jamais seule un état : la forme et le texte aussi.

## Quitter

« Quitter Kairos » (menu du clic droit sur l'icône, ou ⌘Q) : sans lot en cours, l'app quitte. Avec un lot en cours, le popover s'ouvre sur le dialogue de la maquette `230:456` : **Arrêter le lot et quitter** (rouge), **Quitter, garder le chrono** (le lot reste ouvert dans la base, l'heure continue d'être comptée), **Annuler**.

## Cas limites à traiter (et tester)

1. Un lot lancé par la ligne de commande ou par Claude pendant que le popover est ouvert : la vue change en moins de 2 secondes.
2. Base verrouillée, ou plus récente que l'app (`db_unavailable`, `db_newer`) : la dernière vue connue reste affichée, grisée, avec le bandeau et **Réessayer**.
3. Lancer deux fois très vite (double clic) : un seul lot, l'autre appel est refusé par le cœur (`already_running`) et son message n'est pas montré comme une erreur si l'état affiché est déjà le bon.
4. Lancer avec un début dans le futur, ou plus vieux que 2 jours : le refus du cœur s'affiche sous le champ « Début ».
5. Un nom de parent ou de sous-tâche de 500 caractères ou sur plusieurs lignes : le refus `invalid_name` s'affiche sous le champ ; à l'écran, les noms longs sont coupés par « … » avec le texte complet en infobulle.
6. Passage de minuit avec un lot en cours : le chrono affiche la durée de l'entrée ouverte (le cœur la coupe à minuit), il repart donc de 00:00:00 ; c'est assumé.
7. Changement d'heure : les durées viennent du cœur, qui les calcule à partir d'instants absolus.
8. L'app lancée deux fois : la seconde se ferme et rouvre la première (`tauri-plugin-single-instance`).
9. Plus de 8 lots récents, ou aucun : « Reprendre » et « Récents » se masquent quand ils sont vides.
10. Quitter pendant une action en cours : l'action se termine avant la fermeture (une écriture n'est jamais coupée en deux : chaque appel est une transaction).
11. Lot sans ID Monday (nom seul) : le récapitulatif, le chrono et la liste du jour montrent le nom, sans « parent … ».
12. Plusieurs écrans (écran externe) : le popover se place dans l'écran de l'icône (existant).

## Tests

- **Cœur** : migration 3 (base vide, base 0.2.0 existante, entrée coupée à minuit qui hérite), `workdir` écrit par `start`/`switch_to`/`back`, hérité par `resume`, `launched_by` dans `status`, `subtask_choices` (parent connu, inconnu, sans ID), `Db::data_version` qui change après une écriture d'une autre connexion et pas après une lecture.
- **Hook et ligne de commande** : le hook écrit le `cwd`, `kairos start` écrit le dossier courant, le MCP n'écrit rien.
- **Pont** : les fonctions que les commandes appellent sont testées sans fenêtre (`snapshot`, `start` avec refus du cœur renvoyé en `{code, message}`, `stop`, `pause`, `resume`, `quit(stop_lot)`).
- **Interface, logique pure en TypeScript** : formats de durée, de date et d'« il y a », libellé du récapitulatif, choix de la vue selon l'état, validation du formulaire, règle « aucune action ne ferme le popover », conversion « HH:MM » → secondes (dont le passage de minuit et le futur).
- **Interface, rendu** : un faux pont avec des états-types (rien en cours, lot en cours, en pause, erreur, base verrouillée, lot sans ID, noms longs, aucun récent) affiche chaque écran dans le navigateur ; chacun est comparé à la maquette Figma correspondante à 340 px, dans les thèmes Sombre et Clair et avec l'accent par défaut.
- **De bout en bout** : un lot lancé par `kairos start` apparaît dans le popover ; un lot lancé dans le popover se voit avec `kairos status`.

## Ordre de travail (esquisse, le plan le détaille)

1. Cœur : migration 3, `workdir`, `launched_by`, `subtask_choices`, `Db::data_version`, hook et CLI qui écrivent le dossier.
2. Pont Rust : commandes, surveillance de `data_version`, titre de l'icône, dialogue « Quitter », hauteur de la fenêtre (fixe), instance unique.
3. Couche Angular : l'interface du pont (réelle et fausse), le magasin d'état, les formats et la logique pure, avec leurs tests.
4. Composants communs, d'après les maquettes de composants.
5. Les vues : Lancer, Minuteur lancé, En pause, Changer de lot, erreur, Quitter ; câblage et raccourcis.
6. Vérification contre Figma, puis essai dans la vraie app (`npm run dev`), de bout en bout avec la ligne de commande.
