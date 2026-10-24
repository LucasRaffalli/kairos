# Étape 1b-bis : distribuer le MCP, plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Le binaire `kairos` arrive avec l'app (cask Homebrew, dans le PATH) et un paquet `Kairos_<version>.mcpb` installe le serveur MCP dans Claude Desktop en un double-clic.

**Architecture:** La CI compile `kairos-cli` une fois, signe ad hoc, l'embarque dans l'app via `externalBin` de Tauri (overlay de release seulement) ; le cask lie `Contents/MacOS/kairos` au PATH ; `make-mcpb.sh` extrait ce même binaire de l'archive de l'app pour assembler le `.mcpb`, que `deploy.sh` vérifie puis envoie sur R2 avant `latest.json`.

**Tech Stack:** Tauri v2 (`externalBin`), Cargo, bash, node (scripts de `scripts/verify`), GitHub Actions, Homebrew cask, wrangler/R2, format MCPB (zip + `manifest.json`).

**Spec:** `docs/superpowers/specs/2026-10-24-etape-1b-bis-distribution-design.md`

## Global Constraints

- Mac Apple Silicon seulement : cible `aarch64-apple-darwin`.
- Signature ad hoc (`codesign -s -`), ni certificat ni notarisation.
- Un seul build du binaire par release, copié dans l'app **et** dans le `.mcpb` (même empreinte).
- Le binaire principal de l'app reste `kairos-app` ; le binaire embarqué s'appelle `kairos`.
- Nom du paquet : `Kairos_<version>.mcpb`, versionné (le Worker met en cache une heure).
- `latest.json` part en dernier dans `deploy.sh`.
- Textes en français, comme le reste des scripts.
- Aucune ligne d'attribution dans les messages de commit.

## Review Focus

1. Le binaire du `.mcpb` n'est pas de la version du tag : `deploy.sh` doit refuser (test en Task 3).
2. L'archive de l'app ne contient pas `kairos` : `make-mcpb.sh` et `deploy.sh` échouent avec un message clair, jamais un `.mcpb` vide (Task 3).
3. Le build CI du workspace (`clippy`, `test`) ne doit pas exiger le binaire embarqué : `externalBin` est dans l'overlay de release seulement (Task 1).
4. Le binaire extrait garde son bit exécutable et sa signature dans le zip (Task 3).
5. Le manifeste doit rester valide quand la version change : elle vient de `tauri.conf.json`, pas d'une copie (Task 3).

---

## Écart assumé avec le spec

Le spec dit « `tauri.release.conf.json` (et `tauri.conf.json` pour les builds locaux) ». Tauri exige que l'`externalBin` existe dès `cargo check` : le déclarer dans `tauri.conf.json` casserait `clippy`, `tauri dev` et la CI Rust. Il va donc **seulement** dans l'overlay de release. Le spec parlait aussi d'un contrôle du changelog sur le MCP dans `check-release.cjs` : abandonné, il ne protège de rien de réel ; le changelog 0.2.0 est écrit à la release via `npm run bump`.

## Structure des fichiers

- Create `scripts/build-cli.sh` : compile, signe et dépose le binaire pour `externalBin`.
- Modify `src-tauri/tauri.release.conf.json` : `bundle.externalBin`.
- Modify `.gitignore` : `src-tauri/binaries/`.
- Modify `package.json` : script `build:cli`, `verify` étendu.
- Modify `scripts/make-cask.sh` : ligne `binary`.
- Create `packaging/mcpb/manifest.json` : gabarit du manifeste.
- Create `scripts/make-mcpb.sh` : assemble le paquet.
- Create `scripts/verify/cask.cjs`, `scripts/verify/mcpb.cjs` : vérifient les scripts réels contre des fixtures.
- Modify `.github/workflows/release.yml`, `scripts/deploy.sh`.
- Modify `README.md`.

---

### Task 1: Le binaire embarqué dans l'app

**Files:**
- Create: `scripts/build-cli.sh`
- Modify: `src-tauri/tauri.release.conf.json`, `.gitignore`, `package.json`, `.github/workflows/release.yml`

**Interfaces:**
- Produces: `src-tauri/binaries/kairos-aarch64-apple-darwin` (exécutable, signé ad hoc), appelé par `npm run build:cli` ; l'app construite avec l'overlay de release contient `Kairos.app/Contents/MacOS/kairos`.

- [ ] **Step 1: Écrire `scripts/build-cli.sh`** : `cargo build --release -p kairos-cli --target aarch64-apple-darwin`, copie vers `src-tauri/binaries/kairos-aarch64-apple-darwin`, `chmod +x`, `codesign --force -s -` dessus. Échoue avec un message clair hors macOS arm64 (`uname -m`).
- [ ] **Step 2: Déclarer** `"externalBin": ["binaries/kairos"]` dans `bundle` de `tauri.release.conf.json` ; ajouter `src-tauri/binaries/` au `.gitignore` ; ajouter `"build:cli": "bash scripts/build-cli.sh"` dans `package.json`.
- [ ] **Step 3: Vérifier en vrai** (le test de cette tâche, c'est le bundle) :
  Run: `npm run build:cli && codesign -v src-tauri/binaries/kairos-aarch64-apple-darwin && src-tauri/binaries/kairos-aarch64-apple-darwin version`
  Expected: `kairos 0.1.0`, sans erreur de signature.
- [ ] **Step 4: Construire l'app avec l'overlay, sans la clé de l'updater** :
  Run: `npx tauri build --bundles app --config src-tauri/tauri.release.conf.json --config '{"bundle":{"createUpdaterArtifacts":false}}'`
  Then: `target/release/bundle/macos/Kairos.app/Contents/MacOS/kairos version`
  Expected: `kairos 0.1.0` ; `ls Kairos.app/Contents/MacOS` montre `kairos` et `kairos-app`.
- [ ] **Step 5: CI** — dans `release.yml`, ajouter avant « Build Tauri » une étape `npm run build:cli`. Vérifier que `ci.yml` n'a pas besoin du binaire (Review Focus 3) : `cargo clippy --workspace --all-targets -- -D warnings` passe toujours en local sans `src-tauri/binaries/`.
- [ ] **Step 6: Commit** — `feat(release): le binaire kairos embarqué dans l'app`

---

### Task 2: Le cask lie `kairos` au PATH

**Files:**
- Modify: `scripts/make-cask.sh`, `package.json` (`verify`)
- Create/Test: `scripts/verify/cask.cjs`

**Interfaces:**
- Consumes: `make-cask.sh` existant (lit `dist-macos/*.dmg`, `TAP_DIR`, `PUBLIC_URL`).
- Produces: `Casks/kairos.rb` contenant `binary "#{appdir}/Kairos.app/Contents/MacOS/kairos"`.

- [ ] **Step 1: Test d'abord** — `scripts/verify/cask.cjs` (même style que `verify/release.cjs`) : fixture avec un faux `dist-macos/Kairos_0.1.0_aarch64.dmg`, un `TAP_DIR` temporaire, `PUBLIC_URL` en variable d'environnement ; lance le vrai `make-cask.sh` ; vérifie que la cask contient la ligne `binary "#{appdir}/Kairos.app/Contents/MacOS/kairos"` (littéral Ruby, pas interpolé par bash), le `sha256` du faux DMG, et que `app "Kairos.app"` est toujours là.
- [ ] **Step 2: Le voir échouer** — Run: `node scripts/verify/cask.cjs` ; Expected: `ÉCHEC` sur la ligne `binary`.
- [ ] **Step 3: Ajouter la ligne `binary`** juste après `app "Kairos.app"` dans le gabarit (guillemets échappés pour que `#{appdir}` reste littéral dans l'heredoc).
- [ ] **Step 4: Le voir passer** — Run: `node scripts/verify/cask.cjs` ; Expected: sortie 0. Ajouter `node scripts/verify/cask.cjs` à `npm run verify`.
- [ ] **Step 5: Essai réel avec un tap local** (après Task 1, avec le vrai bundle) : fabriquer un DMG local depuis le `.app` de la Task 1, générer la cask dans un tap temporaire, `brew install --cask` depuis ce chemin ; `which kairos` puis `kairos version` ; `brew uninstall --cask kairos` ; `which kairos` ne trouve plus rien. Noter le résultat dans le message de commit.
- [ ] **Step 6: Commit** — `feat(cask): la commande kairos dans le PATH`

---

### Task 3: Le paquet `.mcpb`

**Files:**
- Create: `packaging/mcpb/manifest.json`, `scripts/make-mcpb.sh`, `scripts/verify/mcpb.cjs`
- Modify: `package.json` (`verify`)

**Interfaces:**
- Consumes: `dist-macos/*.app.tar.gz` (archive de l'app de la release, contient `Kairos.app/Contents/MacOS/kairos`) ; version lue dans `src-tauri/tauri.conf.json`.
- Produces: `scripts/make-mcpb.sh [dossier-de-sortie]` écrit `Kairos_<version>.mcpb` (zip) contenant `manifest.json`, `server/kairos` (exécutable), `icon.png` ; sortie 1 avec un message si l'archive ou le binaire manque. `scripts/make-mcpb.sh --check <fichier.mcpb> <version>` sort 1 si le binaire du paquet n'annonce pas `kairos <version>` (pour `deploy.sh`).

- [ ] **Step 1: Écrire `packaging/mcpb/manifest.json`** : `manifest_version`, `name: "kairos"`, `display_name: "Kairos"`, description française, `version: "__VERSION__"` (remplacé à l'assemblage), `server: { type: "binary", entry_point: "server/kairos", mcp_config: { command: "${__dirname}/server/kairos", args: ["mcp"] } }`, `compatibility.platforms: ["darwin"]`, `icon: "icon.png"`. Avant d'écrire, lire la spécification MCPB officielle (dépôt `anthropics/mcpb`, `MANIFEST.md`) : vérifier les noms de champs et la version de `manifest_version`.
- [ ] **Step 2: Tests d'abord** — `scripts/verify/mcpb.cjs` : crée une fausse archive `Kairos.app/Contents/MacOS/kairos` (script shell qui affiche `kairos 9.9.9`) dans `dist-macos/` d'une fixture ; lance le vrai `make-mcpb.sh` ; vérifie : le zip existe sous `Kairos_<version>.mcpb`, `manifest.json` est du JSON valide avec la version de la fixture et sans `__VERSION__`, `server/kairos` est exécutable dans le zip (`unzip -Z`), `icon.png` présent. Cas d'échec : archive sans `kairos` → sortie 1, message qui nomme le fichier manquant, aucun `.mcpb` écrit (Review Focus 2) ; `--check` avec une version différente de celle du binaire → sortie 1 (Review Focus 1).
- [ ] **Step 3: Les voir échouer** — Run: `node scripts/verify/mcpb.cjs` ; Expected: `ÉCHEC` (script absent).
- [ ] **Step 4: Écrire `make-mcpb.sh`** : extraire le binaire de l'archive (`tar -xzOf … Kairos.app/Contents/MacOS/kairos`), remplir `__VERSION__` depuis `tauri.conf.json`, zipper avec `zip -X` en conservant le bit exécutable ; l'icône vient de `src-tauri/icons/128x128.png`. `--check` extrait `server/kairos` dans un dossier temporaire et compare `kairos version`.
- [ ] **Step 5: Les voir passer** — Run: `node scripts/verify/mcpb.cjs` ; Expected: sortie 0. Ajouter à `npm run verify`.
- [ ] **Step 6: Essai réel dans Claude Desktop** : assembler le `.mcpb` depuis l'app de la Task 1, le télécharger/copier avec attribut de quarantaine (`xattr -w com.apple.quarantine "0081;00000000;Safari;" fichier`), double-clic, appeler `status` depuis Claude Desktop. Si macOS bloque : noter le message exact et la manipulation qui débloque, ou le signaler à Lucas avant de continuer.
- [ ] **Step 7: Commit** — `feat(release): le paquet .mcpb pour Claude Desktop`

---

### Task 4: Release et déploiement

**Files:**
- Modify: `.github/workflows/release.yml`, `scripts/deploy.sh`

**Interfaces:**
- Consumes: `make-mcpb.sh` et `make-mcpb.sh --check` (Task 3), le binaire embarqué (Task 1).
- Produces: sur R2, `Kairos_<version>.mcpb` envoyé avant `latest.json` ; URL annoncée à la fin de `deploy.sh`.

- [ ] **Step 1: `deploy.sh`** — après la vérification de la version de l'archive : exiger que l'archive contienne `Kairos.app/Contents/MacOS/kairos` (sinon message « l'app de la release n'embarque pas le binaire kairos », sortie 1) ; lancer `make-mcpb.sh dist-macos` puis `make-mcpb.sh --check dist-macos/Kairos_$VERSION.mcpb $VERSION` ; l'envoyer avec `put "Kairos_${VERSION}.mcpb" … "application/octet-stream"` avant `latest.json` ; afficher l'URL à la fin.
- [ ] **Step 2: `release.yml`** — rien à publier de plus (le `.mcpb` est assemblé au déploiement à partir de l'archive déjà publiée) ; mettre à jour le commentaire d'en-tête pour le dire. Vérifier la syntaxe : `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml'))"`.
- [ ] **Step 3: Essai à blanc** — Run: `bash -n scripts/deploy.sh` ; Expected: aucune erreur. Le déploiement réel n'a lieu qu'à la release 0.2.0, sur le feu vert de Lucas.
- [ ] **Step 4: Type MIME du Worker** — lire `infra/release-worker/index.js` et `scripts/verify/worker.cjs` ; si `.mcpb` n'a pas de `Content-Type` correct, l'ajouter avec son test dans `worker.cjs`, sinon ne rien changer.
- [ ] **Step 5: Commit** — `feat(release): déploiement du .mcpb sur R2`

---

### Task 5: Documentation et vérification de bout en bout

**Files:**
- Modify: `README.md`

- [ ] **Step 1: README** — réécrire « Brancher Claude (MCP) » : installation par `brew install --cask lucasraffalli/kairos/kairos`, puis `claude mcp add kairos --scope user -- kairos mcp` ; l'extension `.mcpb` pour Claude Desktop (lien `release.lucasraffalli.com/kairos/Kairos_<version>.mcpb`, ce que fait macOS au premier lancement, qu'elle ne se met pas à jour toute seule) ; le chemin de compilation gardé pour le développement ; le cas de deux installations (Homebrew et DMG manuel).
- [ ] **Step 2: Vérification finale** — Run: `npm run verify && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` ; Expected: tout vert.
- [ ] **Step 3: Commit** — `docs: installation du MCP par Homebrew et .mcpb`
- [ ] **Step 4 (hors tâches, sur feu vert de Lucas):** `npm run bump` (0.2.0), notes de changelog fr/en, `npm run release`, `npm run deploy`, puis `brew upgrade`/installation et essai réel du `.mcpb` téléchargé depuis R2.
