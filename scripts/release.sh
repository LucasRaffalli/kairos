#!/usr/bin/env bash
# Déclenche les builds de release : rien ne compile en local, tout part sur
# GitHub Actions (workflow release.yml, runners macOS + Windows en parallèle).
#
# Ce script se contente de poser le tag de la version courante et de le
# pousser — c'est le push du tag qui démarre la CI. Une fois les deux jobs
# verts (onglet Actions), lance `npm run deploy` pour publier.
set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(python3 -c "import json;print(json.load(open('$DIR/src-tauri/tauri.conf.json'))['version'])")"
TAG="v$VERSION"

# RETAG=1 : cas du rebuild sur la MÊME version (par ex. un déploiement relancé
# sans avoir bumpé le numéro). Un tag qui existe déjà ne bouge jamais tout
# seul — le push d'un tag inchangé ne déclenche rien côté CI — donc sans ce
# drapeau le rebuild part pour rien : la CI reste figée sur l'ancien commit.
# RETAG=1 supprime le tag local ET distant puis le recrée sur HEAD, en
# forçant le push.
TAG_EXISTS="$(git -C "$DIR" rev-parse "$TAG" >/dev/null 2>&1 && echo 1 || echo 0)"
if [ "$TAG_EXISTS" = "1" ] && [ "${RETAG:-0}" = "1" ]; then
  git -C "$DIR" tag -d "$TAG" >/dev/null 2>&1 || true
  git -C "$DIR" push origin ":refs/tags/$TAG" >/dev/null 2>&1 || true
  echo "Tag $TAG retiré (local + distant), recréé sur ce commit."
  TAG_EXISTS=0
fi
if [ "$TAG_EXISTS" = "1" ]; then
  echo "Tag $TAG déjà présent (rebuild de la même version) — RETAG=1 pour le repousser."
else
  git -C "$DIR" tag "$TAG"
  echo "Tag $TAG créé."
fi

if git -C "$DIR" push origin HEAD "$TAG"; then
  echo "Poussé : branche courante + $TAG → la CI démarre (macOS + Windows, ~20 min)."
  echo "Suivre : https://github.com/LucasRaffalli/kairos/actions"
  echo "Une fois les deux jobs verts : npm run deploy"
else
  echo "⚠ Push impossible (hors-ligne ? remote ?) — à faire à la main : git push origin HEAD $TAG" >&2
fi
