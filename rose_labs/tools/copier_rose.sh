#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════
# copier_rose.sh — Fait une copie de travail de rose/ vers copies/rose_ref/
# pour expérimenter sans toucher au dépôt principal.
# ═══════════════════════════════════════════════════════════════════════════
set -euo pipefail

RACINE="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$RACINE/../rose"
DST="$RACINE/copies/rose_ref"

if [ ! -d "$SRC" ]; then
  echo "✗ Source introuvable : $SRC"
  exit 1
fi

echo "→ Copie de $SRC vers $DST"
mkdir -p "$DST"

# Copie du code source (pas le binaire, pas le target/)
rsync -a --delete \
  --exclude='target/' \
  --exclude='*.exe' \
  --exclude='Cargo.lock' \
  "$SRC/" "$DST/"

echo "✓ Copie terminée dans $DST"
echo "  Éditez $DST pour expérimenter (niveaux -O1/-O2/-O3, PGO, etc.)"
