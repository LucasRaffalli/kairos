# Kairos, étape 1b-bis : distribuer le MCP

Date : 24 octobre 2026. Sur la branche `feat/etape-1b`, à la suite du serveur MCP (`kairos mcp`). Ce document ne décrit que la **distribution** : faire arriver le binaire `kairos` chez les gens, et brancher Claude sans éditer de fichier.

## But

Aujourd'hui `kairos mcp` n'existe que si on compile le dépôt. Après ce sous-projet, une personne sur Mac Apple Silicon :

1. fait `brew install --cask lucasraffalli/kairos/kairos` : l'app **et** la commande `kairos` (dans le PATH) arrivent ;
2. branche Claude Code avec `claude mcp add kairos --scope user -- kairos mcp` ;
3. ou, pour Claude Desktop, double-clique sur `Kairos.mcpb` : l'extension s'installe sans éditer de JSON.

Hors périmètre : Windows et Intel (comme le DMG actuel), la notarisation Apple, l'installation de la commande depuis l'app (elle viendra avec l'écran Réglages de l'étape 2 pour ceux qui ont installé le DMG sans Homebrew), les hooks (1c).

## Décisions

- **Le binaire `kairos` vit dans l'app.** Il est empaqueté dans `Kairos.app/Contents/MacOS/kairos` (mécanisme `externalBin` de Tauri). Un seul artefact, une seule signature, et la mise à jour de l'app met à jour la commande, puisque le lien du PATH pointe dans l'app. Le binaire principal s'appelle `kairos-app` : pas de conflit de nom.
- **Le cask ajoute un lien.** `binary "#{appdir}/Kairos.app/Contents/MacOS/kairos"` met `kairos` dans le PATH de Homebrew. Pas de second téléchargement.
- **L'extension `.mcpb` est un second artefact de la release**, distinct du DMG : un zip contenant `manifest.json`, `server/kairos` (le même binaire) et l'icône. Pas de lien avec le PATH : Claude Desktop lance le binaire du paquet.
- **Un seul build du binaire par release**, fait par la CI, copié à la fois dans l'app et dans le `.mcpb` (même empreinte).
- **Signature ad hoc**, comme l'app (`codesign -s -`). Pour le `.mcpb` téléchargé, la quarantaine peut s'appliquer : c'est le principal risque, vérifié par un essai réel avant d'annoncer la fonction (voir Vérifications).

## Pièces

### 1. Compilation du binaire en CI (`release.yml`)

Avant `tauri build`, une étape : `cargo build --release -p kairos-cli --target aarch64-apple-darwin`, copie vers `src-tauri/binaries/kairos-aarch64-apple-darwin`, `codesign -s -`. `src-tauri/tauri.release.conf.json` (et `tauri.conf.json` pour les builds locaux) déclare `bundle.externalBin: ["binaries/kairos"]`. Le dossier `binaries/` est ignoré par git. Un build local d'app sans le binaire échoue clairement : `npm run build:cli` (nouveau) le produit.

### 2. Le cask (`make-cask.sh`)

Ajoute la ligne `binary` au gabarit. Désinstaller l'app retire le lien. Le `postflight` existant (`xattr -cr`) couvre déjà le binaire, puisqu'il est dans l'app.

### 3. Le paquet `.mcpb`

Nouveau `scripts/make-mcpb.sh` : assemble `manifest.json` (nom `kairos`, version tirée de `tauri.conf.json`, `server.type: "binary"`, `entry_point: "server/kairos"`, `mcp_config.command: "${__dirname}/server/kairos"`, `args: ["mcp"]`, plateforme `darwin`), l'icône, et le binaire extrait de l'app de la release ; zippe en `Kairos_<version>.mcpb`. Le gabarit du manifeste est un fichier du dépôt (`packaging/mcpb/manifest.json`), pas une chaîne dans le script.

### 4. Release et déploiement

- `release.yml` : publie aussi le `.mcpb` sur la release GitHub (le build vient après l'app).
- `deploy.sh` : récupère le `.mcpb`, vérifie que le binaire dedans a la version du tag, l'envoie sur R2 sous un nom versionné, **avant** `latest.json`.
- `check-release.cjs` : le changelog de la version (français et anglais) doit parler du MCP ; le `.mcpb` n'est pas contrôlé ici (il n'existe pas encore).
- Le Worker de R2 sert déjà `kairos/*` : aucun changement, vérifier que `.mcpb` a un bon type MIME.

### 5. Documentation

README : section « Brancher Claude (MCP) » réécrite autour de l'installation Homebrew et du `.mcpb`, avec le chemin de compilation gardé pour le développement. Page Notion « Claude et Kairos » à jour. Changelog 0.2.0 (fr + en).

## Vérifications avant d'annoncer

1. `cargo build` du binaire en CI, signature ad hoc valide (`codesign -v`), `kairos version` s'exécute depuis l'app téléchargée.
2. Le cask : `brew install` depuis un tap local fait apparaître `kairos` dans le PATH ; `brew uninstall` le retire. À tester sur cette machine.
3. Le `.mcpb` : manifeste valide, installation réelle dans Claude Desktop, appel d'un outil (`status`), **avec le fichier téléchargé** (quarantaine comprise). Si Gatekeeper bloque, on le documente (clic droit → ouvrir, ou `xattr -d`) ou on choisit une autre voie.
4. Le test de bout en bout existant (`kairos mcp` sur stdio) tourne aussi sur le binaire extrait de l'app.

## Cas limites à traiter

- Release sans binaire dans l'app : `deploy.sh` échoue avec un message clair (comme pour l'archive).
- Binaire du `.mcpb` d'une autre version que le tag : `deploy.sh` refuse.
- Deux installations (Homebrew et DMG manuel) : le lien `kairos` pointe sur celle de Homebrew ; le README le dit.
- Mise à jour par l'updater de l'app : le binaire change avec l'app, le lien reste valide ; le `.mcpb` déjà installé, lui, ne se met pas à jour tout seul (Claude Desktop propose la nouvelle version si le fichier est ré-ouvert) : noté dans le README.

## Ordre de travail (esquisse)

1. Build du binaire + `externalBin` + essai local d'un bundle.
2. Cask avec `binary`, essai avec un tap local.
3. `make-mcpb.sh` + manifeste, essai dans Claude Desktop.
4. CI, `deploy.sh`, `check-release`.
5. Documentation, changelog, puis release 0.2.0 (sur ton feu vert).

## Écarts décidés à l'exécution

- `externalBin` seulement dans l'overlay de release (Tauri l'exige dès `cargo check`).
- Le `.mcpb` est assemblé par `deploy.sh` à partir de l'archive de l'app publiée, et non par `release.yml` : même binaire que le cask.
- `mcp_config.command` vaut `/bin/sh` et lance `server/start.sh`, qui lève la quarantaine puis lance `kairos mcp` : un binaire ad hoc en quarantaine est tué au lancement (code 137, constaté).
- Pas de contrôle du changelog sur le MCP dans `check-release.cjs`.
