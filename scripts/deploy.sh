#!/usr/bin/env bash
# Déploiement d'une release vers Cloudflare R2, en une commande :
#   récupération des artefacts CI → manifeste latest.json → cask → envoi vers R2.
#
# Rien ne compile ici : macOS (et le binaire kairos embarqué dans l'app) est buildé par la CI (workflow release.yml,
# déclenché par `npm run release`). Ce script tente UNE FOIS de récupérer
# l'archive et le .dmg depuis la release GitHub du tag courant — s'ils ne sont
# pas encore là, il le dit clairement plutôt que de boucler ; vérifier l'onglet
# Actions puis relancer `npm run deploy`.
#
# Les fichiers vont dans le bucket R2 (R2_BUCKET), qu'un Worker sert sur
# release.lucasraffalli.com/kairos/* (voir infra/release-worker). L'envoi passe
# par wrangler : `npx wrangler login` une fois sur cette machine.
#
# Config (une fois) : copie scripts/deploy.env.example vers scripts/deploy.env.
# Si le dépôt devient privé, ajoute aussi GH_TOKEN (scope contents:read) pour que
# le téléchargement des artefacts continue de marcher (GitHub répond 404, pas
# 401, à un accès anonyme refusé).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"

R2_BUCKET="${R2_BUCKET:-kairos-releases}"
[ -f "$HERE/deploy.env" ] && source "$HERE/deploy.env"

: "${PUBLIC_URL:?Renseigne PUBLIC_URL dans scripts/deploy.env (ex. https://release.lucasraffalli.com/kairos)}"

VERSION="$(python3 -c "import json;print(json.load(open('$ROOT/src-tauri/tauri.conf.json'))['version'])")"
TAG="v$VERSION"
CURL_AUTH=()

# dist-macos/ n'est qu'un ENTREPÔT LOCAL de ce que la CI a produit, jamais une
# source de vérité : la vérité est la release GitHub du tag. Le vider avant
# chaque déploiement l'impose mécaniquement (l'archive Kairos.app.tar.gz
# s'appelle pareil d'une release à l'autre : c'est ce qui, chez Charon, a fait
# servir une ancienne version sous l'étiquette de la nouvelle).
rm -rf "$ROOT/dist-macos"
mkdir -p "$ROOT/dist-macos"

[ -n "${GH_TOKEN:-}" ] && CURL_AUTH=(-H "Authorization: Bearer $GH_TOKEN")

# Le JSON complet de la release, UN SEUL appel réutilisé pour tous les fichiers.
# Dans un fichier temporaire (nettoyé en sortie) et non interpolé dans une
# chaîne : le corps peut contenir n'importe quel caractère.
RELEASE_JSON_FILE="$(mktemp)"
export RELEASE_JSON_FILE
trap 'rm -f "$RELEASE_JSON_FILE"' EXIT
curl -fsSL ${CURL_AUTH[@]+"${CURL_AUTH[@]}"} "https://api.github.com/repos/LucasRaffalli/kairos/releases/tags/$TAG" \
  -o "$RELEASE_JSON_FILE" 2>/dev/null || true

# Téléchargement d'UN asset de la release PAR SON NOM : true/false.
#
# Le lien direct (github.com/.../releases/download/...) ne marche PAS sur un
# dépôt privé, même avec un Bearer valide (404) : GitHub veut l'endpoint dédié
# /releases/assets/{id} avec Accept: application/octet-stream. On y passe
# systématiquement, il marche aussi bien en public qu'en privé.
fetch_asset() {
  local name="$1" dest="$2" id
  id="$(NAME="$name" python3 -c "
import json, os, sys
try:
    data = json.load(open(os.environ['RELEASE_JSON_FILE']))
except Exception:
    sys.exit(0)
name = os.environ['NAME']
for a in data.get('assets', []):
    if a['name'] == name:
        print(a['id']); break
" || true)"
  [ -n "$id" ] || return 1
  curl -fsSL ${CURL_AUTH[@]+"${CURL_AUTH[@]}"} -H "Accept: application/octet-stream" -o "$dest" \
    "https://api.github.com/repos/LucasRaffalli/kairos/releases/assets/$id" 2>/dev/null
}

# 1. Archive .app.tar.gz (+ signature), TOUJOURS récupérée depuis la release
# GitHub du tag. Son nom exact dépend de tauri-bundler : on le lit dans les
# assets plutôt que de le deviner.
ASSET_NAME="$(python3 -c "
import json, os, sys
try:
    data = json.load(open(os.environ['RELEASE_JSON_FILE']))
except Exception:
    sys.exit(0)
for a in data.get('assets', []):
    if a['name'].endswith('.app.tar.gz'):
        print(a['name']); break
" || true)"
if [ -n "$ASSET_NAME" ] &&
  fetch_asset "$ASSET_NAME" "$ROOT/dist-macos/$ASSET_NAME" &&
  fetch_asset "$ASSET_NAME.sig" "$ROOT/dist-macos/$ASSET_NAME.sig"; then
  ARCHIVE="$ROOT/dist-macos/$ASSET_NAME"
  echo "Archive macOS récupérée depuis la release GitHub $TAG."
else
  rm -f "$ROOT/dist-macos/$ASSET_NAME" "$ROOT/dist-macos/$ASSET_NAME.sig" 2>/dev/null || true
  echo "Archive macOS introuvable dans la release $TAG." >&2
  echo "Vérifier que le job « macos » de l'onglet Actions est vert, puis relancer npm run deploy." >&2
  exit 1
fi
# Rien dans le nom de l'archive ne dit si son CONTENU correspond au tag : on
# lit la version dans son Info.plist (un run rejoué sur un mauvais commit ou un
# upload manuel resterait invisible sans ce contrôle).
ARCHIVE_VERSION="$(tar -xzOf "$ARCHIVE" "Kairos.app/Contents/Info.plist" 2>/dev/null | plutil -extract CFBundleShortVersionString raw - 2>/dev/null || true)"
if [ "$ARCHIVE_VERSION" != "$VERSION" ]; then
  echo "L'archive macOS de la release $TAG contient la version ${ARCHIVE_VERSION:-inconnue}, pas $VERSION." >&2
  echo "Le job « macos » a probablement tourné sur le mauvais commit — à corriger avant de publier." >&2
  exit 1
fi
# L'app doit embarquer le binaire `kairos` (npm run build:cli avant le build) : c'est lui que
# le cask lie au PATH, et lui que le paquet .mcpb emballe.
if ! tar -tzf "$ARCHIVE" "Kairos.app/Contents/MacOS/kairos" >/dev/null 2>&1; then
  echo "L'archive macOS de la release $TAG n'embarque pas le binaire kairos (Kairos.app/Contents/MacOS/kairos)." >&2
  echo "Le job « macos » a-t-il lancé npm run build:cli avant tauri build ?" >&2
  exit 1
fi
DMG_NAME="Kairos_${VERSION}_aarch64.dmg"
DMG="$ROOT/dist-macos/$DMG_NAME"
if ! fetch_asset "$DMG_NAME" "$DMG"; then
  echo "(pas de .dmg dans la release $TAG — premières installations via l'archive .app.tar.gz)"
  DMG=""
fi

# 1b. L'extension Claude Desktop, assemblée avec le binaire de l'archive ci-dessus (le même
# que celui du cask), puis contrôlée : un binaire d'une autre version que le tag ne part pas.
MCPB_NAME="Kairos_${VERSION}.mcpb"
MCPB="$ROOT/dist-macos/$MCPB_NAME"
"$HERE/make-mcpb.sh" "$ROOT/dist-macos"
"$HERE/make-mcpb.sh" --check "$MCPB" "$VERSION"

# 2. Le manifeste (signature + URL dedans).
"$HERE/make-latest-json.sh" "$PUBLIC_URL" > "$ROOT/latest.json"

# 3. Cask Homebrew (tap séparé : version + sha256 du .dmg), puis commit et push
# automatiques du tap s'il a changé.
"$HERE/make-cask.sh"
TAP_DIR="${TAP_DIR:-$ROOT/../homebrew-kairos}"
# Chemin absolu, sans « .. » : git se lance DEPUIS le tap (cd), pas avec -C.
TAP_DIR="$(cd "$TAP_DIR" 2>/dev/null && pwd || echo "$TAP_DIR")"
if [ -d "$TAP_DIR/.git" ]; then
  # Une erreur ici ne doit JAMAIS passer sous silence : sans cask à jour, brew
  # installerait l'ancienne version (c'est ce qui s'est passé à la 0.1.0 : un
  # git qui échouait dans une substitution était pris pour « rien à pousser »).
  if ! TAP_CHANGES="$(cd "$TAP_DIR" && git status --porcelain)"; then
    echo "⚠ Tap illisible ($TAP_DIR) — cask générée mais pas publiée. À faire à la main :" >&2
    echo "  cd \"$TAP_DIR\" && git add -A && git commit -m \"kairos $VERSION\" && git push" >&2
  elif [ -n "$TAP_CHANGES" ]; then
    if (cd "$TAP_DIR" && git add -A && git commit -q -m "kairos $VERSION" && git push -q); then
      echo "✓ Tap Homebrew poussé (kairos $VERSION)."
    else
      echo "⚠ Publication du tap impossible — à faire à la main : cd \"$TAP_DIR\" && git push" >&2
    fi
  else
    echo "(tap Homebrew déjà à jour)"
  fi
fi

# 4. Envoi vers R2. L'archive part sous un nom VERSIONNÉ (Kairos_<version>.app.tar.gz,
# celui que latest.json annonce) : le Worker met les fichiers en cache une heure.
# latest.json part EN DERNIER : tant qu'il n'est pas à jour, personne ne voit la
# nouvelle version, donc personne ne tombe sur un fichier pas encore là.
put() { # $1 = clé, $2 = fichier, $3 = type MIME
  echo "  → $R2_BUCKET/$1"
  npx --yes wrangler@4 r2 object put "$R2_BUCKET/$1" --file "$2" --content-type "$3" --remote >/dev/null
}
echo "Envoi vers R2 ($R2_BUCKET) …"
put "Kairos_${VERSION}.app.tar.gz" "$ARCHIVE" "application/gzip"
[ -n "$DMG" ] && put "$DMG_NAME" "$DMG" "application/x-apple-diskimage"
put "$MCPB_NAME" "$MCPB" "application/octet-stream"
put "latest.json" "$ROOT/latest.json" "application/json"

# Les anciennes versions restent en place (quelques dizaines de Mo, dans le
# palier gratuit) : le cask et un client qui n'a pas encore vu le nouveau
# latest.json en ont encore besoin.
echo "✓ Déployé : $PUBLIC_URL/latest.json"
echo "✓ Extension Claude Desktop : $PUBLIC_URL/$MCPB_NAME"
if [ -n "$DMG" ]; then
  echo "✓ Installeur : $PUBLIC_URL/$DMG_NAME"
else
  echo "(pas de .dmg produit — premières installations via l'archive .app.tar.gz)"
fi
