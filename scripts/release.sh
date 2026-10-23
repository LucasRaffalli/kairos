#!/usr/bin/env bash
# Déclenche le build de release : rien ne compile en local, tout part sur
# GitHub Actions (workflow release.yml, runner macOS).
#
# Ce script vérifie d'abord que la release tiendra (branche, arbre propre,
# à jour avec origin, cohérence des versions et du changelog, format, tests),
# puis pose le tag de la version courante et le pousse — c'est le push du tag
# qui démarre la CI. Une erreur se voit donc en quelques secondes, pas après
# 20 minutes de build. Une fois le job vert (onglet Actions), lance
# `npm run deploy` pour publier.
#
# Variables : DRY_RUN=1 (tout vérifier sans tagger ni pousser), SKIP_CHECKS=1
# (sauter les vérifications longues : format, verify, clippy, tests),
# ALLOW_BRANCH=1 (releaser depuis une autre branche que main), RETAG=1 (voir
# plus bas).
set -euo pipefail

DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(python3 -c "import json;print(json.load(open('$DIR/src-tauri/tauri.conf.json'))['version'])")"
TAG="v$VERSION"

fail() {
  echo "✗ $1" >&2
  exit 1
}

# --- Vérifications avant de tagger -------------------------------------------
BRANCH="$(git -C "$DIR" rev-parse --abbrev-ref HEAD)"
if [ "$BRANCH" != "main" ] && [ "${ALLOW_BRANCH:-0}" != "1" ]; then
  fail "Tu es sur « $BRANCH », pas sur main (ALLOW_BRANCH=1 pour releaser d'ici quand même)."
fi
if [ -n "$(git -C "$DIR" status --porcelain)" ]; then
  fail "Des changements ne sont pas commités : commit (ou stash) avant de releaser."
fi
if git -C "$DIR" fetch --quiet origin "$BRANCH" 2>/dev/null; then
  BEHIND="$(git -C "$DIR" rev-list --count "HEAD..origin/$BRANCH" 2>/dev/null || echo 0)"
  [ "$BEHIND" = "0" ] || fail "origin/$BRANCH a $BEHIND commit(s) de plus que toi : git pull avant de releaser."
else
  echo "(origin injoignable : retard sur origin non vérifié)"
fi

echo "Vérification de la cohérence (versions, changelog)…"
node "$DIR/scripts/check-release.cjs" || fail "Release incohérente : voir ci-dessus (npm run bump aide à tout aligner)."

if [ "${SKIP_CHECKS:-0}" != "1" ]; then
  echo "Format, vérifications et tests (SKIP_CHECKS=1 pour les sauter)…"
  (cd "$DIR" && npm run --silent format:check >/dev/null) || fail "Format incorrect : npm run format"
  (cd "$DIR" && npm run --silent verify >/dev/null) || fail "npm run verify échoue : relance-le pour voir lequel."
  (cd "$DIR" && cargo fmt --all --check) || fail "Format Rust incorrect : cargo fmt --all"
  (cd "$DIR" && cargo clippy --workspace --all-targets --quiet -- -D warnings) || fail "clippy échoue."
  (cd "$DIR" && cargo test --workspace --quiet >/dev/null) || fail "cargo test échoue : relance-le pour voir lequel."
fi
echo "✓ Prêt à releaser $TAG depuis $BRANCH."
if [ "${DRY_RUN:-0}" = "1" ]; then
  echo "DRY_RUN=1 : rien n'est tagué ni poussé."
  exit 0
fi

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
  echo "Poussé : branche courante + $TAG → la CI démarre (macOS, 10 à 20 min)."
  echo "Suivre : https://github.com/LucasRaffalli/kairos/actions"
  echo "Une fois le job « macos » vert : npm run deploy"
else
  echo "⚠ Push impossible (hors-ligne ? remote ?) — à faire à la main : git push origin HEAD $TAG" >&2
fi
