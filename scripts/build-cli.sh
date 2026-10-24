#!/usr/bin/env bash
# Compile le binaire `kairos` (ligne de commande et serveur MCP) et le dépose là où
# Tauri l'attend pour l'embarquer dans l'app (`externalBin`, voir
# src-tauri/tauri.release.conf.json) : src-tauri/binaries/kairos-<triple>.
#
# Signature AD HOC, comme l'app : aucun certificat Apple Developer. Le même binaire sert
# ensuite au paquet .mcpb (scripts/make-mcpb.sh l'extrait de l'archive de l'app).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="aarch64-apple-darwin"

if [ "$(uname -s)-$(uname -m)" != "Darwin-arm64" ]; then
  echo "build-cli : seul un Mac Apple Silicon livre le binaire pour l'instant ($TARGET)." >&2
  exit 1
fi

cd "$ROOT"
cargo build --release -p kairos-cli --target "$TARGET"

mkdir -p src-tauri/binaries
cp "target/$TARGET/release/kairos" "src-tauri/binaries/kairos-$TARGET"
chmod +x "src-tauri/binaries/kairos-$TARGET"
codesign --force -s - "src-tauri/binaries/kairos-$TARGET"
echo "✓ src-tauri/binaries/kairos-$TARGET"
