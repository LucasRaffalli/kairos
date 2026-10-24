#!/bin/sh
# Lance le serveur MCP de Kairos pour Claude Desktop.
#
# Le paquet .mcpb est téléchargé : macOS peut lui coller l'attribut de quarantaine, et un
# binaire non notarisé qui le porte est tué au lancement (code 137). Ce script (un script
# n'est pas soumis à Gatekeeper) lève l'attribut sur notre propre binaire puis le lance,
# comme le fait déjà le cask Homebrew (xattr -cr).
here="$(cd "$(dirname "$0")" && pwd)"
xattr -d com.apple.quarantine "$here/kairos" 2>/dev/null || true
exec "$here/kairos" mcp
