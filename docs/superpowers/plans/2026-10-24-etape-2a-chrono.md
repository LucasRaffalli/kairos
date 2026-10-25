# Étape 2a : le chronomètre dans la barre de menus, plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Une app de barre de menus qui lance, met en pause, reprend, arrête et change de lot sur la même base que la ligne de commande et Claude, avec les trois écrans de la maquette Figma, sans qu'aucune action ne ferme le popover.

**Architecture:** des commandes Tauri fines appellent `kairos-core` (une base ouverte par appel) ; un fil Rust surveille `PRAGMA data_version` et prévient l'interface (`kairos://changed`) ; l'interface Angular lit un `snapshot` unique, fait grandir le chrono localement, et passe par une interface `KairosApi` (réelle, ou fausse avec des scénarios pour la voir dans un navigateur). La logique de l'interface est du TypeScript pur, testé par un script de `scripts/verify` (esbuild, comme l'existant).

**Tech Stack:** Rust (kairos-core, src-tauri : Tauri v2, tray-icon, `tauri-plugin-single-instance`), Angular 22 zoneless (signals, composants autonomes, OnPush), SCSS sur les tokens de `src/styles/_tokens.scss`, esbuild pour les vérifications, Figma (fichier `st5ct8bxzDkzwnY1UEH1YS`) comme cible visuelle.

**Spec:** `docs/superpowers/specs/2026-10-24-etape-2a-chrono-design.md`

## Écart avec le spec

`Snapshot` porte aussi `home` (le dossier personnel, lu côté Rust) : l'interface ne le connaît pas, et `describeLaunch` en a besoin pour écrire `~/…`.

## Global Constraints

- Les règles métier restent dans `kairos-core` : le pont et l'interface n'en dupliquent aucune (ils affichent les refus du cœur tels quels, en français).
- **Aucune action ne ferme le popover** (Lancer, Changer de lot, Reprendre, Arrêter, Pause, Reprise, erreur). Seuls Échap et un clic ailleurs (perte du focus, déjà en place) le ferment.
- Les heures de fin et « Fin dans… », le calendrier, l'engrenage, « Clôturer la journée » et « ⌘K » ne sont **pas affichés** en 2a.
- L'interface est fidèle aux maquettes de la section « Kairos · v2 » ; les valeurs (couleurs, rayons, espacements, polices) viennent des tokens existants, jamais d'un hexadécimal du code généré par Figma ; aucune image de maquette n'est utilisée comme ressource.
- Tous les textes passent par `I18nService` (fr et en). La couleur ne porte jamais seule un état. Contrastes AA.
- Largeur du popover : 340 px. Hauteur : celle du contenu, plafonnée à l'écran moins 80 px, défilement interne.
- Chaque `kairos://changed` ou action se traduit par un nouveau `snapshot` ; l'interface ne garde aucun état métier qui contredise le dernier `snapshot`.
- Source des lancements faits par l'app : `manual`, sans dossier de travail.
- Textes de commit en français, sans ligne d'attribution.

## Review Focus

1. Base verrouillée, ou plus récente que l'app : la dernière vue connue reste affichée, grisée, avec le bandeau et Réessayer ; jamais un écran vide ni une erreur silencieuse (Tasks 3, 4, 6).
2. Double clic sur « Lancer » : un seul lot, et le refus `already_running` n'est pas montré comme une erreur quand l'état affiché est déjà le bon (Task 4).
3. Passage de minuit avec un lot en cours : le chrono affiche la durée de l'entrée ouverte et repart de 00:00:00 sans planter (Tasks 4, 6).
4. Migration 3 sur une base 0.2.0 existante, avec des lots, des entrées ouvertes, des jours clôturés : rien n'est perdu et `status` marche (Task 1).
5. Aucune action ne ferme le popover : un contrôle automatique le garantit (Task 7).

---

### Task 1: Le cœur : origine des entrées, dossier de travail, choix de sous-tâches, `data_version`

**Files:**

- Create: `crates/kairos-core/src/migrations/003_entry_origin.sql`, `crates/kairos-core/tests/launch.rs`
- Modify: `crates/kairos-core/src/db.rs` (liste des migrations, `LATEST_VERSION`, `Db::data_version`), `crates/kairos-core/src/model.rs`, `crates/kairos-core/src/service.rs`

**Interfaces:**

- Produces:
  - `Source::parse(text: &str) -> Option<Source>` (l'inverse de `as_str`).
  - `StartArgs.workdir: Option<String>` ; `Entry.source: Option<Source>` et `Entry.workdir: Option<String>` (sérialisés) ; `Status::Running` et `Status::Paused` gagnent `launched_by: Option<Source>` et `workdir: Option<String>`.
  - `SubtaskChoices { pub main: String, pub base: Vec<String>, pub own: Vec<String> }` (`Serialize`, `Debug`, `Clone`, `PartialEq`, `Eq`) et `Kairos::subtask_choices(&self, parent_monday_id: Option<&str>) -> Result<SubtaskChoices>`.
  - `Db::data_version(&self) -> Result<i64>`.

- [ ] **Step 1: Tests (RED)** dans `tests/launch.rs` (montage de `tests/recent.rs`, horloge figée) :
  - `a_started_entry_remembers_who_and_where` : `start` avec `source: ClaudeCode` et `workdir: Some("/Users/lucas/Beeside/main")` → `status()` rend `launched_by == Some(ClaudeCode)` et ce `workdir` ; `switch_to` sans dossier rend `launched_by` de la nouvelle source et `workdir == None`.
  - `a_resumed_entry_and_a_midnight_split_inherit_the_origin` : `start` (ClaudeCode + dossier), `pause`, `resume` → la nouvelle entrée a la même source et le même dossier ; un lot ouvert à 23 h 50 puis `reconcile` (avancer l'horloge de 20 min) → l'entrée du lendemain hérite.
  - `a_reused_lot_reports_the_launcher_of_the_current_entry` : lot lancé par `Manual`, arrêté, relancé par `ClaudeCode` → `launched_by == Some(ClaudeCode)`.
  - `a_workdir_is_cleaned_never_an_error` : un dossier vide, avec un retour à la ligne, ou de plus de 500 caractères ne fait jamais échouer `start` : il vaut `None` (vide ou caractère de contrôle) ou est tronqué à 500 caractères.
  - `migration_3_keeps_a_0_2_0_database` : construire une base au `user_version` 2 avec un lot, une entrée ouverte, une entrée fermée et un jour clôturé (SQL direct sur la base de la migration 2), l'ouvrir avec `Db::open` → `user_version` 3, `status()` rend le lot en cours avec `launched_by == None`, rien de perdu.
  - `subtask_choices_lists_main_base_and_own_subtasks` : parent inconnu → `own` vide ; parent avec la sous-tâche propre « Retour icône » → `own == ["Retour icône"]` ; `main` vaut le réglage `main_subtask` ; `base` ne contient aucune sous-tâche propre.
  - `data_version_changes_after_another_connection_writes` : deux `Db::open` sur le même fichier ; `data_version` de A est stable après une lecture, et change après une écriture de B.
  - Dans `db.rs` : le test qui fixe `LATEST_VERSION` passe à 3.
- [ ] **Step 2: Les voir échouer** — Run: `cargo test -p kairos-core --test launch` ; Expected: erreurs de compilation.
- [ ] **Step 3: Implémenter** : la migration 3 (`ALTER TABLE entries ADD COLUMN source TEXT; ALTER TABLE entries ADD COLUMN workdir TEXT;`) ajoutée à la liste ; `insert_open_entry(ctx, lot_id, at, origin)` avec une petite structure `Origin { source, workdir }` ; `start_in` écrit l'origine de `args` ; `resume` et `reconcile` copient celle de l'entrée qu'ils continuent ; `add_entry` laisse les deux colonnes vides ; la lecture des entrées (`row_to_entry`) remplit les nouveaux champs ; `status` lit l'origine de l'entrée ouverte (ou de la dernière du lot en pause) ; nettoyage du dossier dans une fonction dédiée (jamais d'erreur).
- [ ] **Step 4: Les voir passer** — Run: `cargo test --workspace` ; Expected: tout vert (les tests JSON existants du CLI qui comparent des entrées se mettent à jour pour les deux champs, sans autre changement) ; `cargo clippy --workspace --all-targets -- -D warnings` propre.
- [ ] **Step 5: Commit** — `feat(core): origine et dossier de travail des entrées, choix de sous-tâches, data_version`

---

### Task 2: La ligne de commande et le hook écrivent le dossier de travail

**Files:**

- Modify: `crates/kairos-cli/src/main.rs`, `crates/kairos-cli/src/hook/mod.rs`
- Test: `crates/kairos-cli/tests/cli.rs`, `crates/kairos-cli/tests/hook.rs`, `crates/kairos-cli/tests/mcp_stdio.rs`

**Interfaces:**

- Consumes: `StartArgs.workdir`, `Status::Running.workdir` (Task 1).

- [ ] **Step 1: Tests (RED)** :
  - CLI : `kairos start --parent 111` lancé avec `current_dir` fixé à un dossier connu → `kairos --json status` rend `workdir` égal à ce dossier ; idem pour `switch` et `back`.
  - Hook (`auto`) : le message avec un ID sûr et `cwd: <dépôt>` → `status` rend `workdir == <dépôt>` et `launched_by == "claude_code"`.
  - MCP stdio : `start` rend un `status` avec `workdir == null`.
- [ ] **Step 2: Les voir échouer** — Run: `cargo test -p kairos-cli` ; Expected: échecs sur les trois nouveaux tests.
- [ ] **Step 3: Implémenter** : `start_args` de `main.rs` y met `std::env::current_dir()` (texte, `None` en cas d'erreur) ; `act` du hook y met `input.cwd` ; le MCP reste sans dossier.
- [ ] **Step 4: Les voir passer** — Run: `cargo test --workspace` ; Expected: tout vert.
- [ ] **Step 5: Commit** — `feat(cli): le dossier de travail est enregistré au lancement (ligne de commande et hook)`

---

### Task 3: Le pont Rust : commandes, surveillance, titre de l'icône, quitter, fenêtre

**Files:**

- Create: `src-tauri/src/bridge.rs`, `src-tauri/src/watcher.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/tray.rs`, `src-tauri/Cargo.toml` (`tauri-plugin-single-instance`, `serde`, `tempfile` en dev), `src-tauri/capabilities/*.json` (écoute d'événements), `src-tauri/tauri.conf.json` si la hauteur de départ change

**Interfaces:**

- Consumes: Task 1 (`subtask_choices`, `data_version`, `Status`, `recent_lots`, `today`).
- Produces (bridge.rs, fonctions pures sur `&Kairos<C: Clock>`, testables sans fenêtre) :
  - `pub struct BridgeError { pub code: String, pub message: String }` (`Serialize`), `From<kairos_core::error::Error>`.
  - `pub struct Snapshot { pub status: Status, pub today: DayView, pub recent: Vec<RecentLot>, pub choices: SubtaskChoices, pub server_time: i64, pub home: String }` (`Serialize`).
  - `pub struct StartRequest { pub parent_monday_id: Option<String>, pub parent_label: Option<String>, pub name: Option<String>, pub subtask: Option<String>, pub subtask_monday_id: Option<String>, pub at: Option<i64> }` (`Deserialize`, noms en snake_case comme le JSON du cœur).
  - `pub fn snapshot<C: Clock>(k: &Kairos<C>, home: &str) -> Result<Snapshot, BridgeError>` ; `pub fn start/switch/stop/pause/resume(k, …)` ; `pub fn subtask_choices(k, parent: Option<String>)`.
  - `pub fn menu_title(status: &Status, now: i64) -> Option<String>` : `Some("0:42")` en cours (heures sans zéro initial, minutes à deux chiffres, sans secondes), `Some("Pause · 0:42")` en pause (durée de la dernière entrée), `None` au repos.
  - Commandes Tauri : `snapshot`, `start`, `switch`, `stop`, `pause`, `resume`, `subtask_choices`, `set_popover_height(pixels: f64)`, `quit(stop_lot: bool)`.
  - Événements : `kairos://changed` (watcher) et `kairos://quit-requested` (le popover doit afficher le dialogue).

- [ ] **Step 1: Tests (RED)** (tests unitaires dans `bridge.rs`, base temporaire, horloge figée) :
  - `snapshot_of_an_empty_database_is_idle_with_main_subtask_choices` ; `snapshot_with_a_running_lot_has_status_today_and_recent`.
  - `start_then_stop_through_the_bridge_match_the_core` ; `a_refusal_keeps_its_code_and_french_message` (deuxième `start` → `BridgeError { code: "already_running", message }` avec le texte du cœur) ; `start_with_a_future_time_is_refused_as_in_future`.
  - `menu_title_formats` : 42 min → `0:42`, 1 h 05 → `1:05`, 12 h 00 → `12:00`, jamais de secondes ; en pause `Pause · 0:42` ; au repos `None`.
  - `quit_with_stop_lot_stops_the_running_lot` (la fonction pure que la commande appelle).
  - Surveillance (`watcher.rs`) : la fonction de comparaison `changed(previous: Option<i64>, now: i64) -> bool` (premier relevé : faux ; valeur différente : vrai) ; un test d'intégration qui écrit depuis une autre connexion et constate que `data_version` a changé.
- [ ] **Step 2: Les voir échouer** — Run: `cargo test -p kairos-app` ; Expected: erreurs de compilation.
- [ ] **Step 3: Implémenter** : `bridge.rs` et ses commandes (`open()` : `Db::open_with_timeout(&Db::default_path(), 2 s)` + `SystemClock`) ; `watcher.rs` : un fil qui, chaque seconde, lit `data_version` d'une connexion ouverte une fois (rouverte à la seconde suivante si elle échoue) et émet `kairos://changed` quand elle change ; `tray.rs` : `tray.set_title` à chaque changement et à chaque minute pile, avec `menu_title`, les glyphes d'état relevés sur la maquette (voir Task 7) ; le menu « Quitter Kairos » et ⌘Q : sans lot en cours `app.exit(0)`, avec un lot en cours on affiche le popover et on émet `kairos://quit-requested` ; `set_popover_height` borne la hauteur à l'écran moins 80 px et conserve la position sous l'icône ; instance unique : une seconde instance se ferme et montre la première ; enregistrer les commandes dans `invoke_handler` et vérifier que les capacités autorisent l'écoute des événements.
- [ ] **Step 4: Les voir passer** — Run: `cargo test --workspace` ; Expected: tout vert, clippy et fmt propres ; `cargo build -p kairos-app` compile.
- [ ] **Step 5: Commit** — `feat(app): le pont Tauri vers le cœur, la surveillance de la base et le titre de l'icône`

---

### Task 4: La couche Angular : interface du pont, scénarios, état, logique pure

**Files:**

- Create: `src/app/services/kairos/kairos-api.ts`, `tauri-kairos-api.ts`, `mock-kairos-api.ts`, `scenarios.ts`, `kairos.store.ts` ; `src/app/features/popover/logic/{clock.ts,form.ts,view.ts,launch.ts,errors.ts}` ; `scripts/verify/popover.cjs`
- Modify: `src/app/app.config.ts` (le fournisseur de `KairosApi`), `package.json` (`verify` étendu)

**Interfaces:**

- Produces (types TypeScript fidèles au JSON du cœur : à relever dans `crates/kairos-core/src/model.rs`, `Status` étiqueté par `state`) :
  - `interface KairosApi { snapshot(): Promise<Snapshot>; start(r: StartRequest): Promise<void>; switch(r: StartRequest): Promise<void>; stop(): Promise<void>; pause(): Promise<void>; resume(): Promise<void>; subtaskChoices(parent: string | null): Promise<SubtaskChoices>; onChanged(cb: () => void): () => void; setPopoverHeight(px: number): Promise<void>; quit(stopLot: boolean): Promise<void>; onQuitRequested(cb: () => void): () => void; }` ; les erreurs sont des `BridgeError { code: string; message: string }`.
  - `KairosStore` (injectable, signals) : `snapshot`, `view`, `busy`, `error`, `stale`, `clockSeconds`, `form` (brouillon), et `start()`, `switchLot()`, `stop()`, `pause()`, `resume()`, `reload()`, qui ne ferment jamais la fenêtre.
  - `MockKairosApi` : choisi quand `window.__TAURI_INTERNALS__` est absent ; le scénario vient de `?scenario=` (`idle`, `running`, `paused`, `switching`, `error-locked`, `error-newer`, `no-id`, `long-names`, `no-recent`, `claude-launched`).
  - Logique pure : `formatClock(seconds) -> "HH:MM:SS"`, `formatHm(seconds) -> "0 h 42"`, `formatBar(seconds) -> "0:42"`, `secondsFromHhmm(text, nowMs) -> number | null`, `canSubmit(form) -> { ok: boolean, reason?: 'need-parent-or-name' | 'bad-time' }`, `pickView(status, switching) -> 'start' | 'running' | 'paused' | 'switch'`, `describeLaunch({ source, startedAt, workdir, home, hm }) -> string` (« Lancé par Claude Code à 13:34 · ~/Beeside/main », « Lancé à 13:34 » pour `manual`/absent, dossier réduit à ses deux derniers composants et préfixé `~/` s'il est sous `home`, `…/` sinon), `parentColorIndex(key) -> 0..5` (déterministe), `errorField(code) -> 'parent' | 'subtask' | 'time' | null` (`invalid_monday_id`, `invalid_name`, `in_future`, `too_old`…).

- [ ] **Step 1: Tests (RED)** dans `scripts/verify/popover.cjs` (même montage que `verify/whats-new.cjs` : esbuild `buildSync` en `bundle`, `platform: 'node'`, sur les modules de `logic/`, qui n'importent aucun module Angular) :
  - `formatClock(2533) === "00:42:13"`, `formatClock(0)`, `formatClock(86399)`, jamais négatif (`formatClock(-5) === "00:00:00"`) ; `formatHm(2520) === "0 h 42"`, `formatHm(14040) === "3 h 54"` ; `formatBar(2533) === "0:42"`, `formatBar(3900) === "1:05"`.
  - `secondsFromHhmm("13:34", ms)` pour un `ms` de ce jour-là rend l'instant local de 13 h 34 ; `"9:5"`, `"25:00"`, `"ab"`, `""` rendent `null` ; `"23:59"` rendu à 00 h 05 est dans le futur de ce jour (le refus vient du cœur) ; à un changement d'heure le résultat reste l'instant local demandé.
  - `canSubmit` : ni ID ni nom → `need-parent-or-name` ; un ID seul passe ; un nom seul passe ; un début mal formé → `bad-time`.
  - `pickView` : `idle` → `start` ; `running` → `running` ; `running` avec `switching` → `switch` ; `paused` → `paused`.
  - `describeLaunch` : `claude_code` + dossier `/Users/aegis/Beeside/main` + home `/Users/aegis` → « Lancé par Claude Code à 13:34 · ~/Beeside/main » ; `/tmp/a/b/c` → « … · …/b/c » ; `manual` sans dossier → « Lancé à 13:34 » ; `claude_desktop` → « Lancé par Claude Desktop à 13:34 ».
  - `parentColorIndex` : deterministe et toujours dans 0..5.
  - Le magasin (Angular testé avec la fausse API, sans composant) : `start()` deux fois très vite → un seul appel à l'API, et si l'API répond `already_running` alors que l'état affiché est déjà « en cours », aucune erreur n'est publiée ; une API qui échoue avec `db_unavailable` laisse le dernier `snapshot` affiché et passe `stale` à vrai ; `reload()` réussi remet `stale` à faux ; `clockSeconds` reste correct quand `server_time` diffère de l'horloge locale de 2 s ; `start()` ne déclenche jamais de fermeture de fenêtre.
- [ ] **Step 2: Les voir échouer** — Run: `node scripts/verify/popover.cjs` ; Expected: échec (modules absents).
- [ ] **Step 3: Implémenter** les modules de `logic/` (purs), `KairosApi` et ses deux implémentations (`tauri-kairos-api.ts` utilise `invoke` et `listen` de `@tauri-apps/api` ; le faux répond avec les scénarios, avec un petit délai), puis `KairosStore` ; ajouter `node scripts/verify/popover.cjs` à `npm run verify`.
- [ ] **Step 4: Les voir passer** — Run: `node scripts/verify/popover.cjs && npm run build` ; Expected: sortie 0, build Angular réussi.
- [ ] **Step 5: Commit** — `feat(app): la couche Angular du chronomètre (pont, scénarios, état, logique pure)`

---

### Task 5: Les composants communs

**Files:**

- Create: `src/app/ui/{button,field,chip,recent-pill,quick-resume,section-title,alert,badge,popover-header}.ts` (+ `.scss` et `.html` ou gabarit en ligne) ; `src/app/ui/index.ts`
- Modify: `src/app/lang/fr.ts`, `src/app/lang/en.ts` (libellés communs)

**Interfaces:**

- Produces (composants autonomes, `OnPush`, sélecteurs en `k-…`) :
  - `k-button` : `variant: 'primary' | 'secondary' | 'danger' | 'ghost'`, `disabled`, `kbd` (puce de raccourci affichée à droite), `icon` ; sortie `pressed`.
  - `k-field` : `label`, `placeholder`, `mono`, `error`, `value` (modèle bidirectionnel), `hint` ; accessible (`label` lié).
  - `k-chip` : `selected`, `dashed` (« + Autre »), `value` ; `k-recent-pill` : `id`, `name` ; `k-quick-resume` : `name`, `id`, `subtask` ; `k-section-title` : `step: number`, `title`, `hint` ; `k-alert` : `kind: 'error' | 'info'`, `actionLabel`, sortie `action` ; `k-badge` : `text` ; `k-popover-header` : `title`, `subtitle`.

- [ ] **Step 1: Relever les maquettes** : pour chaque composant, `get_design_context` (fichier `st5ct8bxzDkzwnY1UEH1YS`, avec `skillNames: "figma-design-to-code"`) sur Bouton `245:348`, Champ de saisie `245:750`, Puce `243:108`, Puce de choix `280:1557`, Pastille récente `243:211`, Reprise rapide `245:1067`, Titre de section `243:299`, Alerte `245:1173`, Badge `243:89`, En-tête de popover `245:1471`. Chercher d'abord les tokens existants dans `src/styles/_tokens.scss` ; créer un composant par maquette, sans copier le code généré (positions absolues, hexadécimaux).
- [ ] **Step 2: Test (RED)** : `scripts/verify/ui.cjs` vérifie par lecture du source que chaque composant déclare ses entrées (`input(`) et qu'aucun `.scss` de `src/app/ui` ne contient de couleur littérale (`#xxxxxx`, `rgb(`) ni de `px` hors `1px`, `border-radius` et tailles de police issus de tokens ; ajouter à `npm run verify`. Run: `node scripts/verify/ui.cjs` ; Expected: échec (fichiers absents).
- [ ] **Step 3: Implémenter** les composants et leurs styles sur les tokens ; les états `:hover`, `:focus-visible`, `:disabled` et l'accessibilité clavier (flèches ou Tab pour les puces, `aria-pressed`, `aria-invalid`, `role="alert"` pour l'alerte).
- [ ] **Step 4: Vérifier** — Run: `node scripts/verify/ui.cjs && npm run build` ; Expected: sortie 0.
- [ ] **Step 5: Commit** — `feat(app): les composants communs du popover`

---

### Task 6: Les vues et le câblage

**Files:**

- Create: `src/app/features/popover/{start-view,running-view,error-banner,quit-dialog,today-list}.ts` (+ gabarits et styles)
- Modify: `src/app/features/popover/popover.ts`, `popover.html`, `popover.scss` (la coquille), `src/app/lang/fr.ts`, `src/app/lang/en.ts`

**Interfaces:**

- Consumes: `KairosStore` (Task 4), composants (Task 5), `KairosApi.setPopoverHeight` et `onQuitRequested`.
- Produces: la coquille `Popover` choisit la vue avec `pickView`, affiche le bandeau d'erreur et, au-dessus, rien de plus ; `StartView` (mode `start` ou `switch`), `RunningView` (en cours et en pause), `TodayList` (entrées du jour en lecture seule), `QuitDialog`.

- [ ] **Step 1: Relever les maquettes** : `get_design_context` (avec `skillNames: "figma-design-to-code"`) sur Lancer `220:81`, Minuteur lancé `220:179`, Changer de lot `223:304`, Lancer sans ID ni nom `223:399`, Dialogue quitter `230:456`, Erreur `230:506` ; relever aussi la maquette de l'état En pause (chercher dans la page « MaquettesV1 » une variante pause ; à défaut, la spécifier à partir du minuteur lancé comme dans le spec).
- [ ] **Step 2: Test (RED)** : étendre `scripts/verify/popover.cjs` avec la lecture du source :
  - aucun fichier de `src/app` autre que la coquille ne contient `.hide(` ni `close(` (sauf `Escape` dans la coquille) ; la coquille ne ferme que sur `Escape` ;
  - chaque clé i18n utilisée par les vues existe en fr **et** en en ;
  - les vues ne contiennent ni le bloc « Fin » ni « Fin dans », ni le calendrier, l'engrenage, « Clôturer la journée », « ⌘K ».
    Run: `node scripts/verify/popover.cjs` ; Expected: échec.
- [ ] **Step 3: Implémenter** :
  - `StartView` : les trois blocs de la maquette (récents → remplissent « Nom » et « ID Monday » ; puces de sous-tâche = `choices.base` puis `choices.own`, la première sélectionnée = `choices.main` ; « + Autre » met le focus sur « Nom » de la sous-tâche ; changer d'ID de parent redemande `subtaskChoices`), « Début » (`HH:MM`), le récapitulatif « Les heures iront sur … › … · Chrono libre : jusqu'à ce que tu l'arrêtes », le bouton **Lancer** ou **Changer de lot** (inactif tant que `canSubmit` n'est pas ok, avec la raison), la ligne « Ou depuis Claude : kairos start » (ou « Le nouveau lot démarre à la même seconde »), la liste **Reprendre** (3 premiers de `recent` ; un clic lance ce lot, ou y bascule) ; les erreurs du cœur s'affichent sous le champ désigné par `errorField`, sinon dans le bandeau.
  - `RunningView` : chrono `HH:MM:SS` (mono), nom du lot, badge de sous-tâche, « parent … · sous-tâche … », `describeLaunch`, **Arrêter** (⌘.), **Pause** ou **Reprendre**, ⇄ ; `TodayList` (« AUJOURD'HUI · 3 h 54 », carré de couleur par `parentColorIndex`, nom, « ID · sous-tâche · début → fin », durée, l'entrée en cours en vert) en lecture seule.
  - `QuitDialog` : trois boutons, **Arrêter le lot et quitter** (danger), **Quitter, garder le chrono**, **Annuler** (Échap) ; affiché sur `onQuitRequested`.
  - Coquille : raccourcis ⌘↵ (lance), ⌘. (arrête), Échap (ferme) ; après chaque changement de contenu, `setPopoverHeight(hauteur du contenu)` ; brouillon du formulaire conservé par le magasin ; vue grisée quand `stale` ; `aria-live="polite"` pour le changement d'état, le chrono n'est lu qu'à la minute.
- [ ] **Step 4: Les voir passer** — Run: `node scripts/verify/popover.cjs && npm run build` ; Expected: sortie 0.
- [ ] **Step 5: Commit** — `feat(app): les vues du popover (lancer, minuteur, changer de lot, erreur, quitter)`

---

### Task 7: Vérification contre Figma, essai réel, documentation

**Files:**

- Modify: `README.md`, page Notion « Kairos » / « Utiliser Kairos » (via le connecteur Notion), `src-tauri/src/tray.rs` (glyphes d'état, si l'essai en révèle l'écart)

- [ ] **Step 1: Comparaison visuelle** : lancer `npm run start` ; dans le navigateur intégré, ouvrir `http://localhost:1420/?scenario=<…>` à 340 px de large pour `idle`, `running`, `paused`, `switching`, `error-locked`, `no-id`, `long-names`, `no-recent`, `claude-launched`, dans les thèmes Sombre et Clair ; comparer avec les captures Figma `220:81`, `220:179`, `223:304`, `223:399`, `230:506`, `226:654` (clair) ; corriger chaque écart de structure, d'espacement, de couleur, de typographie ou de texte ; relever les glyphes de la barre de menus sur les maquettes `231:115` et `231:140` et en déduire ceux des états repos, en cours et pause.
- [ ] **Step 2: Essai réel dans l'app** : `npm run dev`. Vérifier : `kairos start --parent 111` dans un terminal apparaît dans le popover en moins de 2 s avec « Lancé à … · <dossier> » ; lancer depuis le popover puis `kairos status` ; Pause, Reprendre, Arrêter, Changer de lot, Reprendre une pastille ; le popover **ne se ferme sur aucune action** ; l'icône affiche la durée sans secondes ; ⌘Q avec un lot en cours ouvre le dialogue, et « garder le chrono » laisse le lot ouvert ; verrouiller la base (`sqlite3 … 'BEGIN IMMEDIATE'`) → bandeau et Réessayer ; relancer l'app : une seule instance.
- [ ] **Step 3: Contrôle automatique** — Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && npm run verify && npx prettier --check .` ; Expected: tout vert (le contrôle « aucune action ne ferme le popover » de `popover.cjs` en fait partie).
- [ ] **Step 4: Documentation** : README (section sur l'app : ce que montre le popover, raccourcis, « aucune action ne ferme le popover », ce qui arrive en 2b à 2d) ; page Notion « Utiliser Kairos » mise à jour avec ce qui est livré.
- [ ] **Step 5: Commit** — `docs: l'app de barre de menus (2a)`
