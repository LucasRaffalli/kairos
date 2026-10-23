# Kairos, étape 0 : le socle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Un dépôt Kairos qui compile, se teste, se publie et se met à jour comme Charon : icône de barre de menus, popover vide aux couleurs de Kairos, CI, release signée et updater.

**Architecture:** Workspace Cargo (`crates/kairos-core`, `crates/kairos-cli`) plus l'app Tauri v2 dans `src-tauri/` et un front Angular 22 zoneless dans `src/`, sur le modèle de Charon (`~/Documents/charon`). Les scripts de release, de déploiement et les workflows GitHub sont copiés de Charon puis adaptés. Aucune règle métier : elles arrivent à l'étape 1.

**Tech Stack:** Rust 1.99, Tauri v2 (`tauri-plugin-updater`, `-process`, `-opener`), Angular 22 (zoneless, signaux, OnPush, SCSS), Node 24, clap, Prettier, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-10-23-etape-0-socle-design.md`

## Global Constraints

- Identifiant d'app `com.aegis.kairos` ; nom produit `Kairos` ; version de départ `0.1.0`, écrite aux trois endroits (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`).
- Dépôt `https://github.com/LucasRaffalli/kairos` ; endpoint updater `https://release.lucasraffalli.com/kairos/latest.json` ; tap `LucasRaffalli/homebrew-kairos` ; hébergement des releases : bucket R2 `kairos-releases` + Worker `kairos-releases`, route `release.lucasraffalli.com/kairos/*` uniquement (jamais `/charon/*`).
- Le crate de l'app reste dans `src-tauri/` ; `kairos-core` et `kairos-cli` sont dans `crates/`.
- Angular : zoneless, signaux uniquement, `OnPush`, sans rxjs ni router, i18n FR et EN, CSP stricte de Charon (`unsafe-eval` en dev seulement).
- `ActivationPolicy::Accessory` : pas d'icône dans le Dock.
- Accents : Kairos (Iris, défaut), Charon, Jade, Unloved, Unicorn caché. Pas de dégradés. Iris : plein `#8458ee`, clair `#c6aeff`, profond `#6440d8`. Fonds sombres : fond `#100f15`, popover `#18171e`, champs `#211f28`, bordures `#38353f`. Focus d'un champ : contour 1 px d'accent + ombre pleine 3 px sans flou à 26 %.
- `data-theme` ne vaut jamais « auto » : c'est le thème réellement peint.
- Qualité : `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm run verify`, `npm run format:check`.
- Signature macOS ad hoc (`signingIdentity "-"`). Aucun `CLARITY_KEY`, aucun certificat Apple, aucun PAT.
- Git : `main`, remote `origin`, un commit par tâche, **aucun push** sans demande explicite.

## Review Focus

- Un clic sur l'icône quand le popover est déjà ouvert : il doit se fermer, pas se rouvrir aussitôt (la perte de focus et le clic arrivent presque en même temps). Test : fonction pure de décision `toggle_action`.
- Endpoint de mise à jour injoignable (hors-ligne, 404) : l'app ne plante pas et dit « aucune mise à jour ». Test : `UpdaterService.check()` sur erreur.
- Une release dont la version est cohérente mais sans entrée de changelog EN (ou avec un nombre de notes différent) : `check-release.cjs` échoue avec un message. Test : le script sur des fixtures.
- Un thème ou un accent auquel il manque une variable utilisée : le popover s'afficherait sans couleur. Test : `scripts/verify/tokens.cjs` compare chaque variable utilisée à chaque thème et accent.
- Popover qui sortirait de l'écran (icône près du bord droit, deuxième écran) : il reste entièrement visible. Test : fonction pure `popover_position`.

---

### Task 1: Workspace Cargo, `kairos-core`, `kairos-cli`, git

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `crates/kairos-core/Cargo.toml`, `crates/kairos-core/src/lib.rs`, `crates/kairos-cli/Cargo.toml`, `crates/kairos-cli/src/main.rs`, `crates/kairos-cli/tests/version.rs`, `.gitignore`
- Run: `git init -b main`, `git remote add origin https://github.com/LucasRaffalli/kairos.git`

**Interfaces:**
- Produces: `kairos_core::version() -> &'static str` (renvoie `env!("CARGO_PKG_VERSION")`) ; binaire `kairos` avec la sous-commande `version` qui écrit `kairos 0.1.0` sur stdout.

- [ ] **Step 1:** `git init -b main` et ajouter le remote. Copier `.gitignore` de Charon (`~/Documents/charon/.gitignore`) en retirant ses lignes propres à Charon (`/samples`, `/docs/mockups`, `/dist-windows`) et en gardant `/dist`, `/dist-macos`, `latest.json`, `scripts/deploy.env`, `target/`.
- [ ] **Step 2:** Écrire le test dans `crates/kairos-core/src/lib.rs` : `version_matches_cargo` affirme `version() == "0.1.0"`, et dans `crates/kairos-cli/tests/version.rs` : `kairos_version_prints_name_and_version` lance le binaire (`env!("CARGO_BIN_EXE_kairos")`) avec `version` et affirme stdout == `"kairos 0.1.0\n"`.
- [ ] **Step 3:** Lancer `cargo test --workspace` : attendu FAIL (crates absents).
- [ ] **Step 4:** Créer le workspace : `Cargo.toml` (`members = ["crates/*", "src-tauri"]`, `resolver = "2"`, `[profile.release]` de Charon : `lto = "fat"`, `codegen-units = 1`, `strip = "symbols"`, panic laissé à `unwind`), `kairos-core` (lib, version 0.1.0), `kairos-cli` (clap derive, sous-commande `version`, dépend de `kairos-core`). Retirer `src-tauri` des `members` tant que la tâche 4 n'existe pas.
- [ ] **Step 5:** `cargo test --workspace` PASS ; `cargo clippy --workspace --all-targets -- -D warnings` sans avertissement.
- [ ] **Step 6:** Commit `feat: workspace Cargo, kairos-core et kairos-cli`.

### Task 2: Projet Angular, tokens, thème et accent

**Files:**
- Create: `package.json`, `angular.json`, `tsconfig.json`, `tsconfig.app.json`, `.prettierrc.json`, `.prettierignore`, `src/index.html`, `src/main.ts`, `src/app/app.config.ts`, `src/app/app.ts`, `src/app/interfaces/appearance/accent.ts`, `src/app/services/appearance/theme.service.ts`, `src/app/services/appearance/secret-accent.service.ts`, `src/styles/_tokens.scss`, `src/styles/styles.scss`, `scripts/verify/tokens.cjs`, `scripts/verify/theme.cjs`
- Reference: `~/Documents/charon/package.json`, `angular.json`, `tsconfig*.json`, `.prettierrc.json`, `src/app/services/appearance/theme.service.ts`, `secret-accent.service.ts`, `src/app/interfaces/appearance/accent.ts`

**Interfaces:**
- Produces: `ACCENTS = ['kairos','charon','jade','unloved','unicorn'] as const`, `type Accent`, `isAccent(value: string | null): value is Accent` ; `ThemeService` avec `preference`, `accent`, `theme` (signaux en lecture seule, `theme()` jamais `'auto'`), `select(theme: ThemePreference)`, `selectAccent(accent: Accent)` (refuse `unicorn` tant qu'il n'est pas trouvé), `activateSecretAccent()`, stockage `kairos:theme`, `kairos:accent`, `kairos:secrets`. `ThemePreference = 'auto' | 'light' | 'dark' | 'contrast'`.

- [ ] **Step 1:** Écrire `scripts/verify/theme.cjs` (modèle : `~/Documents/charon/scripts/verify/theme.cjs`) : charge `theme.service.ts` transpilé et affirme (a) `theme()` vaut `'dark'` ou `'light'` quand la préférence est `'auto'`, jamais `'auto'` ; (b) `selectAccent('unicorn')` est sans effet avant `activateSecretAccent()` puis fonctionne ; (c) un accent caché trouvé survit à un changement d'accent. Écrire `scripts/verify/tokens.cjs` : lit `_tokens.scss`, collecte les variables `var(--…)` utilisées dans `src/**/*.scss` et affirme que chacune est définie pour `[data-theme='dark'|'light'|'contrast']` et pour chaque `[data-accent]`.
- [ ] **Step 2:** `node scripts/verify/theme.cjs` et `tokens.cjs` : attendu FAIL (fichiers absents).
- [ ] **Step 3:** Créer le projet Angular en copiant les configs de Charon (versions `@angular/* ^22.2.1`, `typescript ~6.0.3`, Prettier, `@tauri-apps/api` et plugins `process`, `opener`, `updater`, `notification`), `name: "kairos"`, version `0.1.0`, scripts `start`, `build`, `dev` (`tauri dev --config src-tauri/tauri.dev.conf.json`), `release`, `deploy`, `verify` (chaînant les deux scripts), `format`, `format:check`, `lang:todo`. Sortie de build `dist/kairos/browser`.
- [ ] **Step 4:** Écrire `_tokens.scss` à partir des variables Figma (fichier `st5ct8bxzDkzwnY1UEH1YS`, collections Color et Accent) : variables CSS `--color-bg-app`, `--color-bg-popover`, `--color-bg-surface`, `--color-bg-raised`, `--color-bg-sunken`, `--color-border-default|subtle|strong`, `--color-text-primary|secondary|tertiary|muted`, `--color-accent-text|fill|fill-hover|deep`, `--color-status-warning|success|danger|danger-ring`, `--color-accent-ring`, posées pour `[data-theme='dark']`, `[data-theme='light']`, `[data-theme='contrast']`, puis surchargées par `[data-accent='…']` (rampe d'accent et élévations des fonds sombres). Utiliser les valeurs de la section Global Constraints pour Iris et les fonds ; lire les autres dans Figma (`get_variable_defs`) sans les deviner. Focus : classe `.focus-ring` (contour 1 px + `box-shadow: 0 0 0 3px var(--color-accent-ring)`).
- [ ] **Step 5:** Implémenter `ThemeService` et `SecretAccentService` en reprenant ceux de Charon (clés `kairos:*`, un seul code caché Unicorn, séquence encadrée de flèches, oubli après 1500 ms), `app.ts` affichant un simple conteneur de popover.
- [ ] **Step 6:** `npm ci`, `npm run build` (sans erreur), `node scripts/verify/theme.cjs` et `tokens.cjs` PASS.
- [ ] **Step 7:** Commit `feat: projet Angular, tokens, thèmes et accents`.

### Task 3: Popover vide et i18n

**Files:**
- Create: `src/app/features/popover/popover.ts`, `popover.html`, `popover.scss`, `src/app/lang/i18n.service.ts`, `src/app/lang/fr.ts`, `src/app/lang/en.ts`, `src/assets/glyph.svg`
- Modify: `src/app/app.ts`, `src/app/app.html`
- Reference: `~/Documents/charon/src/app/lang/i18n.service.ts`

**Interfaces:**
- Consumes: `ThemeService` (tâche 2).
- Produces: `injectT(): (key: TranslationKey) => string` ; clés `popover.title` (« Kairos »), `popover.empty` (« Rien en cours »), `popover.date` formaté avec la langue courante ; composant `Popover` (OnPush) : en-tête (glyphe, « Kairos », date du jour) sur `--color-bg-popover`, largeur 340 px.

- [ ] **Step 1:** Test : un script `scripts/verify/lang.cjs` (modèle : `scripts/lang-todo.cjs` de Charon) affirme que `fr.ts` et `en.ts` ont les mêmes clés.
- [ ] **Step 2:** Lancer : FAIL (fichiers absents). Implémenter l'i18n (copie allégée de Charon) et le composant `Popover` ; le glyphe `glyph.svg` est l'étoile à quatre branches du fichier Figma (page « Kairos · logo final », nœud `156:119`), exportée en SVG monochrome.
- [ ] **Step 3:** `npm run build` puis `npm run verify` : PASS.
- [ ] **Step 4:** Commit `feat: popover vide et i18n`.

### Task 4: Coquille Tauri, icône de barre de menus et popover

**Files:**
- Create: `src-tauri/Cargo.toml` (crate `kairos-app`, lib `kairos_lib`), `src-tauri/build.rs`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/placement.rs`, `src-tauri/tauri.conf.json`, `src-tauri/tauri.dev.conf.json`, `src-tauri/tauri.release.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/icons/` (générées)
- Modify: `Cargo.toml` (remettre `src-tauri` dans `members`)
- Reference: `~/Documents/charon/src-tauri/Cargo.toml`, `tauri.conf.json`, `tauri.release.conf.json`

**Interfaces:**
- Produces: `placement::popover_position(tray: Rect, popover: Size, screen: Rect) -> Point` (centré sous l'icône, ramené dans l'écran avec 8 px de marge) ; `tray::toggle_action(visible: bool, just_blurred: bool) -> ToggleAction` avec `enum ToggleAction { Show, Hide }` : `Hide` si le popover est visible, `Show` sinon, sauf si `just_blurred` (perte de focus de moins de 250 ms causée par ce même clic) qui donne `Hide`.

- [ ] **Step 1:** Écrire les tests unitaires : `popover_position` (a) centré quand tout tient, (b) ramené à gauche quand l'icône est au bord droit, (c) ramené sur le deuxième écran, jamais au-dessus du bord haut ; `toggle_action` (visible → Hide ; caché → Show ; caché + `just_blurred` → Hide).
- [ ] **Step 2:** `cargo test -p kairos-app` : FAIL.
- [ ] **Step 3:** Implémenter les deux fonctions pures, puis `lib.rs` : `ActivationPolicy::Accessory`, fenêtre `popover` (340 px de large, sans décoration, non redimensionnable, toujours au-dessus, masquée), icône de barre de menus (modèle monochrome, `icon_as_template(true)`), clic gauche → `toggle_action`, perte de focus → masque, clic droit → menu (Quitter). Config : `productName "Kairos"`, `identifier "com.aegis.kairos"`, `version "0.1.0"`, CSP de Charon, `frontendDist "../dist/kairos/browser"`, `devUrl "http://localhost:1420"`, plugin updater avec l'endpoint de la section Global Constraints et `pubkey` à remplir à la tâche 6 (valeur provisoire documentée dans le README), `signingIdentity "-"`, overlay release identique à celle de Charon (`createUpdaterArtifacts: true`, cibles `app` et `dmg`). Icônes : générer avec `npx tauri icon` à partir d'un PNG 1024 du glyphe (`scripts/make-icons.sh` de Charon, adapté) ; marquer l'icône comme provisoire dans le README.
- [ ] **Step 4:** `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` PASS ; `npm run dev` : l'icône apparaît dans la barre de menus, un clic ouvre le popover vide, un second clic le ferme, pas d'icône dans le Dock (vérification manuelle, à rapporter).
- [ ] **Step 5:** Commit `feat: coquille Tauri, icône de barre de menus et popover`.

### Task 5: Mises à jour dans l'app et changelog

**Files:**
- Create: `src/app/services/system/updater.service.ts`, `whats-new.service.ts`, `changelog-i18n.ts`, `src/assets/changelog.json`, `src/assets/changelog.en.json`
- Reference: `~/Documents/charon/src/app/services/system/updater.service.ts`, `whats-new.service.ts`, `changelog-i18n.ts`

**Interfaces:**
- Produces: `UpdaterService.check(): Promise<UpdateStatus>` avec `type UpdateStatus = { kind: 'none' } | { kind: 'available'; version: string } | { kind: 'error'; message: string }` (ne lève jamais), `install(): Promise<void>` ; `WhatsNewService.entries(): ChangelogEntry[]` triées de la plus récente ; changelog `0.1.0` (« Le socle ») en FR et EN, une note chacune.

- [ ] **Step 1:** Test `scripts/verify/whats-new.cjs` (modèle Charon) : `check()` rend `{ kind: 'error' }` et ne lève pas quand le plugin échoue ; `entries()` est triée ; chaque version du FR a sa traduction EN avec le même nombre de notes.
- [ ] **Step 2:** FAIL ; implémenter les services en copiant ceux de Charon (sans la partie propre à Charon).
- [ ] **Step 3:** `npm run build` et `npm run verify` PASS.
- [ ] **Step 4:** Commit `feat: mises à jour et nouveautés`.

### Task 6: Scripts de release et de déploiement

**Files:**
- Create: `scripts/release.sh`, `scripts/deploy.sh`, `scripts/make-cask.sh`, `scripts/make-latest-json.sh`, `scripts/check-release.cjs`, `scripts/deploy.env.example`, `scripts/verify/release.cjs`, `infra/release-worker/index.js`, `infra/release-worker/wrangler.jsonc`, `scripts/verify/worker.cjs`
- Reference: les mêmes fichiers dans `~/Documents/charon/scripts/`

**Interfaces:**
- Produces: `check-release.cjs [tag]` : sortie 0 si `package.json`, `tauri.conf.json` et `src-tauri/Cargo.toml` portent la même version, que le changelog FR a une entrée pour elle, que le changelog EN a le même nombre de notes, et que le tag (s'il est donné) vaut `v<version>` ; sinon sortie 1 et un message par erreur.

- [ ] **Step 1:** Test `scripts/verify/release.cjs` : exécute `check-release.cjs` sur des fixtures temporaires (versions divergentes, changelog EN manquant, nombre de notes différent, tag faux) et affirme sortie 1 avec le message attendu, puis sortie 0 sur le dépôt courant.
- [ ] **Step 2:** FAIL ; copier et adapter les scripts : remplacer `charon`/`Charon` par `kairos`/`Kairos`, `LucasRaffalli/charon` par `LucasRaffalli/kairos`, l'envoi `scp` par `npx wrangler r2 object put "$R2_BUCKET/<clé>" --file <fichier> --remote` (clés `latest.json`, `Kairos_<version>_aarch64.dmg`, `Kairos.app.tar.gz`, `Kairos.app.tar.gz.sig`), `PUBLIC_URL` par `https://release.lucasraffalli.com/kairos`, tap `homebrew-kairos`, cask `kairos`, description « Chronomètre de barre de menus pour Monday ». Garder `RETAG=1`, la vérification de la version de l'archive, le `GH_TOKEN` facultatif.
- [ ] **Step 3:** `node scripts/verify/release.cjs` PASS ; `bash -n scripts/*.sh` sans erreur ; `bash scripts/make-latest-json.sh https://exemple.test/kairos` sans archive doit échouer avec le message prévu.
- [ ] **Step 4:** Commit `feat: scripts de release et de déploiement`.

### Task 6 bis: Worker Cloudflare et ressources R2

**Files:**
- Create: `infra/release-worker/index.js`, `infra/release-worker/wrangler.jsonc`, `scripts/verify/worker.cjs`

**Interfaces:**
- Produces: Worker `export default { fetch(request, env): Promise<Response> }` : méthodes `GET` et `HEAD` seulement (sinon 405) ; le chemin `/kairos/<clé>` lit `env.RELEASES.get(<clé>)` ; clé absente ou chemin hors `/kairos/` → 404 ; `latest.json` renvoyé en `application/json` avec `Cache-Control: no-cache`, les autres fichiers avec `ETag` et `Cache-Control: public, max-age=3600` ; jamais de listing du bucket ; les clés avec `..` ou vides → 404.

- [ ] **Step 1:** Test `scripts/verify/worker.cjs` : importe `index.js` avec un faux `env.RELEASES` et affirme : `GET /kairos/latest.json` 200 JSON no-cache ; `GET /kairos/absent` 404 ; `GET /kairos/` 404 ; `GET /kairos/../x` 404 ; `POST` 405 ; `GET /charon/latest.json` 404 ; `HEAD` sans corps.
- [ ] **Step 2:** FAIL ; implémenter le Worker et `wrangler.jsonc` (nom `kairos-releases`, binding `RELEASES` vers le bucket `kairos-releases`, route `release.lucasraffalli.com/kairos/*`, zone `lucasraffalli.com`).
- [ ] **Step 3:** `node scripts/verify/worker.cjs` PASS. Puis, avec le connecteur Cloudflare : créer le bucket `kairos-releases`, téléverser le Worker, créer la route `release.lucasraffalli.com/kairos/*` ; vérifier qu'une requête sur `https://release.lucasraffalli.com/kairos/latest.json` répond 404 (clé absente) et que `https://release.lucasraffalli.com/charon/latest.json` répond toujours comme avant.
- [ ] **Step 4:** Commit `feat: Worker Cloudflare pour les releases`.

### Task 7: GitHub (CI, audit, release, Dependabot) et README

**Files:**
- Create: `.github/workflows/ci.yml`, `audit.yml`, `release.yml`, `.github/dependabot.yml`, `.github/ISSUE_TEMPLATE/{bug.yml,idea.yml,config.yml}`, `README.md`
- Reference: `~/Documents/charon/.github/`

- [ ] **Step 1:** Copier les workflows de Charon. `ci.yml` : jobs `angular` (ubuntu : `npm ci`, `format:check`, `verify`, `build`, `check-release`) et `rust` (macOS : clippy et tests du workspace). `audit.yml` : `cargo audit` sur le workspace. `release.yml` : `preflight` puis job `macos` seul (le job Windows reste en commentaire daté), `workspaces: .` pour `rust-cache`, secrets `TAURI_SIGNING_PRIVATE_KEY` et `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` seulement, artefact `kairos-macos`. `dependabot.yml` : cargo sur `/`, npm sur `/`, actions ; mêmes groupes que Charon, sans l'exception TypeScript propre à Charon si elle n'a plus lieu d'être (la garder tant que Angular exige `<6.1`).
- [ ] **Step 2:** README : ce qu'est Kairos (lien vers la page Notion), commandes (`npm run dev|build|verify|release|deploy`), et la section « Mise en place de GitHub » : les deux secrets, `npx tauri signer generate -w ~/.tauri/kairos-updater.key` et où coller la clé publique, Workflow permissions *Read and write*, Dependabot alerts et security updates, `npx wrangler login` une fois, bucket R2 `kairos-releases` et Worker (lien vers la page Notion), tap `homebrew-kairos`, `scripts/deploy.env`.
- [ ] **Step 3:** Valider la syntaxe YAML (`python3 -c "import yaml,sys; [yaml.safe_load(open(f)) for f in sys.argv[1:]]" .github/workflows/*.yml .github/dependabot.yml`) : aucune erreur.
- [ ] **Step 4:** Commit `ci: workflows GitHub, Dependabot et README`.

### Task 8: Vérification d'ensemble

- [ ] **Step 1:** `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm run verify`, `npm run format:check`, `npm run build` : tout vert, sorties rapportées.
- [ ] **Step 2:** `npx tauri build --config src-tauri/tauri.release.conf.json --debug` n'est pas lancé ici (la CI s'en charge) ; lancer `npm run dev` une dernière fois et rapporter ce qu'on voit.
- [ ] **Step 3:** Mettre à jour la page Notion « Technique » (dossier `src/` à la place de `ui/`, `crates/` pour core et cli) et « Distribution » (chemins réels) ; commit final `docs: état de l'étape 0`.
- [ ] **Step 4:** Lister à Lucas ce qui reste à faire de son côté (clé de signature, secrets, `wrangler login`, tap, premier push) sans rien pousser.
