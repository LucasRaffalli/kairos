#!/usr/bin/env bash
# Génère le latest.json de l'updater. Appelé par deploy.sh une fois l'archive de
# la CI récupérée dans dist-macos/ ; fonctionne aussi après un `npx tauri build
# --config src-tauri/tauri.release.conf.json` fait à la main (repli sur le
# dossier de build local).
#
# Usage : scripts/make-latest-json.sh https://release.lucasraffalli.com/kairos > latest.json
# (l'URL de base doit correspondre à l'endpoint de tauri.conf.json).
#
# L'archive est publiée sous un nom VERSIONNÉ (Kairos_<version>.app.tar.gz) :
# le Worker met les fichiers en cache une heure, et une archive au nom fixe
# serait servie à l'ancienne version avec la signature de la nouvelle.
set -euo pipefail

BASE_URL="${1:?usage: make-latest-json.sh <url-de-base>}"
DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE="$DIR/dist-macos"
# `|| true` : sous set -e + pipefail, un glob qui ne matche rien fait sortir
# `ls` en erreur et tue le script SANS message.
ARCHIVE="$(ls "$BUNDLE"/*.app.tar.gz 2>/dev/null | head -1 || true)"
if [ -z "$ARCHIVE" ]; then
  BUNDLE="$DIR/src-tauri/target/release/bundle/macos"
  ARCHIVE="$(ls "$BUNDLE"/*.app.tar.gz 2>/dev/null | head -1 || true)"
fi
if [ -z "$ARCHIVE" ]; then
  echo "Aucune archive .app.tar.gz dans dist-macos/ ni dans le dossier de build local." >&2
  exit 1
fi
if [ ! -f "$ARCHIVE.sig" ]; then
  echo "Signature absente ($ARCHIVE.sig) — le build doit être signé (TAURI_SIGNING_PRIVATE_KEY)." >&2
  exit 1
fi

VERSION="$(python3 -c "import json;print(json.load(open('$DIR/src-tauri/tauri.conf.json'))['version'])")"
SIGNATURE="$(cat "$ARCHIVE.sig")"
NAME="Kairos_${VERSION}.app.tar.gz"

# Notes de version : depuis le changelog CURATÉ (src/assets/changelog.json),
# jamais depuis les messages de commit, qui peuvent contenir des détails internes.
NOTES="$(CHANGELOG="$DIR/src/assets/changelog.json" VERSION="$VERSION" python3 - <<'PYEOF'
import json
import os

entries = json.load(open(os.environ["CHANGELOG"], encoding="utf-8"))
version = os.environ["VERSION"]
entry = next((e for e in entries if e["version"] == version), None)
if entry:
    print("\n".join(f"- {n['text']}" for n in entry["notes"]))
PYEOF
)"
if [ -z "$NOTES" ]; then
  echo "Attention : aucune entrée '$VERSION' dans src/assets/changelog.json — notes vides." >&2
fi

NOTES="$NOTES" python3 - "$VERSION" "$SIGNATURE" "$BASE_URL/$NAME" <<'EOF'
import datetime
import json
import os
import sys

version, signature, url = sys.argv[1:4]
platform = {"signature": signature, "url": url}
print(
    json.dumps(
        {
            "version": version,
            "notes": os.environ.get("NOTES", ""),
            "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "platforms": {"darwin-aarch64": platform, "darwin-x86_64": platform},
        },
        indent=2,
    )
)
EOF
