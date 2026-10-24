# Kairos

Chronomètre de barre de menus pour macOS. Kairos mesure le temps par **tâche parent** et **sous-tâche** Monday, se pilote à la main (icône, raccourcis) ou par Claude, et prépare chaque soir la liste exacte à saisir dans Monday. Il n'écrit jamais dans Monday : la saisie reste manuelle.

> **Saisir le moment.** Kairos est le dieu grec du moment opportun.

La conception complète (pourquoi, concept, utilisation, cahier des charges, technique, design) est dans la page Notion « Kairos ». Les maquettes sont dans le fichier Figma [Kairos](https://www.figma.com/design/st5ct8bxzDkzwnY1UEH1YS).

**État : étape 0 (le socle).** Une icône dans la barre de menus et un popover vide aux couleurs de l'app ; le chronomètre arrive aux étapes suivantes. L'icône de l'app est **provisoire** (un aplat Iris et le glyphe blanc).

## Utiliser en ligne de commande

Depuis l'étape 1a, `kairos` chronomètre une journée entière sans l'app ni Claude. La base est `~/Library/Application Support/com.aegis.kairos/kairos.db` (`KAIROS_DB` la remplace).

```bash
cargo build -p kairos-cli          # le binaire : target/debug/kairos
kairos start --parent 12406850311 --subtask Front   # lance un lot (sous-tâche par défaut : le poste principal)
kairos switch --parent 12406856533 --subtask Recettage   # change de lot, à la même seconde
kairos pause ; kairos resume
kairos stop
kairos add --name "Réunion" --from 13:00 --to 13:30      # un temps oublié
kairos today                       # les entrées du jour, avec leurs numéros
kairos edit 3 --to 10:30           # corrige une entrée
kairos report                      # la ventilation et la ligne à saisir dans Monday
kairos close-day                   # clôture (--stop arrête d'abord le lot en cours)
kairos mark-declared               # quand tu as saisi dans Monday
kairos config rounding_minutes 15  # arrondi léger (0, 5, 10 ou 15), main_subtask Back…
```

Ajoute `--json` pour une réponse structurée (les erreurs aussi : `{"error":{"code","message"}}`, code de sortie 2). Une entrée ne traverse jamais minuit : un lot encore ouvert à 00:00 est coupé en deux, une entrée par jour.

## Brancher Claude (MCP)

`kairos mcp` est un serveur MCP en stdio : Claude le lance, lui parle en JSON-RPC, et il s'arrête quand l'entrée se ferme. Il expose 14 outils (`start`, `stop`, `pause`, `resume`, `switch`, `status`, `today`, `report`, `add_entry`, `edit_entry`, `delete_entry`, `close_day`, `mark_declared`, `set_subtask_id`) qui appellent le même service que la ligne de commande. Aucun outil ne touche aux réglages.

**Installation (Mac Apple Silicon).** Le binaire `kairos` est dans l'app. Avec Homebrew, il arrive dans le PATH en même temps qu'elle :

```bash
brew install --cask lucasraffalli/kairos/kairos
```

Déjà installé avant la 0.2.0 ? L'app s'est mise à jour toute seule, mais Homebrew n'a pas posé le lien `kairos` (le cask se met à jour par l'app, donc `brew upgrade` l'ignore). Une fois suffit, en mettant d'abord le tap à jour (sans quoi Homebrew réinstalle l'ancienne version) :

```bash
brew update && brew reinstall --cask kairos
```

Installé à la main par le DMG ? `brew install --cask --force lucasraffalli/kairos/kairos` remplace l'app et pose le lien.

**Claude Code** :

```bash
claude mcp add kairos --scope user -- kairos mcp
```

**Claude Desktop** : télécharger `https://release.lucasraffalli.com/kairos/Kairos_<version>.mcpb` (la version est celle de `latest.json`) et double-cliquer. L'extension embarque son propre binaire : elle ne dépend ni de Homebrew ni du PATH. Elle ne se met pas à jour toute seule : pour une nouvelle version, rouvrir le nouveau fichier. À défaut, l'ancienne méthode reste possible : entrée `kairos` sous `mcpServers` dans `~/Library/Application Support/Claude/claude_desktop_config.json`, avec `args: ["mcp"]` et `command: "/Applications/Kairos.app/Contents/MacOS/kairos"` (chemin absolu : Claude Desktop, lancé depuis le Dock, n'a pas le PATH de Homebrew), puis redémarrage.

**Développement** (sans installer l'app) : `cargo build` puis `claude mcp add kairos --scope user -- "$PWD/target/debug/kairos" mcp`.

**Autonomie.** Trois outils sont difficiles à défaire : `delete_entry`, `close_day`, `mark_declared`. Avec `claude_autonomy` à `ask` (défaut), Claude doit demander puis rappeler l'outil avec `confirm: true` ; à `auto`, il agit seul. Seule la ligne de commande (ou l'app) change ce réglage :

```bash
kairos config claude_autonomy auto
```

**À mettre dans le CLAUDE.md** : nouvelle tâche → `switch` avec son ID ; ID inconnu → le demander, ne jamais le deviner ; sous-tâche non précisée → poste principal ; ne jamais arrêter un lot en fin de réponse.

## Construire le binaire embarqué

`npm run build:cli` compile `kairos` pour Apple Silicon, le signe (ad hoc) et le dépose dans `src-tauri/binaries/`, où l'overlay de release (`src-tauri/tauri.release.conf.json`, `externalBin`) l'embarque dans l'app. La CI le fait avant `tauri build` ; `npm run deploy` refuse une release dont l'app ne l'embarque pas, assemble `Kairos_<version>.mcpb` à partir de ce même binaire (`scripts/make-mcpb.sh`) et l'envoie sur R2 avant `latest.json`.

## Architecture

| Dossier                 | Rôle                                                                                                                                              |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/kairos-core`    | Le cœur Rust : toutes les règles métier (étape 1). L'app et la ligne de commande l'utilisent, donc donnent les mêmes refus et les mêmes messages. |
| `crates/kairos-cli`     | Le binaire `kairos` : ligne de commande, hooks de Claude Code, serveur MCP.                                                                       |
| `src-tauri/`            | L'app Tauri v2 : icône de la barre de menus, popover, mises à jour.                                                                               |
| `src/`                  | L'interface Angular 22 (zoneless, signaux, OnPush).                                                                                               |
| `design/tokens.json`    | Les couleurs, exportées des variables Figma. `npm run tokens` en génère `src/styles/_tokens.scss`.                                                |
| `infra/release-worker/` | Le Worker Cloudflare qui sert les releases depuis le bucket R2.                                                                                   |
| `scripts/`              | Release, déploiement, cask Homebrew, vérifications (`scripts/verify`).                                                                            |

## Commandes

```bash
npm run dev           # l'app dans la barre de menus (Tauri + Angular)
npm run build         # build Angular
npm run verify        # vérifications sur le code compilé (thème, tokens, langues, mises à jour, release, bump, Worker)
npm run bump 0.2.0    # monte la version partout + gabarit de changelog FR/EN
npm run format        # Prettier
npm run tokens        # régénère _tokens.scss depuis design/tokens.json
npm run release       # pose le tag de la version courante et le pousse : la CI construit
npm run deploy        # récupère les artefacts de la CI et les publie sur R2
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Mise en place de GitHub (une fois)

Tout vient de Charon ; seuls les noms changent.

1. **Clé de signature des mises à jour** (distincte de celle de Charon) :
   ```bash
   npx tauri signer generate -w ~/.tauri/kairos-updater.key
   ```
   Colle la clé **publique** affichée dans `src-tauri/tauri.conf.json`, à `plugins.updater.pubkey`. Garde le mot de passe dans le trousseau.
2. **Secrets Actions** (Settings, Secrets and variables, Actions) :
   - `TAURI_SIGNING_PRIVATE_KEY` : le contenu de `~/.tauri/kairos-updater.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` : le mot de passe de cette clé
3. **Réglages du dépôt** : Settings, Actions, General, Workflow permissions : _Read and write_. Settings, Code security : activer _Dependabot alerts_ et _Dependabot security updates_.
4. Aucun autre token : pas de certificat Apple (signature ad hoc), pas de PAT. `GH_TOKEN` (scope `contents:read`) n'est utile que si le dépôt passe en privé, et seulement dans `scripts/deploy.env`.

## Publier une version

1. `npm run bump 0.2.0` : monte la version aux trois endroits que la CI compare (`package.json`, `tauri.conf.json`, `src-tauri/Cargo.toml`), aligne les deux crates et `Cargo.lock`, et ajoute une entrée **à compléter** dans `src/assets/changelog.json` (français) et `changelog.en.json` (anglais). Écris les notes ; tant qu'il reste « À COMPLÉTER », la release est refusée.
2. Commit, puis `npm run release`. Il vérifie avant de tagger : sur `main`, arbre propre, pas en retard sur `origin`, versions et changelog cohérents, format, `npm run verify`, clippy et tests. Puis il pose le tag `vX.Y.Z` et le pousse : la CI (`release.yml`) construit l'app signée et l'attache à la release GitHub. `DRY_RUN=1 npm run release` fait toutes les vérifications sans tagger ni pousser.
3. Quand le job est vert : `npm run deploy`. Il récupère les artefacts, génère `latest.json`, met à jour le tap Homebrew et envoie le tout dans le bucket R2.

Une seule fois sur la machine : `cp scripts/deploy.env.example scripts/deploy.env` et `npx wrangler login`.

## Hébergement des releases : Cloudflare R2 + Worker

Les fichiers vivent dans le bucket R2 `kairos-releases`. Un Worker les sert sur `https://release.lucasraffalli.com/kairos/*` (la route est limitée à ce chemin : `/charon/*` n'est pas touché). L'app interroge `https://release.lucasraffalli.com/kairos/latest.json`. Le mode d'emploi complet, pour refaire ou réparer, est dans la page Notion « Héberger les releases sur Cloudflare R2 ».

## Installer (Homebrew)

```bash
brew install --cask lucasraffalli/kairos/kairos
```

Le tap est le dépôt `LucasRaffalli/homebrew-kairos`, alimenté par `npm run deploy`.
