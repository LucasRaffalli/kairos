# Kairos, étape 1a : le cœur et la ligne de commande

Date : 23 octobre 2026. Source de vérité du produit : la page Notion « Kairos » (cahier des charges, concept, règles). Ce document ne décrit que le sous-projet **1a** ; le serveur MCP (1b) et les hooks de Claude Code (1c) viennent ensuite et s'appuient sur lui.

## But

`kairos-core` porte **toutes** les règles métier ; le binaire `kairos` les expose en ligne de commande. À la fin du 1a, on peut chronométrer une journée entière depuis le terminal, la corriger, la clôturer et obtenir la ligne à saisir dans Monday, sans l'app ni Claude. Les deux autres sous-projets (MCP, hooks) et l'app Tauri n'ajouteront aucune règle : ils appellent le cœur.

Hors 1a : détection d'absence, veille, minuterie (signaux de l'app, étape 2), serveur MCP (1b), hooks et détection du lot (1c), gestion de plusieurs projets en ligne de commande, vue Semaine et export CSV (étape 3).

## Décisions prises

- **Découpe à minuit** : une entrée ne traverse jamais minuit. Un lot encore ouvert à 00:00 voit son entrée fermée à 23:59:59 (heure locale) et une nouvelle s'ouvrir à 00:00 sur le même lot. La déclaration Monday est par jour, chaque jour doit être exact.
- **Modèle B** : des entrées avec début et fin, les règles vérifiées dans une transaction. Une correction est une simple modification.
- **Une pause ferme l'entrée courante, une reprise en ouvre une nouvelle** sur le même lot : les pauses d'une journée se déduisent des trous.

## Dépendances

`rusqlite` (SQLite embarqué, `bundled`), `chrono`, `thiserror`, `serde` et `serde_json` dans `kairos-core` ; `clap` (déjà présent) dans `kairos-cli`. Aucune autre.

## Modules de `kairos-core`

| Module     | Rôle                                                                                                                                                                                                                                                      |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `clock`    | `trait Clock { fn now(&self) -> i64; fn utc_offset(&self, at: i64) -> i32 }` (secondes depuis l'epoch et décalage local en secondes) ; `SystemClock` (heure du système, décalage local réel) ; `FakeClock` (heure et décalage réglables, pour les tests). |
| `model`    | Les types : `Parent`, `Subtask`, `Lot`, `Entry`, `Day`, `DayStatus`, `Status`, `StartArgs`, `Report`.                                                                                                                                                     |
| `db`       | `Db::open(path)` : crée le dossier, ouvre SQLite en mode WAL, `foreign_keys=ON`, `busy_timeout` 5 000 ms, applique les migrations (`PRAGMA user_version`).                                                                                                |
| `timeline` | Fonctions **pures** sur des intervalles `[début, fin)` : chevauchement, découpe d'un intervalle aux minuits, bornes d'un jour local, durée par jour. Aucun accès à la base.                                                                               |
| `service`  | `Kairos<C: Clock>` : toutes les opérations (voir plus bas), chacune dans une transaction `BEGIN IMMEDIATE`.                                                                                                                                               |
| `report`   | Calcul de la ventilation, du recoupement, du texte à saisir, du diff d'une journée à resaisir.                                                                                                                                                            |
| `error`    | `Error` (`thiserror`) : un variant par cas métier, avec un message français qui dit **comment corriger** ; `code()` donne un identifiant stable (`already_running`…).                                                                                     |

Le chemin de la base : `~/Library/Application Support/com.aegis.kairos/kairos.db` ; la variable d'environnement `KAIROS_DB` le remplace (tests, essais).

## Données (SQLite)

Les instants sont des entiers (secondes depuis l'epoch, UTC) ; les dates de journée sont `AAAA-MM-JJ` en heure locale.

- **projects** : `id`, `name`, `is_default`. Un projet par défaut, « Mon projet », est créé par la migration 1. Tous les lots du 1a lui appartiennent.
- **parents** : `id`, `project_id`, `monday_id` (texte, chiffres, unique par projet, nullable), `label` (nom affiché, facultatif), `pinned`, `archived`, `last_seen`.
- **subtasks** : `id`, `project_id`, `parent_id` (nullable : vide = sous-tâche de base, proposée partout), `name`, `monday_item_id` (nullable), `is_base`. Unique sur `(project_id, parent_id, name)`.
- **lots** : `id`, `project_id`, `parent_id` (nullable), `subtask_id` (nullable), `name` (nullable : nom de la tâche quand il n'y a pas d'ID), `source` (`manual`, `claude-code`, `claude-desktop`). Un lot est unique sur `(project_id, parent_id, subtask_id, name)` : relancer la même chose réutilise le lot (la « reprise rapide »).
- **entries** : `id`, `lot_id`, `start`, `end` (nullable : entrée ouverte), `removed` (0 ou 1 : temps retiré, réservé à l'absence de l'étape 2, exclu de tous les totaux), `note`. Contraintes : `end > start` ; au plus **une** entrée ouverte (index unique partiel) ; jamais à cheval sur minuit.
- **days** : `date` (clé), `status` (`open`, `closed`, `declared`, `to_redeclare`), `closed_at`, `declared_at`, `closed_totals` et `declared_totals` (JSON : totaux par parent et sous-tâche au moment de l'événement).
- **state** : une seule ligne : `paused_lot_id` (nullable).
- **settings** : `key`, `value`. Clés du 1a : `main_subtask` (défaut `Front`), `rounding_minutes` (défaut `0`, valeurs permises `0`, `5`, `10`, `15`).

## Règles

1. **Un seul lot en cours.** Démarrer pendant qu'un lot tourne est refusé (`kairos switch` existe pour ça).
2. **Jamais de chevauchement.** Les bornes qui se touchent (fin d'une entrée = début d'une autre) sont permises. L'entrée ouverte compte comme `[début, maintenant)`.
3. **Pas dans le futur.** Un début ou une fin après « maintenant » est refusé.
4. **Pas à cheval sur minuit** pour une entrée ajoutée ou modifiée à la main : refus avec la proposition de la scinder. Les entrées automatiques sont, elles, coupées (règle de découpe ci-dessous).
5. **Découpe à minuit.** À chaque opération, avant de faire quoi que ce soit, `reconcile(now)` ferme l'entrée ouverte à chaque minuit local franchi et en ouvre une nouvelle, sur le même lot, au minuit suivant jusqu'au jour courant. Si l'app est restée fermée trois jours, trois jours sont créés. Les limites d'un jour sont calculées avec le décalage local **à cet instant** (jours de 23 ou 25 heures au changement d'heure).
6. **Référence d'un lot.** Un lot porte un ID Monday (chiffres seulement, 1 à 20) **ou** un nom de tâche (1 à 120 caractères), jamais aucun des deux. Avec un ID Monday, la sous-tâche est obligatoire : à défaut, c'est le poste principal. Sans ID, la sous-tâche est facultative.
7. **Sous-tâches propres à un parent.** Une sous-tâche inconnue d'un parent est créée pour **ce** parent (`is_base = 0`) ; elle n'apparaît pas pour les autres parents. Les sous-tâches de base sont `Back`, `Front`, `Recettage`, `Maquette`, créées par la migration 1.
8. **Pause et reprise.** `pause` ferme l'entrée et mémorise le lot (`paused_lot_id`) ; `resume` ouvre une entrée sur ce lot. `start`, `switch` et `stop` effacent la pause. `pause` sans lot en cours et `resume` sans pause sont des erreurs claires.
9. **Changer de lot** ferme l'entrée courante et ouvre la nouvelle **à la même seconde**, dans la même transaction (jamais de trou ni de chevauchement, même si la seconde crash).
10. **Heure de début en retard** (`--at`) : le lot démarre à cet instant s'il ne chevauche aucune entrée ; sinon refus.

### États d'une journée

```
open ──close_day──▶ closed ──mark_declared──▶ declared
                      ▲                          │ (entrée ajoutée, modifiée ou supprimée)
                      │                          ▼
                      └────mark_declared──── to_redeclare
```

- `close_day` : refusée si la journée n'est plus ouverte, ou si un lot est en cours (le message propose `--stop` qui l'arrête d'abord). Fige les totaux (`closed_totals`).
- `mark_declared` : refusée sur une journée ouverte (« clôture d'abord »). Fige `declared_totals`.
- Une entrée ajoutée, modifiée ou supprimée sur une journée `declared` la passe à `to_redeclare` ; sur une journée `closed`, elle reste `closed` et ses totaux figés sont recalculés ; sur une journée `open`, rien ne change d'état.
- Un `report` sur `to_redeclare` renvoie aussi la liste des changements (`Front 2 h 10 → 2 h 25`) calculée entre `declared_totals` et la réalité.

## Ventilation, recoupement et texte à saisir

Pour une journée, `report` calcule :

- **Bornes** : début (début de la première entrée), fin (fin de la dernière, ou « maintenant » si ouverte), pauses (somme des trous entre entrées), total (somme des durées des entrées non retirées).
- **Lignes** : une par parent (ID Monday) avec ses sous-tâches et leurs durées, dans l'ordre de première apparition ; tout ce qui n'a pas d'ID est regroupé en **sans ID** (avec la liste des noms de tâche dans la sortie structurée).
- **Arrondi** : si `rounding_minutes` > 0, chaque ligne parent × sous-tâche est arrondie à la valeur la plus proche (la moitié vers le haut), jamais en dessous de la valeur de pas pour une durée non nulle.
- **Recoupement** : `écart = (fin − début − pauses) − somme des lignes`. Zéro sans arrondi ; avec arrondi il peut être non nul, et il est **signalé**, jamais caché.
- **Texte** : `12406850311 : Front 2 h 10 · Back 1 h 44 · 12406856533 : Recettage 1 h 05 · Retour icône mal faite 0 h 43 · sans ID : 1 h 51`. Durées au format `H h MM`.

## Opérations de `service`

Chaque opération prend `&self` (le service possède la connexion) et renvoie `Result<_, Error>`.

| Opération                                                                                | Rôle                                                                                   |
| ---------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `start(StartArgs) -> StartedLot`                                                         | Règles 1, 3, 6, 7, 10.                                                                 |
| `stop(at: Option<i64>) -> StoppedEntry`                                                  | Ferme l'entrée ouverte ; efface la pause ; erreur si rien ne tourne ni n'est en pause. |
| `pause()` / `resume()`                                                                   | Règle 8.                                                                               |
| `switch_to(StartArgs) -> SwitchResult`                                                   | Règle 9.                                                                               |
| `status() -> Status`                                                                     | `Idle`, `Running { lot, since, duration, today_total }` ou `Paused { lot, since }`.    |
| `today(date: Option<String>) -> DayView`                                                 | Les entrées du jour, leurs lots, les bornes.                                           |
| `add_entry(AddEntry) -> Entry`, `edit_entry(id, EditEntry) -> Entry`, `delete_entry(id)` | Règles 2, 3, 4, 6 ; mettent à jour l'état de la journée.                               |
| `report(date) -> Report`                                                                 | Ventilation, recoupement, texte, changements.                                          |
| `close_day(date, stop_running: bool)` / `mark_declared(date)`                            | États d'une journée.                                                                   |
| `set_subtask_id(parent_monday_id, subtask, monday_item_id)`                              | Mémorise l'ID collé à la main ; chiffres seulement.                                    |
| `config_get(key)` / `config_set(key, value)`                                             | `main_subtask`, `rounding_minutes`.                                                    |

Les types d'arguments (`StartArgs`, `AddEntry`, `EditEntry`) sont des structures avec des champs facultatifs ; leurs champs exacts sont fixés par le plan.

## Erreurs

Chaque erreur a un `code` stable et un message français qui explique **quoi faire**. Exemples (le texte exact est celui des tests) :

- `already_running` : « Un lot est déjà en cours : Front sur 12406850311 depuis 13 h 34. `kairos switch` l'arrête et lance le suivant. »
- `nothing_running` : « Aucun lot en cours. »
- `overlap` : « Chevauche 14 h 10 – 15 h 02 sur 12406850311 : raccourcis cette entrée ou l'autre. »
- `in_future` : « 17 h 40 est dans le futur (il est 15 h 02). »
- `crosses_midnight` : « Cette entrée traverse minuit : scinde-la en deux, une par jour. »
- `reference_missing` : « Il faut un ID Monday ou un nom de tâche. »
- `invalid_monday_id` : « L'ID Monday s'écrit avec des chiffres seulement (par exemple 12406850311). »
- `day_not_open`, `day_not_closed`, `running_lot_on_close`, `entry_not_found`, `nothing_to_resume`, `invalid_setting`.
- `db_newer` : « Cette base a été créée par une version plus récente de Kairos : mets Kairos à jour. » (`user_version` supérieur à celui connu : on n'écrit jamais dedans.)
- `db_unavailable` : « La base est verrouillée par un autre processus depuis plus de 5 secondes. » (`busy_timeout` dépassé.)

## La ligne de commande `kairos`

Sortie en français, lisible, par défaut ; `--json` donne une structure stable (la même que celle que le 1b et le 1c consommeront). Les erreurs sortent avec le code 2 et, en `--json`, un objet `{"error":{"code","message"}}`.

| Commande                    | Rôle                                                                                                                                      |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `start`                     | `--parent <ID>`, `--name <NOM>`, `--subtask <NOM>`, `--subtask-id <ID>`, `--at <HH:MM>`, `--source <manual\|claude-code\|claude-desktop>` |
| `switch`                    | mêmes arguments que `start`                                                                                                               |
| `stop`                      | `--at <HH:MM>`                                                                                                                            |
| `pause`, `resume`, `status` |                                                                                                                                           |
| `today`, `report`           | `--date <AAAA-MM-JJ>` (aujourd'hui par défaut)                                                                                            |
| `close-day`                 | `--date`, `--stop`                                                                                                                        |
| `mark-declared`             | `--date`                                                                                                                                  |
| `add`                       | `--parent`, `--name`, `--subtask`, `--from <HH:MM>`, `--to <HH:MM>`, `--date`                                                             |
| `edit <id>`                 | `--from`, `--to`, `--parent`, `--name`, `--subtask`                                                                                       |
| `delete <id>`               |                                                                                                                                           |
| `set-subtask-id`            | `--parent <ID>`, `--subtask <NOM>`, `--id <ID Monday>`                                                                                    |
| `config <clé> [valeur]`     | lit ou écrit `main_subtask`, `rounding_minutes`                                                                                           |
| `version`                   | existe déjà                                                                                                                               |

Les heures s'écrivent `13:30` (heure locale, jour courant ou `--date`), ou `AAAA-MM-JJTHH:MM`, ou RFC 3339. Un lot qui tourne depuis plusieurs minutes se voit avec `kairos status`.

## Concurrence

La ligne de commande, l'app (étape 2) et le serveur MCP (1b) peuvent écrire en même temps dans la même base. Garanties : une transaction `BEGIN IMMEDIATE` par opération, `busy_timeout` 5 000 ms, WAL. Deux `kairos start` simultanés : un seul gagne, l'autre reçoit `already_running`, jamais deux entrées ouvertes (l'index unique partiel est le dernier rempart).

## Tests

- **Horloge simulée et base temporaire** pour chaque test ; aucun test n'attend l'heure réelle.
- **`timeline`** (fonctions pures) : chevauchement avec bornes qui se touchent, entrées identiques, entrée contenue dans une autre, découpe d'un intervalle sur 1, 2 et 3 minuits, jours de 23 h et de 25 h (changement d'heure, décalage modifié par l'horloge simulée), minuit pile.
- **`service`** : chaque règle (1 à 10) a son test, positif et négatif ; chaque transition d'état d'une journée ; une entrée retirée n'entre dans aucun total ; relancer le même lot réutilise le lot ; une sous-tâche propre à un parent n'apparaît pas chez un autre parent.
- **`report`** : la ligne d'exemple ci-dessus exactement ; l'arrondi (5, 10, 15 minutes) et l'écart signalé ; « sans ID » ; l'ordre de première apparition ; le diff `Front 2 h 10 → 2 h 25`.
- **Base** : ouverture sur un dossier inexistant (créé), base plus récente refusée sans écriture, deux processus qui écrivent en même temps (un `start` gagne, l'autre est refusé proprement), nom avec accents, guillemets et emoji, ID Monday de 21 chiffres refusé.
- **CLI** : une batterie de tests d'intégration sur le binaire (`start`, `status`, `pause`, `resume`, `switch`, `stop`, `report`, `close-day`, `mark-declared`, erreurs en texte et en `--json`, code de sortie 2).

## Cas limites qui doivent être traités (et testés)

1. Un lot ouvert la nuit puis relancé **plusieurs jours après** : une entrée par jour, pas une seule de 72 heures.
2. Un `start --at` dont l'heure tombe **pile sur la fin** d'une entrée existante : accepté (bornes qui se touchent).
3. `close-day` sur une journée **sans aucune entrée** : message clair, aucune journée créée.
4. Un `switch` quand **rien ne tourne** : se comporte comme `start`, sans erreur.
5. Un nom de tâche avec **des espaces autour** ou sur plusieurs lignes : rogné et refusé s'il reste vide.

## Ordre de travail (esquisse, le plan le détaille)

1. `clock` et `timeline` (pur, testé).
2. `db` et migrations, puis `model` et `error`.
3. `service` : démarrer, arrêter, pause, reprise, changement de lot, état.
4. `service` : entrées manuelles, journée, états.
5. `report` : ventilation, arrondi, recoupement, texte, diff.
6. La ligne de commande, texte et `--json`.
7. Tests de concurrence et de base ancienne ou plus récente ; mise à jour du README et de Notion.
