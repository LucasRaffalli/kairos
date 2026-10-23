# Kairos, étape 1a : le cœur et la ligne de commande, plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Chronométrer une journée entière depuis le terminal (`kairos start`, `pause`, `switch`, `stop`), la corriger, la clôturer et obtenir la ligne à saisir dans Monday, avec toutes les règles dans `kairos-core`.

**Architecture:** `kairos-core` expose un service `Kairos<C: Clock>` sur SQLite (WAL, une transaction `BEGIN IMMEDIATE` par opération), des fonctions pures de calendrier (`timeline`) et un calcul de ventilation (`report`). `kairos-cli` n'ajoute aucune règle : il traduit des arguments en appels du service et rend le résultat en texte ou en JSON.

**Tech Stack:** Rust 1.99, `rusqlite` (bundled), `chrono`, `thiserror`, `serde`, `serde_json`, `clap`.

**Spec:** `docs/superpowers/specs/2026-10-23-etape-1a-coeur-design.md`

## Global Constraints

- Toutes les règles métier vivent dans `crates/kairos-core` ; `crates/kairos-cli` n'en contient aucune.
- Instants : entiers en secondes UTC. Dates de journée : `AAAA-MM-JJ` en heure locale. Limites de jour calculées avec le décalage local **à cet instant** (jours de 23 h ou 25 h).
- Une entrée ne traverse jamais minuit ; au plus une entrée ouverte (index unique partiel) ; bornes qui se touchent permises ; jamais dans le futur ; `end > start`.
- Références d'un lot : ID Monday (chiffres, 1 à 20) **ou** nom de tâche (1 à 120 caractères après rognage, une seule ligne), jamais aucun. Avec un ID, la sous-tâche est obligatoire (défaut : réglage `main_subtask`, `Front`).
- SQLite : WAL, `foreign_keys=ON`, `busy_timeout` 5 000 ms, `PRAGMA user_version` pour les migrations ; une base de `user_version` plus récent est refusée sans écriture.
- Base : `~/Library/Application Support/com.aegis.kairos/kairos.db`, remplaçable par `KAIROS_DB`. Crochets de test de la CLI : `KAIROS_NOW` (secondes) et `KAIROS_TZ_OFFSET` (secondes), lus par la CLI seulement.
- Erreurs : un variant par cas, `code()` stable, message français qui dit comment corriger. CLI : code de sortie 2, `--json` rend `{"error":{"code","message"}}`.
- Qualité : `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo test --workspace`, `npm run verify`, `npm run format:check`. Un commit par tâche, aucun push sans demande.

## Review Focus

- L'horloge recule (réglage manuel, NTP) alors qu'un lot tourne : `stop` ne crée jamais une entrée de durée négative ou nulle ; erreur `clock_behind` (« L'horloge est en retard sur le début du lot (13 h 34) : corrige l'heure ou utilise --at »). Test : Task 3.
- Une entrée de durée zéro (`start == end`) ajoutée ou modifiée à la main : refusée (`empty_entry`). Test : Task 4.
- Un `switch` vers le lot qui tourne déjà : refusé (`same_lot`), il ne crée pas une coupure artificielle. Test : Task 3.
- Une date invalide (`2026-13-45`, `hier`) en argument : `invalid_date`, jamais une panique. Test : Task 4 et Task 6.
- Un nom de tâche avec retour à la ligne, des espaces autour, 500 caractères ou un emoji : rogné, une seule ligne, refusé au-delà de 120 ; l'emoji est accepté. Test : Task 3.

---

### Task 1: Horloge et calendrier (fonctions pures)

**Files:**

- Modify: `crates/kairos-core/Cargo.toml` (ajouter `chrono = { version = "0.4", default-features = false, features = ["std", "clock"] }`)
- Create: `crates/kairos-core/src/clock.rs`, `crates/kairos-core/src/timeline.rs`
- Modify: `crates/kairos-core/src/lib.rs` (`pub mod clock; pub mod timeline;`)

**Interfaces:**

- Produces:
  - `pub trait Clock { fn now(&self) -> i64; fn utc_offset(&self, at: i64) -> i32; }` ; `pub struct SystemClock;` ; `pub struct FakeClock` avec `FakeClock::new(now: i64, offset: i32) -> Self`, `set_now(&self, i64)`, `advance(&self, secs: i64)`, `with_offset_fn(self, f: Box<dyn Fn(i64) -> i32 + Send + Sync>) -> Self`.
  - `pub struct Interval { pub start: i64, pub end: i64 }` (semi-ouvert `[start, end)`).
  - `pub fn overlaps(a: Interval, b: Interval) -> bool`
  - `pub fn local_date(at: i64, offset: &dyn Fn(i64) -> i32) -> String` (`AAAA-MM-JJ`)
  - `pub fn day_bounds(date: &str, offset: &dyn Fn(i64) -> i32) -> Result<Interval, Error>` (début inclus, minuit suivant exclu) ; date invalide → `Error::InvalidDate`
  - `pub fn split_at_midnights(iv: Interval, offset: &dyn Fn(i64) -> i32) -> Vec<(String, Interval)>`
  - `pub fn format_duration(secs: i64) -> String` (`"2 h 10"`, `"0 h 43"`, `"12 h 05"` : minutes sur deux chiffres, arrondi vers le bas).

- [ ] **Step 1:** Écrire les tests dans `timeline.rs` : `touching_intervals_do_not_overlap` (`[0,10)` et `[10,20)` → false), `contained_interval_overlaps`, `identical_intervals_overlap`, `split_over_one_midnight` (entrée de 23:00 à 01:00 à décalage 0 → deux morceaux, `[23:00, 24:00)` du jour J et `[00:00, 01:00)` du jour J+1), `split_over_three_midnights`, `split_exactly_at_midnight` (fin pile à minuit → un seul morceau), `dst_spring_day_is_23_hours` et `dst_autumn_day_is_25_hours` (décalage passant de 3600 à 7200 puis inverse, via `with_offset_fn`), `invalid_date_is_refused` (`"2026-13-45"` et `"hier"`), `format_duration_examples` (`7800 → "2 h 10"`, `2580 → "0 h 43"`, `0 → "0 h 00"`).
- [ ] **Step 2:** `cargo test -p kairos-core` : FAIL (modules absents).
- [ ] **Step 3:** Implémenter `clock.rs` (`SystemClock` : `chrono::Utc::now().timestamp()` et décalage `chrono::Local` à l'instant donné) et `timeline.rs` avec `chrono::NaiveDate` pour la calendrier ; la limite de jour est trouvée en cherchant l'instant dont `local_date` change (le décalage est lu à cet instant).
- [ ] **Step 4:** `cargo test -p kairos-core` PASS ; clippy et fmt sans avertissement.
- [ ] **Step 5:** Commit `feat(core): horloge injectable et calendrier pur`.

### Task 2: Erreurs, modèle et base SQLite

**Files:**

- Modify: `crates/kairos-core/Cargo.toml` (`rusqlite = { version = "0.40", features = ["bundled"] }`, `thiserror = "2"`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`)
- Create: `crates/kairos-core/src/error.rs`, `model.rs`, `db.rs`, `migrations/001_init.sql`
- Modify: `crates/kairos-core/src/lib.rs`

**Interfaces:**

- Consumes: `Interval` (Task 1).
- Produces:
  - `pub enum Error` avec les variants du spec : `AlreadyRunning`, `NothingRunning`, `Overlap`, `InFuture`, `CrossesMidnight`, `EmptyEntry`, `ReferenceMissing`, `InvalidMondayId`, `InvalidName`, `InvalidDate`, `ClockBehind`, `SameLot`, `DayNotOpen`, `DayNotClosed`, `DayEmpty`, `RunningLotOnClose`, `EntryNotFound`, `NothingToResume`, `InvalidSetting`, `DbNewer`, `DbUnavailable`, `Internal(String)` ; `Error::code(&self) -> &'static str` (le nom en `snake_case`) ; `Display` = message français ; `pub type Result<T> = std::result::Result<T, Error>`.
  - `pub enum Source { Manual, ClaudeCode, ClaudeDesktop }` ; `pub struct Entry { id: i64, lot_id: i64, start: i64, end: Option<i64>, removed: bool, note: Option<String> }` ; `pub struct LotView { id: i64, parent_monday_id: Option<String>, parent_label: Option<String>, subtask: Option<String>, name: Option<String> }` ; `pub enum DayStatus { Open, Closed, Declared, ToRedeclare }` ; tous `serde::Serialize`.
  - `pub struct Db` ; `Db::open(path: &Path) -> Result<Db>` ; `Db::default_path() -> PathBuf` (lit `KAIROS_DB`) ; `Db::conn(&self) -> &rusqlite::Connection`. `Db::open` crée le dossier, active WAL, `foreign_keys`, `busy_timeout(5000)`, applique les migrations.

- [ ] **Step 1:** Tests dans `db.rs` : `open_creates_missing_directory`, `fresh_db_has_default_project_and_base_subtasks` (projet « Mon projet » par défaut ; sous-tâches de base `Back`, `Front`, `Recettage`, `Maquette` avec `parent_id` nul), `two_open_entries_are_impossible` (insérer deux entrées avec `end` nul → violation de l'index unique), `entry_with_end_before_start_is_rejected` (contrainte `CHECK`), `newer_user_version_is_refused_without_writing` (poser `PRAGMA user_version = 99`, `Db::open` → `Error::DbNewer`, et la base n'a pas été modifiée), `reopening_does_not_redo_migrations`. Dans `error.rs` : `codes_are_stable` (une table `variant → code` vérifiée en entier) et `every_message_is_french_and_actionable` (chaque message non vide et ne contient pas de point d'interrogation seul).
- [ ] **Step 2:** FAIL.
- [ ] **Step 3:** Écrire la migration 1 d'après la section Données du spec (tables `projects`, `parents`, `subtasks`, `lots`, `entries`, `days`, `state`, `settings` ; index unique partiel `entries(end) WHERE end IS NULL` ; `CHECK (end IS NULL OR end > start)` ; réglages par défaut) et `db.rs` (migrations par `user_version`, chaque migration dans une transaction).
- [ ] **Step 4:** PASS, clippy, fmt.
- [ ] **Step 5:** Commit `feat(core): erreurs, modèle et base SQLite avec migrations`.

### Task 3: Service, lots et chronomètre

**Files:**

- Create: `crates/kairos-core/src/service.rs`, `crates/kairos-core/tests/chrono.rs`
- Modify: `crates/kairos-core/src/model.rs`, `lib.rs`

**Interfaces:**

- Consumes: `Clock`, `Interval`, `split_at_midnights`, `Db`, `Error`.
- Produces:
  - `pub struct StartArgs { pub parent_monday_id: Option<String>, pub parent_label: Option<String>, pub name: Option<String>, pub subtask: Option<String>, pub subtask_monday_id: Option<String>, pub at: Option<i64>, pub source: Source }` (`Default` : tout vide, `Source::Manual`).
  - `pub struct Kairos<C: Clock>` ; `Kairos::new(db: Db, clock: C) -> Self`.
  - `start(&self, StartArgs) -> Result<StartedLot>` ; `stop(&self, at: Option<i64>) -> Result<StoppedEntry>` ; `pause(&self) -> Result<StoppedEntry>` ; `resume(&self) -> Result<StartedLot>` ; `switch_to(&self, StartArgs) -> Result<SwitchResult>` ; `status(&self) -> Result<Status>` avec `enum Status { Idle, Running { lot: LotView, since: i64, duration: i64, today_total: i64 }, Paused { lot: LotView, since: i64 } }`.
  - Chaque opération commence par `reconcile(now)` (règle 5 du spec) dans la même transaction.

- [ ] **Step 1:** Tests d'intégration (`FakeClock`, base en mémoire ou temporaire), un par règle : `start_creates_lot_and_open_entry`, `second_start_is_refused_already_running`, `start_with_id_defaults_to_main_subtask` (sous-tâche `Front`), `unknown_subtask_is_created_for_that_parent_only` (créée pour le parent A, absente du parent B), `same_lot_is_reused` (deux `start` / `stop` identiques → un seul lot), `reference_is_required_and_exclusive` (ni ID ni nom → `ReferenceMissing`), `monday_id_must_be_digits_up_to_20` (`"12a"` et 21 chiffres → `InvalidMondayId`), `name_is_trimmed_single_line_and_bounded` (`"  Refonte \n"` → refusé si multi-ligne, `"  Refonte  "` → `"Refonte"`, 121 caractères → `InvalidName`, un emoji accepté), `start_at_past_time_ok_if_free_and_refused_if_overlapping` (dont fin pile sur début d'une autre : accepté), `start_in_future_is_refused`, `pause_closes_entry_and_resume_opens_a_new_one`, `pause_without_lot_and_resume_without_pause_fail`, `switch_is_atomic` (même seconde de fermeture et d'ouverture), `switch_when_idle_behaves_like_start`, `switch_to_same_lot_is_refused_same_lot`, `stop_clears_pause`, `lot_open_at_midnight_is_split` (démarrer à 23:30, avancer l'horloge de 2 h, `status` → deux entrées coupées à 00:00), `lot_left_open_three_days_gives_one_entry_per_day`, `clock_going_backwards_makes_stop_fail_clock_behind` (horloge reculée sous le début de l'entrée ouverte → `ClockBehind`, l'entrée reste ouverte intacte), `status_reports_running_duration_and_today_total`.
- [ ] **Step 2:** FAIL.
- [ ] **Step 3:** Implémenter `service.rs` : une fonction interne `with_tx` (`BEGIN IMMEDIATE`, `reconcile`, la closure, `COMMIT` ou `ROLLBACK`) ; recherche ou création du parent, de la sous-tâche et du lot ; validations de référence ; contrôle de chevauchement via `overlaps` contre les entrées non retirées et l'entrée ouverte comptée comme `[début, maintenant)`. `reconcile` s'appuie sur `split_at_midnights`.
- [ ] **Step 4:** `cargo test -p kairos-core` PASS ; clippy, fmt.
- [ ] **Step 5:** Commit `feat(core): service des lots et chronomètre`.

### Task 4: Entrées manuelles, journées et réglages

**Files:**

- Create: `crates/kairos-core/tests/day.rs`
- Modify: `crates/kairos-core/src/service.rs`, `model.rs`

**Interfaces:**

- Consumes: `Kairos<C>`, `StartArgs`, `Entry`, `DayStatus`.
- Produces:
  - `pub struct AddEntry { pub parent_monday_id: Option<String>, pub parent_label: Option<String>, pub name: Option<String>, pub subtask: Option<String>, pub from: i64, pub to: i64 }` ; `pub struct EditEntry { pub from: Option<i64>, pub to: Option<i64>, pub parent_monday_id: Option<String>, pub name: Option<String>, pub subtask: Option<String> }` (`Default`).
  - `add_entry(&self, AddEntry) -> Result<Entry>` ; `edit_entry(&self, id: i64, EditEntry) -> Result<Entry>` ; `delete_entry(&self, id: i64) -> Result<()>` ; `today(&self, date: Option<&str>) -> Result<DayView>` avec `DayView { date: String, status: DayStatus, entries: Vec<EntryView>, start: Option<i64>, end: Option<i64>, pauses: i64, total: i64 }` ; `close_day(&self, date: Option<&str>, stop_running: bool) -> Result<Day>` ; `mark_declared(&self, date: Option<&str>) -> Result<Day>` ; `set_subtask_id(&self, parent_monday_id: &str, subtask: &str, monday_item_id: &str) -> Result<()>` ; `config_get(&self, key: &str) -> Result<String>` ; `config_set(&self, key: &str, value: &str) -> Result<()>` (clés `main_subtask`, `rounding_minutes` ∈ {0, 5, 10, 15}, sinon `InvalidSetting`).

- [ ] **Step 1:** Tests : `add_entry_refuses_overlap_future_and_midnight_crossing` (trois cas, trois codes), `add_entry_zero_length_is_refused_empty_entry` (`from == to`), `entries_touching_are_accepted`, `edit_entry_applies_the_same_rules`, `delete_entry_removes_it_and_unknown_id_fails`, `invalid_date_argument_is_refused` (`"2026-13-45"`, `"hier"` → `InvalidDate`), `today_gives_bounds_pauses_and_total` (deux entrées séparées de 30 min → `pauses == 1800`), `removed_entries_are_excluded_from_totals` (mise à `removed = 1` directement en base), `close_day_freezes_totals_and_changes_status`, `close_day_refused_while_a_lot_runs_unless_stop` (`RunningLotOnClose`, puis réussite avec `stop_running = true`), `close_day_on_empty_date_is_day_empty_and_creates_no_row`, `close_day_twice_is_refused`, `mark_declared_requires_closed`, `edit_on_declared_day_goes_to_redeclare`, `edit_on_closed_day_stays_closed_and_refreshes_totals`, `edit_on_open_day_changes_nothing`, `to_redeclare_returns_to_declared_after_mark_declared`, `set_subtask_id_validates_digits`, `config_roundtrip_and_validation`, `days_without_row_are_open`.
- [ ] **Step 2:** FAIL.
- [ ] **Step 3:** Implémenter ; la ligne `days` n'est créée qu'à la première clôture ; `closed_totals` et `declared_totals` sont des JSON `{ "<parent>": { "<sous-tâche>": secondes } }` produits par une fonction `totals(date)` du module `report` (stub minimal ici, complété à la Task 5).
- [ ] **Step 4:** PASS, clippy, fmt.
- [ ] **Step 5:** Commit `feat(core): entrées manuelles, journées et réglages`.

### Task 5: Ventilation, recoupement et texte à saisir

**Files:**

- Create: `crates/kairos-core/src/report.rs`, `crates/kairos-core/tests/report.rs`
- Modify: `crates/kairos-core/src/service.rs` (`report`), `lib.rs`

**Interfaces:**

- Consumes: `DayView`, `config_get`.
- Produces: `report(&self, date: Option<&str>) -> Result<Report>` avec `Report { date: String, status: DayStatus, start: Option<i64>, end: Option<i64>, pauses: i64, total: i64, lines: Vec<ParentLine>, no_id: Vec<NoIdLine>, rounding_minutes: i64, lines_total: i64, gap: i64, text: String, changes: Vec<Change> }` ; `ParentLine { monday_id: String, subtasks: Vec<(String, i64)>, total: i64 }` ; `Change { monday_id: String, subtask: String, from: i64, to: i64 }`. Fonction pure `report::round_to(secs: i64, step_minutes: i64) -> i64`.

- [ ] **Step 1:** Tests : `text_matches_the_spec_example` (parent `12406850311` : Front 7800 s, Back 6240 s ; parent `12406856533` : Recettage 3900 s, `Retour icône mal faite` 2580 s ; sans ID 6660 s → exactement `12406850311 : Front 2 h 10 · Back 1 h 44 · 12406856533 : Recettage 1 h 05 · Retour icône mal faite 0 h 43 · sans ID : 1 h 51`), `lines_follow_first_appearance_order`, `no_id_groups_names_in_structured_output`, `gap_is_zero_without_rounding`, `rounding_5_10_15_half_up_and_never_below_step` (`round_to(60, 15) == 900`, `round_to(450, 15) == 900`, `round_to(0, 15) == 0`), `gap_is_reported_with_rounding` (écart non nul visible dans `gap`), `report_on_to_redeclare_lists_changes` (`Front 2 h 10 → 2 h 25` : `from = 7800`, `to = 8700`), `report_excludes_removed_entries`, `running_entry_counts_until_now`.
- [ ] **Step 2:** FAIL.
- [ ] **Step 3:** Implémenter `report.rs` : agrégation par (parent, sous-tâche) en une passe dans l'ordre des entrées ; arrondi par ligne parent × sous-tâche ; `gap = (fin − début − pauses) − lines_total`.
- [ ] **Step 4:** PASS, clippy, fmt.
- [ ] **Step 5:** Commit `feat(core): ventilation, recoupement et texte à saisir`.

### Task 6: La ligne de commande

**Files:**

- Modify: `crates/kairos-cli/Cargo.toml` (`serde_json = "1"`), `crates/kairos-cli/src/main.rs`
- Create: `crates/kairos-cli/src/args.rs`, `time_input.rs`, `render.rs`, `crates/kairos-cli/tests/cli.rs`
- Modify: `crates/kairos-cli/tests/version.rs` (inchangé, doit rester vert)

**Interfaces:**

- Consumes: `Kairos<SystemClock>`, `Db::default_path`, tous les types du service, `Report`, `Status`, `DayView`.
- Produces: `time_input::parse(input: &str, date: &str, offset: &dyn Fn(i64) -> i32) -> Result<i64, Error>` (accepte `13:30`, `AAAA-MM-JJTHH:MM`, RFC 3339 ; sinon `Error::Internal("heure invalide …")` renommé `Error::InvalidTime`, à ajouter au Task 2 si absent) ; `render::text(&Outcome) -> String` et `render::json(&Outcome) -> serde_json::Value` ; les 13 commandes du spec avec leurs options ; `KAIROS_NOW` et `KAIROS_TZ_OFFSET` lus dans `main.rs` seulement, pour construire une horloge figée.

- [ ] **Step 1:** Tests (`time_input`) : `parses_hh_mm_on_the_given_date`, `parses_rfc3339_with_offset`, `rejects_25_61_and_words`. Tests d'intégration du binaire, chacun avec un `KAIROS_DB` temporaire et `KAIROS_NOW` : `start_then_status_shows_running` (texte), `start_json_has_stable_keys` (clés `lot`, `since`), `second_start_exits_2_with_already_running`, `error_json_is_code_and_message` (`{"error":{"code":"already_running", …}}`), `pause_resume_switch_stop_roundtrip`, `switch_when_idle_starts`, `add_edit_delete_entry_roundtrip`, `report_prints_the_monday_line`, `report_json_matches_text`, `close_day_then_mark_declared_then_edit_goes_to_redeclare`, `close_day_with_running_lot_suggests_stop_flag`, `invalid_date_exits_2_invalid_date`, `config_main_subtask_roundtrip`, `help_lists_all_commands`.
- [ ] **Step 2:** FAIL.
- [ ] **Step 3:** Implémenter avec `clap` (dérivé) ; sortie en français, `--json` global ; en cas d'erreur, code de sortie 2 (jamais de panique, y compris `KAIROS_DB` illisible → `db_unavailable`).
- [ ] **Step 4:** `cargo test --workspace` PASS ; clippy, fmt.
- [ ] **Step 5:** Commit `feat(cli): commandes kairos, texte et JSON`.

### Task 7: Concurrence, robustesse et documentation

**Files:**

- Create: `crates/kairos-cli/tests/concurrency.rs`
- Modify: `README.md`, `docs/superpowers/specs/2026-10-23-etape-1a-coeur-design.md` (si un détail a changé en route)

- [ ] **Step 1:** Tests : `two_processes_starting_at_once_only_one_wins` (deux `kairos start` lancés en parallèle sur la même base : exactement un succès, l'autre `already_running` ; jamais deux entrées ouvertes, vérifié en base), `locked_database_reports_db_unavailable_after_timeout` (un processus garde une transaction `IMMEDIATE` ouverte 6 s ; le second sort avec `db_unavailable`, sans panique), `newer_database_is_refused_by_the_binary` (code de sortie 2, `db_newer`), `unicode_and_emoji_names_roundtrip_through_report`.
- [ ] **Step 2:** FAIL puis implémenter ce qui manque ; PASS.
- [ ] **Step 3:** README : section « Utiliser en ligne de commande » avec une journée type (`start`, `pause`, `switch`, `stop`, `report`, `close-day`, `mark-declared`). Mettre à jour la page Notion « Utiliser Kairos » si une commande diffère du spec.
- [ ] **Step 4:** Vérification d'ensemble : `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo test --workspace`, `npm run verify`, `npm run format:check`, `DRY_RUN=1 npm run release` ; rapporter chaque sortie.
- [ ] **Step 5:** Commit `docs: étape 1a terminée` ; lister à Lucas ce qui reste (1b MCP, 1c hooks), sans rien pousser.
