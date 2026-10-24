#!/usr/bin/env bash
# Assemble l'extension Claude Desktop : Kairos_<version>.mcpb (un zip : manifeste, icône et
# le binaire `kairos`). Le binaire est celui de l'app de la release : extrait de l'archive
# Kairos.app.tar.gz de dist-macos/, jamais recompilé, donc identique à celui du cask.
#
#   make-mcpb.sh [dossier-de-sortie]            assemble (défaut : dist-macos/)
#   make-mcpb.sh --check <fichier.mcpb> <version>   vérifie que le binaire annonce cette version
#
# Appelé par deploy.sh. Le format du manifeste : https://github.com/anthropics/mcpb
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY_IN_APP="Kairos.app/Contents/MacOS/kairos"

if [ "${1:-}" = "--check" ]; then
  PKG="${2:?usage: make-mcpb.sh --check <fichier.mcpb> <version>}"
  EXPECTED="${3:?usage: make-mcpb.sh --check <fichier.mcpb> <version>}"
  TMP="$(mktemp -d)"
  trap 'rm -rf "$TMP"' EXIT
  if ! unzip -p "$PKG" server/kairos > "$TMP/kairos" 2>/dev/null; then
    echo "$PKG ne contient pas server/kairos." >&2
    exit 1
  fi
  chmod +x "$TMP/kairos"
  ACTUAL="$("$TMP/kairos" version 2>/dev/null | awk '{print $2}' || true)"
  if [ "$ACTUAL" != "$EXPECTED" ]; then
    echo "Le binaire de $(basename "$PKG") annonce la version ${ACTUAL:-inconnue}, pas $EXPECTED." >&2
    exit 1
  fi
  echo "✓ $(basename "$PKG") : kairos $ACTUAL"
  exit 0
fi

OUT="${1:-$ROOT/dist-macos}"
VERSION="$(python3 -c "import json;print(json.load(open('$ROOT/src-tauri/tauri.conf.json'))['version'])")"

# `|| true` : sous set -e + pipefail, un glob vide ferait sortir `ls` en silence.
ARCHIVE="$(ls "$ROOT"/dist-macos/*.app.tar.gz 2>/dev/null | head -1 || true)"
if [ -z "$ARCHIVE" ]; then
  echo "Aucune archive .app.tar.gz dans dist-macos/ : lancer deploy.sh (ou la récupérer depuis la release GitHub)." >&2
  exit 1
fi

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/pkg/server"

if ! tar -xzOf "$ARCHIVE" "$BINARY_IN_APP" > "$STAGE/pkg/server/kairos" 2>/dev/null; then
  echo "$(basename "$ARCHIVE") ne contient pas $BINARY_IN_APP : l'app de la release n'embarque pas le binaire kairos (npm run build:cli avant le build)." >&2
  exit 1
fi
chmod +x "$STAGE/pkg/server/kairos"
# Lance le binaire en levant d'abord la quarantaine (voir packaging/mcpb/start.sh).
cp "$ROOT/packaging/mcpb/start.sh" "$STAGE/pkg/server/start.sh"
chmod +x "$STAGE/pkg/server/start.sh"

sed "s/__VERSION__/$VERSION/" "$ROOT/packaging/mcpb/manifest.json" > "$STAGE/pkg/manifest.json"
cp "$ROOT/src-tauri/icons/128x128.png" "$STAGE/pkg/icon.png"

mkdir -p "$OUT"
PKG="$OUT/Kairos_${VERSION}.mcpb"
rm -f "$PKG"
(cd "$STAGE/pkg" && zip -X -q -r "$PKG" manifest.json icon.png server)
echo "✓ $PKG"
