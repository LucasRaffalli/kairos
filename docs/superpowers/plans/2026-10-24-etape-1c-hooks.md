# Étape 1c : les hooks de Claude Code et la détection du lot, plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kairos suit le bon lot quand Lucas travaille dans Claude Code : les hooks lancent ou basculent seuls quand l'ID Monday est sûr, et dans le doute Claude demande ; Claude retrouve un lot précédent sans ID grâce à `recent`, `back` et `set_label`.

**Architecture:** les règles de détection et de décision sont des fonctions pures dans `kairos-core` (`detect.rs`), les lectures Git et fichiers vivent dans `kairos-cli/src/hook/`, et `kairos hook <événement>` fait le lien avec Claude Code (JSON sur l'entrée et la sortie standard, code de sortie toujours 0). Trois nouvelles opérations du service (`recent_lots`, `back`, `set_label`) sont exposées par la ligne de commande et par le MCP (17 outils). `kairos hooks install` écrit la configuration de Claude Code.

**Tech Stack:** Rust (kairos-core, kairos-cli, rusqlite, serde_json, clap), rmcp déjà en place, aucun nouveau crate attendu (`std::process` pour Git).

**Spec:** `docs/superpowers/specs/2026-10-24-etape-1c-hooks-design.md`

## Global Constraints

- Les règles métier sont dans `kairos-core`, jamais dans le CLI ni le MCP ; le CLI traduit des arguments en appels et lit l'extérieur (Git, fichiers).
- **Un hook sort toujours avec le code 0** et n'écrit sur la sortie standard que du JSON (ou rien) ; le détail d'une erreur part sur la sortie d'erreur.
- Le texte du message de l'utilisateur n'est jamais écrit (ni base, ni journal, ni `hook-state.json`).
- ID Monday : 9 à 12 chiffres, sans zéro initial, non collé à des lettres ou à d'autres chiffres, ou lien `monday.com/…/pulses/<id>` ou `…/items/<id>`.
- Repères d'un ID sûr : « monday », « ticket », « tâche », « task », « item », « pulse » (casse ignorée) ou `#`, à moins de 12 caractères devant l'ID, sur la même ligne.
- `claude_autonomy` est lu à chaque appel. `start`, `switch`, `back`, `recent`, `set_label` ne sont pas soumis à la confirmation.
- Source des lots lancés par un hook : `claude-code`.
- Délai de chaque lecture Git : 2 secondes. Entrée du hook lue jusqu'à 1 Mio.
- Textes en français. Aucune ligne d'attribution dans les messages de commit.

## Review Focus

1. Entrée de hook invalide, vide, binaire ou énorme (plus de 1 Mio) : code 0, rien d'autre qu'un JSON sur la sortie standard (Task 4).
2. Nombres pièges dans un message ou un commit (horodatage de 10 chiffres, téléphone, date, version, numéro d'issue, nombre collé à des lettres) : jamais un lot lancé, au plus un doute (Task 2).
3. Le message cite l'ID du lot déjà en cours : aucune action ni question (Task 2).
4. `hook-state.json` absent, illisible ou impossible à écrire : le hook fonctionne, au pire il redemande (Task 4).
5. `settings.json` avec les hooks d'autres outils, ou invalide : les premiers sont conservés, le second est refusé sans rien modifier (Task 5).

---

### Task 1: Le cœur : `resolve_monday_id`, `recent_lots`, `back`, `set_label`

**Files:**

- Modify: `crates/kairos-core/src/service.rs`, `crates/kairos-core/src/model.rs`, `crates/kairos-core/src/error.rs`
- Test: `crates/kairos-core/tests/recent.rs` (nouveau)

**Interfaces:**

- Produces (model.rs):
  - `pub enum KnownId { Parent { label: Option<String> }, Subtask { parent_monday_id: String, subtask: String, label: Option<String> }, Unknown }`
  - `pub struct RecentLot { pub lot: LotView, pub last_end: i64, pub today_total: i64, pub current: bool }` (`Serialize`, `Debug`, `Clone`, `PartialEq`, `Eq`)
- Produces (service.rs, sur `Kairos<C>`):
  - `pub fn resolve_monday_id(&self, id: &str) -> Result<KnownId>` : lecture seule ; `Error::InvalidMondayId` si la forme est fausse.
  - `pub fn recent_lots(&self, limit: usize) -> Result<Vec<RecentLot>>` : lots des 7 derniers jours (selon l'horloge), un par lot distinct, du plus récent au plus ancien, au plus `limit` ; `last_end` = fin de la dernière entrée (maintenant pour l'entrée ouverte) ; `today_total` = secondes du lot aujourd'hui (date locale) ; `current` = le lot de l'entrée ouverte ou en pause.
  - `pub fn back(&self, source: Source) -> Result<SwitchResult>` : lot de la dernière entrée fermée dont le lot diffère de l'ouvert, lancé comme `switch_to` ; rien d'ouvert : relance le lot de la dernière entrée fermée.
  - `pub fn set_label(&self, parent_monday_id: &str, label: &str) -> Result<()>`.
- Produces (error.rs): `Error::NoPreviousLot` (code `no_previous_lot`, « Aucun lot précédent à reprendre. »), `Error::ParentNotFound(String)` (code `parent_not_found`, « Aucun parent avec l'ID {id}. »).

- [ ] **Step 1: Tests (RED)** dans `tests/recent.rs`, avec l'horloge figée habituelle des tests du cœur (voir `tests/day.rs` pour le montage) :
  - `resolve_distinguishes_parent_subtask_and_unknown` : après `start` du parent `12406850311` avec le libellé `Bundle` et `set_subtask_id("12406850311", "Back", "12406856533")`, `resolve_monday_id("12406850311")` vaut `Parent { label: Some("Bundle") }`, `resolve_monday_id("12406856533")` vaut `Subtask { parent_monday_id: "12406850311", subtask: "Back", .. }`, `resolve_monday_id("12406850412")` vaut `Unknown`, et `resolve_monday_id("abc")` renvoie `InvalidMondayId`.
  - `recent_lots_lists_each_lot_once_most_recent_first` : trois lots A, B, A dans la journée (switch), `recent_lots(8)` rend `[A, B]` (A en premier, `current` vrai si A tourne), le `today_total` de A additionne ses deux entrées.
  - `recent_lots_ignores_lots_older_than_seven_days_and_honors_limit` : une entrée vieille de 8 jours est absente ; `limit = 1` rend un seul lot.
  - `back_toggles_between_two_lots` : A puis `switch` B ; `back` relance A (B arrêté) ; un deuxième `back` relance B.
  - `back_with_nothing_running_resumes_the_last_lot` : A lancé puis arrêté ; `back` relance A.
  - `back_without_previous_lot_fails` : base vide → `NoPreviousLot` ; un seul lot jamais quitté → `NoPreviousLot`.
  - `set_label_sets_and_replaces_and_survives_a_start_without_label` : `set_label` pose `Bundle`, puis `Bundle v2` ; un `start` du même parent sans libellé le conserve ; ID inconnu → `ParentNotFound` ; libellé vide, sur deux lignes ou de plus de 200 caractères → `InvalidName`.
  - Une assertion de table dans `error.rs` (liste des codes) pour `no_previous_lot` et `parent_not_found`.
- [ ] **Step 2: Les voir échouer** — Run: `cargo test -p kairos-core --test recent` ; Expected: erreurs de compilation (méthodes absentes).
- [ ] **Step 3: Implémenter** les types, les erreurs (avec leurs messages et codes) et les quatre méthodes dans le style de `set_subtask_id` / `status` : une transaction par appel via `with_tx`, `clean_text` pour le libellé (mêmes limites que les noms de tâche), `ensure_parent` pour ne pas dupliquer la logique des libellés.
- [ ] **Step 4: Les voir passer** — Run: `cargo test -p kairos-core` ; Expected: tout vert (les tests existants inclus).
- [ ] **Step 5: Commit** — `feat(core): resolve_monday_id, recent_lots, back et set_label`

---

### Task 2: Le cœur : détection et décision (`detect.rs`)

**Files:**

- Create: `crates/kairos-core/src/detect.rs`
- Modify: `crates/kairos-core/src/lib.rs` (`pub mod detect;`)
- Test: tests unitaires en bas de `detect.rs` (tableaux)

**Interfaces:**

- Consumes: aucune dépendance au service ni à l'horloge ; l'appelant résout les IDs (Task 1) et les passe.
- Produces:
  - `pub struct FoundId { pub id: String, pub sure_by_shape: bool }` ; `pub fn extract_ids(text: &str) -> Vec<FoundId>` : dans l'ordre d'apparition, sans doublon ; `sure_by_shape` vrai pour un lien Monday ou un ID précédé d'un repère.
  - `pub enum Autonomy { Ask, Auto }`.
  - `pub enum Origin { Message, BranchOrCommits, Config }`.
  - `pub struct Resolved { pub parent: String, pub subtask: Option<String>, pub label: Option<String>, pub sure: bool, pub new_parent: bool }` : `sure` = sûr par la forme, ou reconnu par Kairos, ou venant de `.kairos.json` ; `new_parent` = inconnu de Kairos.
  - `pub struct Detection { pub origin: Origin, pub ids: Vec<Resolved> }` : un parent au plus une fois par détection.
  - `pub struct DecideInput { pub running_parent: Option<String>, pub autonomy: Autonomy, pub message: Vec<Resolved>, pub branch_or_commits: Vec<Resolved>, pub conflicting_branch_and_commits: bool, pub config: Option<Resolved> }`.
  - `pub struct Plan { pub parent: String, pub subtask: Option<String>, pub label: Option<String>, pub new_parent: bool, pub origin: Origin, pub replaces_running: bool }`.
  - `pub enum Decision { Nothing, Act(Plan), Inform(Plan), Doubt(String) }`.
  - `pub fn decide(input: &DecideInput) -> Decision`.

- [ ] **Step 1: Tests d'extraction (RED)**, en tableau (texte → IDs attendus et `sure_by_shape`) :
  - Gabarits de commit : `[Monday #12406850311]`, `[Monday #12406850311, #12406856533]`, `[Monday #12406850311] [Monday #12406856533]`, `closes #12406850311 #12406856533`, `fix(bundle): 12406850311 12406856533 12406850412` (trois IDs sans repère : `sure_by_shape` faux), `monday: 12406850311` (repère), `https://x.monday.com/boards/123456789/pulses/12406850311` (lien : l'ID du lien, et pas `123456789`).
  - Doublon : `12406850311 ... #12406850311` → un seul ID.
  - Pièges ignorés ou non sûrs : `1792774800` (10 chiffres, valide en forme mais sans repère : non sûr), `0612345678` (zéro initial : ignoré), `2026-10-24` (ignoré), `v1.2.3`, `issue 12345` (5 chiffres, ignoré), `abc1234567890xyz` (collé à des lettres : ignoré), un nombre de 13 chiffres (ignoré), retours à la ligne entre deux IDs.
- [ ] **Step 2: Tests de décision (RED)** : une table complète, `auto` et `ask`, qui pin :
  - message avec un ID sûr, rien en cours → `Act` (`auto`, `replaces_running` faux) / `Inform` (`ask`) ;
  - message avec un ID sûr, autre lot en cours → `Act` avec `replaces_running` vrai / `Inform` ;
  - message avec l'ID du lot en cours → `Nothing` ;
  - message avec deux parents → `Doubt` dont le texte commence par « Doute : » et cite les deux IDs ;
  - message avec un numéro nu inconnu et sans repère → `Doubt` ;
  - rien dans le message, branche/commits avec trois parents dont celui en cours → `Nothing` ;
  - rien en cours, trois parents → `Doubt` listant les candidats ;
  - lot en cours absent des parents de la branche → `Doubt` qui cite le lot en cours et les candidats ;
  - `conflicting_branch_and_commits` vrai → `Doubt` ;
  - rien en cours, un seul parent venant de `.kairos.json` → `Act` (`auto`) / `Inform` (`ask`), avec libellé et sous-tâche du fichier ;
  - aucune source → `Nothing`.
  - Le doute ne propose jamais d'agir : le texte contient « Demande à l'utilisateur » et « Ne devine pas ».
- [ ] **Step 3: Les voir échouer** — Run: `cargo test -p kairos-core detect` ; Expected: erreurs de compilation.
- [ ] **Step 4: Implémenter** `extract_ids` (balayage à la main ou `regex` si déjà dans l'arbre, sinon à la main : pas de nouvelle dépendance) et `decide` : ordre des sources `message` > `branch_or_commits` > `config` ; la première source non vide décide ; règles exactes du spec (« Ce que fait `UserPromptSubmit` » et « Plusieurs tâches sur une même branche »).
- [ ] **Step 5: Les voir passer** — Run: `cargo test -p kairos-core` ; Expected: tout vert, `cargo clippy -p kairos-core --all-targets -- -D warnings` propre.
- [ ] **Step 6: Commit** — `feat(core): détection des IDs Monday et décision des hooks`

---

### Task 3: `recent`, `back`, `set_label` en ligne de commande et en MCP

**Files:**

- Modify: `crates/kairos-cli/src/args.rs`, `crates/kairos-cli/src/main.rs`, `crates/kairos-cli/src/render.rs`, `crates/kairos-cli/src/mcp/tools.rs`
- Test: `crates/kairos-cli/tests/cli.rs` (existant), `crates/kairos-cli/tests/mcp_stdio.rs` (existant), tests unitaires de `tools.rs`

**Interfaces:**

- Consumes: Task 1 (`recent_lots`, `back`, `set_label`).
- Produces: commandes `kairos recent [--limit N]` (défaut 8), `kairos back`, `kairos label <ID> <nom>` ; outils MCP `recent` (`limit` facultatif), `back` (aucun paramètre), `set_label` (`parent`, `label`) → 17 outils ; `Outcome::Recent(Vec<RecentLot>)` et `Outcome::Label { parent: String, label: String }` ; `back` rend `Outcome::Switched`.

- [ ] **Step 1: Tests (RED)** :
  - CLI : `kairos recent --json` après deux lots rend un tableau de deux éléments avec `lot`, `last_end`, `today_total`, `current` ; sortie texte : une ligne par lot, le courant marqué ; `kairos back` aller-retour ; `kairos back` sans lot précédent sort en code 2 avec `no_previous_lot` ; `kairos label 12406850311 "Bundle"` puis `recent` montre « Bundle ».
  - MCP (tools.rs, unitaires) : `recent`, `back`, `set_label` appelables ; paramètre inconnu ou `limit` de mauvais type → erreur de protocole ; `set_label` sur un parent inconnu → `Failed` avec le code `parent_not_found` ; la liste des outils contient 17 noms et ni `week` ni outil de réglage.
  - Stdio : `lists_the_seventeen_tools_with_descriptions_and_schemas` (le test de 14 devient 17, mêmes assertions) ; un `recent` puis un `back` de bout en bout.
- [ ] **Step 2: Les voir échouer** — Run: `cargo test -p kairos-cli` ; Expected: échecs sur les nouveaux tests et sur le nombre d'outils.
- [ ] **Step 3: Implémenter** les trois variantes de `Command`, leur exécution dans `execute`, le rendu texte et JSON (le JSON est la sérialisation des types du cœur, comme le reste), et les trois outils dans `definitions()` / `dispatch` : descriptions courtes qui disent **quand** s'en servir (`recent` : « quand l'utilisateur parle d'un lot sans donner d'ID ou veut retrouver ce qu'il faisait » ; `back` : « pour revenir au lot d'avant » ; `set_label` : « pour enregistrer le nom d'une tâche dont on connaît l'ID »).
- [ ] **Step 4: Les voir passer** — Run: `cargo test --workspace` ; Expected: tout vert ; clippy et `cargo fmt --all --check` propres.
- [ ] **Step 5: Commit** — `feat(cli): recent, back et label (ligne de commande et MCP)`

---

### Task 4: `kairos hook` : entrée, sources, décision, sortie, robustesse

**Files:**

- Create: `crates/kairos-cli/src/hook/mod.rs`, `input.rs`, `sources.rs`, `output.rs`, `state.rs`, `rules.rs`
- Modify: `crates/kairos-cli/src/args.rs` (`Command::Hook { event: HookEvent }`, `HookEvent` = `SessionStart` / `UserPrompt`, noms clap `session-start` / `user-prompt`), `crates/kairos-cli/src/main.rs`
- Test: `crates/kairos-cli/tests/hook.rs` (nouveau, sur le vrai binaire), tests unitaires de `sources.rs`, `output.rs`, `state.rs`

**Interfaces:**

- Consumes: Task 1 (`resolve_monday_id`, `status`, `start`, `switch_to`), Task 2 (`detect`), `claude_autonomy` via `config_get`.
- Produces:
  - `hook::run(event: HookEvent) -> i32` : toujours 0.
  - `input::read(reader) -> HookInput { event: String, session_id: String, cwd: Option<PathBuf>, prompt: String }` : lit au plus 1 Mio, ignore les champs inconnus, rend un `HookInput` vide si le JSON est invalide.
  - `sources::{git_branch(cwd) -> Option<String>, git_commits(cwd) -> Option<String>, kairos_json(cwd) -> Option<ConfigFile>}` ; `ConfigFile { parent: String, label: Option<String>, subtask: Option<String> }` ; `.kairos.json` cherché en remontant jusqu'à la racine du dépôt.
  - `output::render(event, additional_context: Option<String>, system_message: Option<String>) -> Option<String>` : le JSON `{"hookSpecificOutput":{"hookEventName":…,"additionalContext":…},"systemMessage":…}`, champs absents omis, `None` si les deux sont vides.
  - `state::{already_asked(session_id, key) -> bool, mark_asked(session_id, key)}` : fichier `hook-state.json` dans le dossier de la base (`KAIROS_DB` respecté), les 50 dernières sessions ; toute erreur est ignorée (on redemande).
  - `rules::SESSION_RULES: &str` : le texte des règles de `SessionStart`.

- [ ] **Step 1: Tests de format (RED)** — avant d'écrire, lire la documentation actuelle des hooks de Claude Code (champs d'entrée `session_id`, `cwd`, `hook_event_name`, `prompt` ; champs de sortie `hookSpecificOutput.additionalContext`, `systemMessage`) et corriger les noms ici si elle diffère. Tests unitaires de `output::render` (les deux champs, un seul, aucun → `None`, `hookEventName` correct pour les deux événements) et de `input::read` (valide, champs en trop, JSON invalide, vide, 2 Mio de texte → lu jusqu'à 1 Mio sans paniquer).
- [ ] **Step 2: Tests de bout en bout du hook (RED)** dans `tests/hook.rs` : lancer `kairos hook user-prompt` / `session-start` avec le JSON sur l'entrée standard, base temporaire, heure figée (`KAIROS_NOW`, `KAIROS_TZ_OFFSET=0`), un dépôt Git temporaire quand la source Git compte. Pour chaque cas : code de sortie 0 et sortie standard vide ou JSON valide.
  - `auto` (réglé par `kairos config claude_autonomy auto`) : message « on attaque Bundle, ticket 12406850311 » rien en cours → lot lancé sur `12406850311`, `systemMessage` qui contient « lancé » et « nouveau parent », `additionalContext` qui contient « set_label » ; deuxième message avec `12406856533` → bascule, `systemMessage` qui contient la commande `kairos switch --parent 12406850311` pour revenir ; message avec l'ID du lot en cours → sortie vide.
  - `ask` (défaut) : même message → aucun lot lancé (`status` = idle), `additionalContext` qui contient `12406850311` et « switch ».
  - Doutes : deux IDs dans le message ; numéro nu inconnu sans repère (`1792774800`) ; trois parents dans les commits et rien en cours ; branche `feature/12406850311-bundle` avec lot en cours `12406856533` → aucune action, `additionalContext` qui contient « Demande à l'utilisateur » ; **le même doute n'est pas répété** au deuxième appel avec le même `session_id`, mais l'est avec un autre `session_id`.
  - Sources : commit `[Monday #12406850311, #12406856533]` et lot en cours `12406856533` → silence ; `.kairos.json` valide → lot lancé avec son libellé et sa sous-tâche (`auto`) ; `.kairos.json` invalide → ignoré.
  - Consigne du libellé : lot lancé sans nom → la consigne `set_label` n'apparaît qu'une fois par parent et par session.
  - `session-start` : rien en cours → contient « aucun lot » et les règles (« ne jamais arrêter un lot en fin de réponse », « Ne devine pas », `recent`, `back`, `set_label`) ; lot en cours → contient son libellé et sa durée.
  - Robustesse : entrée vide, `pas du json`, 2 Mio de texte, JSON sans `prompt`, base verrouillée par une transaction ouverte (le hook rend après au plus 5 s sans lot lancé), base plus récente (`user_version` 99) → toujours code 0 ; `hook-state.json` remplacé par du texte illisible, ou dossier non inscriptible → fonctionne.
  - Le texte du message n'apparaît dans aucun fichier créé (ni base, ni `hook-state.json`) : on y cherche une phrase distinctive du message.
- [ ] **Step 3: Les voir échouer** — Run: `cargo test -p kairos-cli --test hook` ; Expected: échecs (commande absente).
- [ ] **Step 4: Implémenter** : `hook::run` enchaîne lecture, résolution des IDs de chaque source (`extract_ids` puis `resolve_monday_id` : un sous-tâche mémorisée donne son parent et sa sous-tâche), `decide`, action éventuelle (`start` si rien ne tourne, `switch_to` sinon, avec `Source::ClaudeCode`), sortie. Tout `Result` en erreur se termine par un message sur la sortie d'erreur et le code 0. Git est lancé avec `std::process::Command`, `current_dir(cwd)`, délai de 2 s par un thread et `try_wait` (le processus est tué au délai).
- [ ] **Step 5: Les voir passer** — Run: `cargo test --workspace` ; Expected: tout vert, clippy et fmt propres.
- [ ] **Step 6: Commit** — `feat(cli): kairos hook (session-start et user-prompt)`

---

### Task 5: `kairos hooks install` / `uninstall` et la commande `/kairos`

**Files:**

- Create: `crates/kairos-cli/src/hook/install.rs`, `crates/kairos-cli/src/hook/kairos_command.md` (inclus par `include_str!`)
- Modify: `crates/kairos-cli/src/args.rs` (`Command::Hooks { action: HooksAction }` avec `install [--dry-run]` et `uninstall`), `crates/kairos-cli/src/main.rs`
- Test: `crates/kairos-cli/tests/hooks_install.rs` (nouveau, `CLAUDE_CONFIG_DIR` sur un dossier temporaire)

**Interfaces:**

- Consumes: le chemin du binaire courant (`std::env::current_exe()` puis `canonicalize`).
- Produces: `install::run(action, dry_run) -> i32` (0 réussi, 1 refus avec message). Dossier de configuration : `$CLAUDE_CONFIG_DIR`, sinon `$HOME/.claude`. Hooks ajoutés sous `hooks.SessionStart` et `hooks.UserPromptSubmit` : `{"hooks":[{"type":"command","command":"\"<exe>\" hook session-start","timeout":5}]}` (et `user-prompt`). Sauvegarde : `settings.json.kairos-backup-<secondes epoch>`. Fichier `commands/kairos.md`.

- [ ] **Step 1: Tests (RED)** dans `tests/hooks_install.rs` :
  - `install_creates_settings_and_command_when_absent` : `settings.json` créé avec les deux hooks (commande absolue, délai 5), `commands/kairos.md` créé, aucune sauvegarde (rien à sauvegarder).
  - `install_keeps_existing_settings_and_other_hooks` : un `settings.json` avec une clé `permissions`, un hook `PreToolUse` et un autre hook `UserPromptSubmit` d'un autre outil → tout est conservé tel quel, les hooks de Kairos ajoutés, une sauvegarde identique à l'original existe.
  - `install_twice_does_not_duplicate` : deux installations → un seul hook de Kairos par événement.
  - `dry_run_writes_nothing_and_says_what_it_would_do`.
  - `invalid_settings_json_is_refused_and_nothing_is_touched` : code 1, message qui nomme le fichier, ni `settings.json` modifié, ni `commands/kairos.md` créé.
  - `a_different_kairos_command_file_is_not_overwritten` : code 1, ni `settings.json` ni le fichier existant modifiés.
  - `uninstall_removes_only_kairos_hooks_and_the_command` : les hooks des autres outils et les réglages restent ; relancée, elle ne fait rien et sort en 0.
  - `command_file_mentions_status_and_switch_and_uses_arguments` : le fichier contient `$ARGUMENTS`, « recent » et « switch ».
- [ ] **Step 2: Les voir échouer** — Run: `cargo test -p kairos-cli --test hooks_install` ; Expected: échecs (commande absente).
- [ ] **Step 3: Implémenter** : valider d'abord les deux cibles (JSON valide, fichier de commande identique ou absent), ensuite sauvegarder, ensuite écrire (écriture atomique : fichier temporaire puis renommage) ; reconnaître les hooks de Kairos à une commande qui se termine par ` hook session-start` / ` hook user-prompt` et dont le programme s'appelle `kairos`.
- [ ] **Step 4: Les voir passer** — Run: `cargo test --workspace` ; Expected: tout vert, clippy et fmt propres.
- [ ] **Step 5: Commit** — `feat(cli): kairos hooks install et uninstall, commande /kairos`

---

### Task 6: Bout en bout, documentation et essai réel

**Files:**

- Modify: `README.md`, page Notion « Claude et Kairos » (via le connecteur Notion)
- Test: scénario dans `crates/kairos-cli/tests/hook.rs`

- [ ] **Step 1: Test de bout en bout (RED)** `urgent_interruption_and_return` : `auto`, base temporaire, horloge figée avancée par `KAIROS_NOW` entre les appels. (1) Le message « feature Bundle 12406850311 » lance Bundle. (2) « urgence de Manon 12406856533 » bascule vers l'urgence. (3) Un message sans ID (« go, retourne sur Bundle ») ne change rien et la sortie est vide. (4) `kairos back` ramène Bundle ; `kairos recent --json` rend Bundle (courant) puis l'urgence. (5) `kairos report --json` : les deux parents ont chacun leur temps, et le total de Bundle additionne ses deux entrées.
- [ ] **Step 2: Le faire passer** (il doit passer sans autre code ; sinon corriger la tâche fautive).
- [ ] **Step 3: README** : section « Brancher Claude (MCP) » complétée — `kairos hooks install` (ce que ça modifie, la sauvegarde, `--dry-run`, `uninstall`), les 17 outils, la règle du doute (« dans le doute Claude te demande »), le réglage `claude_autonomy` pour les hooks, `/kairos`, et le fichier `.kairos.json` avec un exemple.
- [ ] **Step 4: Notion** : mettre à jour « Claude et Kairos » (hooks restreints à `SessionStart` et `UserPromptSubmit`, `Stop` reporté à l'étape 2, règle du doute, `recent` / `back` / `set_label`, détection par la forme de l'ID).
- [ ] **Step 5: Vérification finale** — Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && npm run verify && npx prettier --check .` ; Expected: tout vert.
- [ ] **Step 6: Essai réel, sur une copie de la configuration** : `CLAUDE_CONFIG_DIR=<copie de ~/.claude> kairos hooks install --dry-run` puis sans `--dry-run`, lancer Claude Code avec cette variable dans un dépôt de test, vérifier `SessionStart` (le contexte contient les règles) et un message avec un ID (selon `claude_autonomy`). La vraie configuration de Lucas n'est modifiée qu'avec son accord.
- [ ] **Step 7: Commit** — `docs: hooks de Claude Code, recent, back, label`
