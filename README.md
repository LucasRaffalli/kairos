# Kairos

Chronomètre de barre de menus pour macOS. Kairos mesure le temps par **tâche parent** et **sous-tâche** Monday, se pilote à la main (icône, raccourcis) ou par Claude, et prépare chaque soir la liste exacte à saisir dans Monday. Il n'écrit jamais dans Monday : la saisie reste manuelle.

> **Saisir le moment.** Kairos est le dieu grec du moment opportun.

La conception complète (pourquoi, concept, utilisation, cahier des charges, technique, design) est dans la page Notion « Kairos ». Les maquettes sont dans le fichier Figma [Kairos](https://www.figma.com/design/st5ct8bxzDkzwnY1UEH1YS).

**État : étape 0 (le socle).** Une icône dans la barre de menus et un popover vide aux couleurs de l'app ; le chronomètre arrive aux étapes suivantes. L'icône de l'app est **provisoire** (un aplat Iris et le glyphe blanc).

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
npm run verify        # vérifications sur le code compilé (thème, tokens, langues, mises à jour, release, Worker)
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
   Colle la clé **publique** affichée dans `src-tauri/tauri.conf.json`, à `plugins.updater.pubkey` (aujourd'hui un texte provisoire). Garde le mot de passe dans le trousseau.
2. **Secrets Actions** (Settings, Secrets and variables, Actions) :
   - `TAURI_SIGNING_PRIVATE_KEY` : le contenu de `~/.tauri/kairos-updater.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` : le mot de passe de cette clé
3. **Réglages du dépôt** : Settings, Actions, General, Workflow permissions : _Read and write_. Settings, Code security : activer _Dependabot alerts_ et _Dependabot security updates_.
4. Aucun autre token : pas de certificat Apple (signature ad hoc), pas de PAT. `GH_TOKEN` (scope `contents:read`) n'est utile que si le dépôt passe en privé, et seulement dans `scripts/deploy.env`.

## Publier une version

1. Monter la version aux **trois** endroits (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`) et ajouter son entrée dans `src/assets/changelog.json` **et** sa traduction dans `changelog.en.json` (même nombre de notes). `node scripts/check-release.cjs` vérifie tout cela.
2. `npm run release` : pose le tag `vX.Y.Z` et le pousse. La CI (`release.yml`) construit l'app signée et l'attache à la release GitHub.
3. Quand le job est vert : `npm run deploy`. Il récupère les artefacts, génère `latest.json`, met à jour le tap Homebrew et envoie le tout dans le bucket R2.

Une seule fois sur la machine : `cp scripts/deploy.env.example scripts/deploy.env` et `npx wrangler login`.

## Hébergement des releases : Cloudflare R2 + Worker

Les fichiers vivent dans le bucket R2 `kairos-releases`. Un Worker les sert sur `https://release.lucasraffalli.com/kairos/*` (la route est limitée à ce chemin : `/charon/*` n'est pas touché). L'app interroge `https://release.lucasraffalli.com/kairos/latest.json`. Le mode d'emploi complet, pour refaire ou réparer, est dans la page Notion « Héberger les releases sur Cloudflare R2 ».

## Installer (Homebrew)

```bash
brew install --cask lucasraffalli/kairos/kairos
```

Le tap est le dépôt `LucasRaffalli/homebrew-kairos`, alimenté par `npm run deploy`.
