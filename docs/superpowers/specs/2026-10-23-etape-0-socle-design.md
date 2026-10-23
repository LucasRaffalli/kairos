# Kairos, étape 0 : le socle du dépôt

Date : 23 octobre 2026. Source de vérité du produit : la page Notion « Kairos » (cahier des charges, concept, technique, design). Ce document ne décrit que l'étape 0.

## But

Un dépôt qui compile, se teste, se met en forme, se publie et se met à jour comme Charon, avant la moindre règle métier. Fin de l'étape 0 :

- `npm run dev` ouvre l'app : une icône dans la barre de menus, et un popover vide aux couleurs de Kairos (Iris, thèmes, accents).
- `cargo test` et `npm run verify` passent ; la CI est verte sur `main`.
- `npm run release` pose un tag, la CI construit l'app signée, `npm run deploy` publie `latest.json` et le `.dmg`, `brew install --cask` installe, et l'app sait se mettre à jour toute seule.

Hors étape 0 : SQLite, règles de lots, outils MCP, écrans réels (étapes 1 à 3).

## Principe : reprendre Charon, ne rien réinventer

Charon (`~/Documents/charon`) est la référence : mêmes outils, mêmes scripts, mêmes conventions. On copie, on renomme, on retire ce qui est propre à Charon (SFTP, terminal, modules, Cadran, Clarity). On n'invente un mécanisme que là où Kairos diffère : un workspace Cargo à plusieurs crates et une app de barre de menus.

## Arborescence

```
kairos/
├─ Cargo.toml                  workspace : src-tauri, crates/*
├─ crates/
│  ├─ kairos-core/             règles + SQLite (étape 1) ; ici : version() et un test
│  └─ kairos-cli/              binaire `kairos` (clap) ; ici : `kairos version`
├─ src-tauri/                  app Tauri v2 (crate kairos-app), comme Charon
│  ├─ src/                     lib.rs, main.rs, tray.rs (icône et popover)
│  ├─ capabilities/, icons/, tauri.conf.json, tauri.dev.conf.json, tauri.release.conf.json
├─ src/                        Angular 22, zoneless, signaux, OnPush (même squelette que Charon)
│  ├─ app/{components,features,interfaces,lang,pipes,services}
│  ├─ assets/changelog.json, changelog.en.json
│  └─ styles/                  tokens SCSS générés depuis les variables Figma
├─ infra/release-worker/       Worker Cloudflare qui sert le bucket R2 (index.js, wrangler.jsonc)
├─ scripts/                    release.sh, deploy.sh, make-cask.sh, make-latest-json.sh,
│                              check-release.cjs, make-icons.sh, lang-todo.cjs, verify/
├─ docs/                       ce spec, docs/releases/ (notes de version), design-system
├─ .github/                    workflows ci, audit, release ; dependabot.yml ; ISSUE_TEMPLATE
├─ package.json, angular.json, tsconfig*.json, .prettierrc.json, .gitignore
└─ README.md, CHANGELOG (via src/assets/changelog.json comme Charon)
```

Le crate de l'app reste dans `src-tauri/` (et non `crates/kairos-app`) pour que les scripts de Charon, qui lisent `src-tauri/tauri.conf.json`, fonctionnent sans réécriture. Le crate `kairos-app` dépendra de `kairos-core` plus tard.

## Ce qu'on copie de Charon, et ce qu'on adapte

| Élément                                | Source dans Charon                                                | Adaptation                                                                              |
| -------------------------------------- | ----------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| CI (`ci.yml`)                          | `.github/workflows/ci.yml`                                        | jobs `angular`, `rust` (macOS seul pour l'instant), cohérence de release                |
| Audit                                  | `audit.yml`                                                       | chemins du workspace                                                                    |
| Release                                | `release.yml`                                                     | job macOS seul ; job Windows gardé en commentaire (plus tard) ; plus de `CLARITY_KEY`   |
| Dependabot                             | `dependabot.yml`                                                  | répertoires `/` (cargo workspace) et `/` (npm), mêmes groupes                           |
| Release, deploy, cask, latest.json     | `scripts/*.sh`                                                    | nom, identifiant, URL, tap `homebrew-kairos`                                            |
| Cohérence des versions et du changelog | `check-release.cjs`                                               | fichier changelog identique (FR + EN apparié)                                           |
| Mises à jour dans l'app                | `updater.service.ts`, `whats-new.service.ts`, `changelog-i18n.ts` | plugin updater, page « Nouveautés »                                                     |
| Apparence                              | `theme.service.ts`, `secret-accent.service.ts`, `accent.ts`       | accents Kairos, Charon, Jade, Unloved, Unicorn ; Stars, Blåhaj, Rainbow laissés de côté |
| Squelette Angular                      | `app.config.ts`, `styles/`, `lang/`, `pipes/`                     | CSP stricte, i18n FR/EN, zoneless                                                       |
| Qualité                                | `prettier`, `clippy -D warnings`, profil release (LTO, strip)     | inchangé                                                                                |
| Modèle de tickets                      | `.github/ISSUE_TEMPLATE/`                                         | textes Kairos                                                                           |

Retiré d'office : SFTP/FTP/SSH, terminal, modules, Cadran, Dock, rapport Clarity, thèmes personnalisés avancés.

## L'app de l'étape 0

- Tauri v2, activation `ActivationPolicy::Accessory` (pas d'icône dans le Dock), une fenêtre unique `popover` sans décoration, masquée au départ.
- `tray.rs` : icône de barre de menus (glyphe Kairos, gabarit modèle monochrome), clic gauche = montre ou cache le popover sous l'icône, perte de focus = masque, clic droit = menu court (Réglages, Quitter).
- Le popover Angular affiche un écran vide : l'en-tête Kairos (glyphe, titre, date) sur le fond de popover. C'est lui qui valide la chaîne tokens, thème et accent.
- CSP identique à Charon (`default-src 'self'`, `unsafe-eval` seulement en dev).
- Identifiant : `com.aegis.kairos`.

## Tokens et thèmes

Les variables Figma (collections Primitives, Accent, Color) sont exportées une fois en `src/styles/_tokens.scss` à la main (script d'export plus tard si utile) : variables CSS posées par `[data-theme]` et `[data-accent]` sur la racine, comme Charon. `ThemeService` (signaux) choisit le thème réellement peint (jamais « auto » dans l'attribut). Le focus d'un champ suit le motif Charon : contour 1 px + ombre pleine de 3 px à 26 %.

## Release et mises à jour (identique à Charon)

1. `npm run release` : pose le tag `vX.Y.Z` lu dans `tauri.conf.json` et le pousse.
2. `release.yml` : `preflight` (`check-release.cjs`) puis build macOS (`tauri build` avec l'overlay `tauri.release.conf.json`, signature ad hoc `signingIdentity "-"`), artefacts publiés sur la release GitHub du tag.
3. `npm run deploy` : récupère les artefacts de la release, vérifie que la version de l'archive est la bonne, génère `latest.json`, envoie le tout dans le bucket **Cloudflare R2** `kairos-releases` (`wrangler r2 object put`), génère la cask. Un **Worker** (`infra/release-worker/`) sert le bucket sur `release.lucasraffalli.com/kairos/*` ; la route est limitée à ce chemin, donc `/charon/*` continue d'aller au VPS. Pas de VPS pour Kairos.
4. L'app interroge `https://release.lucasraffalli.com/kairos/latest.json` (plugin updater, clé publique dans `tauri.conf.json`).
5. Le changelog vit dans `src/assets/changelog.json` (FR) et `changelog.en.json` (apparié note pour note) ; `check-release.cjs` bloque une release sans entrée ou sans traduction.

## À mettre en place côté GitHub et machine (à faire une fois)

Dépôt : `https://github.com/LucasRaffalli/kairos`.

**Secrets Actions** (Settings, Secrets and variables, Actions, onglet _Secrets_) :

| Secret                               | Valeur                                   | Obligatoire |
| ------------------------------------ | ---------------------------------------- | ----------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | contenu de `~/.tauri/kairos-updater.key` | oui         |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | mot de passe de cette clé                | oui         |

La clé se génère une fois : `npx tauri signer generate -w ~/.tauri/kairos-updater.key` ; sa partie **publique** va dans `plugins.updater.pubkey` de `tauri.conf.json`. Une clé **distincte** de celle de Charon (une clé compromise ne doit pas lier les deux apps). Sauvegarder le mot de passe dans le trousseau.

**Réglages du dépôt :**

- Settings, Actions, General, Workflow permissions : _Read and write_ (le workflow déclare déjà `contents: write` pour publier la release).
- Settings, Code security : activer _Dependabot alerts_ et _Dependabot security updates_ (non déclarables dans un fichier).
- Branche `main` : pas de règle de protection nécessaire au début.

**Pas de token GitHub** à créer : pas de certificat Apple (signature ad hoc), pas de `CLARITY_KEY`, aucun PAT. Un `GH_TOKEN` (scope `contents:read`) n'est utile que si le dépôt passe en privé, et seulement dans `scripts/deploy.env` en local, jamais comme secret.

**Cloudflare (compte « Lucas Raffalli », zone `lucasraffalli.com`) :** bucket R2 `kairos-releases` (sans accès public direct), Worker `kairos-releases` lié au bucket, route `release.lucasraffalli.com/kairos/*`. Créés une fois (par Claude via le connecteur Cloudflare), reproductibles avec `infra/release-worker/wrangler.jsonc`. Palier gratuit de R2 et des Workers : aucun frais attendu. Le mode d'emploi est dans la page Notion « Héberger les releases sur Cloudflare R2 ».

**Local, non versionné :** `scripts/deploy.env` (copié de `deploy.env.example`) : `PUBLIC_URL=https://release.lucasraffalli.com/kairos`, `R2_BUCKET=kairos-releases`, `TAP_DIR` (clone du tap). Un `npx wrangler login` une fois sur la machine, aucun secret GitHub pour R2.

**Dépôt Homebrew :** créer `LucasRaffalli/homebrew-kairos` (vide) ; `make-cask.sh` y écrit `Casks/kairos.rb`. Installation : `brew install --cask lucasraffalli/kairos/kairos`.

## Qualité de l'étape 0

- `cargo clippy --workspace -D warnings`, `cargo test --workspace`.
- `npm run verify` : au minimum un script qui vérifie les tokens (chaque variable utilisée existe dans chaque thème) et un pour la cohérence de release.
- Prettier : `npm run format:check`.
- La CI exécute tout cela sur chaque push et chaque PR.

## Git

`git init` sur `main`, remote `origin` vers le dépôt, un commit par bloc cohérent (workspace Rust, Angular, Tauri, scripts et CI, README). Aucun push sans demande explicite.

## Ordre de travail

1. Workspace Cargo et deux crates (core, cli) avec leurs tests.
2. Projet Angular (config, styles, tokens, thème, popover vide), `npm run build`.
3. Coquille Tauri : icône, popover, config dev et release, `npm run dev`.
4. Scripts de release, de déploiement, cask, `check-release.cjs`, changelog 0.1.0.
5. Workflows CI, audit, release, Dependabot, modèles de tickets.
6. README, vérification complète (clippy, tests, verify, format, build).
7. Mise à jour de la page Notion Technique et Distribution si la structure diffère (dossier `src/` au lieu de `ui/`).

## Décisions ouvertes (à confirmer à la relecture)

- Identifiant `com.aegis.kairos` et endpoint `release.lucasraffalli.com/kairos` : à confirmer.
- Windows : volontairement absent de la CI de l'étape 0.
- Version de départ : `0.1.0`.
