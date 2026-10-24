#!/usr/bin/env bash
# Génère la cask Homebrew du tap après le build du .dmg :
# version depuis tauri.conf.json, sha256 du .dmg, URL publique depuis deploy.env.
#
# Le tap est un dépôt git SÉPARÉ (github.com/LucasRaffalli/homebrew-kairos),
# cloné à côté du projet (../homebrew-kairos par défaut, TAP_DIR dans
# deploy.env pour changer). Après génération : commit + push du tap.
#
# Installation utilisateur : brew install --cask lucasraffalli/kairos/kairos
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
[ -f "$HERE/deploy.env" ] && source "$HERE/deploy.env"

: "${PUBLIC_URL:?Renseigne PUBLIC_URL dans scripts/deploy.env}"
TAP_DIR="${TAP_DIR:-$ROOT/../homebrew-kairos}"

if [ ! -d "$TAP_DIR" ]; then
  echo "Tap introuvable ($TAP_DIR) : cask non générée." >&2
  echo "Clone-le : git clone https://github.com/LucasRaffalli/homebrew-kairos \"$TAP_DIR\"" >&2
  exit 0
fi

# dist-macos/ d'abord (là que deploy.sh dépose le .dmg récupéré sur la CI) ;
# à défaut le dossier de build local. `|| true` : voir make-latest-json.sh.
DMG="$(ls "$ROOT/dist-macos"/*.dmg 2>/dev/null | head -1 || true)"
[ -n "$DMG" ] || DMG="$(ls "$ROOT/src-tauri/target/release/bundle/dmg"/*.dmg 2>/dev/null | head -1 || true)"
[ -n "$DMG" ] || { echo "Aucun .dmg dans dist-macos/ ni dans le dossier de build local : lancer deploy.sh d'abord." >&2; exit 1; }

VERSION="$(python3 -c "import json;print(json.load(open('$ROOT/src-tauri/tauri.conf.json'))['version'])")"
SHA256="$(shasum -a 256 "$DMG" | awk '{print $1}')"

# NB : #{version} est de l'interpolation Ruby, résolue par Homebrew : seuls
# ${VERSION}/${SHA256}/${PUBLIC_URL} sont substitués ici.
mkdir -p "$TAP_DIR/Casks"
cat > "$TAP_DIR/Casks/kairos.rb" <<RUBY
cask "kairos" do
  version "${VERSION}"
  sha256 "${SHA256}"

  url "${PUBLIC_URL}/Kairos_#{version}_aarch64.dmg"
  name "Kairos"
  desc "Chronomètre de barre de menus pour Monday"
  homepage "https://github.com/LucasRaffalli/kairos"

  # L'app embarque son propre updater : brew upgrade n'a rien à faire.
  auto_updates true
  depends_on arch: :arm64

  app "Kairos.app"
  # La commande kairos (CLI et serveur MCP, lancé par « kairos mcp ») vit dans l'app : le lien suit
  # donc ses mises à jour.
  binary "#{appdir}/Kairos.app/Contents/MacOS/kairos"

  # App non notarisée : sans ça, Gatekeeper affiche « Kairos est endommagé ».
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-cr", "#{appdir}/Kairos.app"]
  end

  uninstall quit: "com.aegis.kairos"

  zap trash: [
    "~/Library/Application Support/com.aegis.kairos",
    "~/Library/Caches/com.aegis.kairos",
    "~/Library/Preferences/com.aegis.kairos.plist",
    "~/Library/Saved Application State/com.aegis.kairos.savedState",
    "~/Library/WebKit/com.aegis.kairos",
  ]
end
RUBY

echo "Cask générée : $TAP_DIR/Casks/kairos.rb (v$VERSION, sha256 ${SHA256:0:12}…)"
echo "→ à publier : cd \"$TAP_DIR\" && git add -A && git commit -m \"kairos $VERSION\" && git push"
