#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
#  Construit l'installeur Windows « Rose-Setup-<version>.exe » (NSIS 3).
#
#  Prérequis sur la machine de build (Windows x86_64 recommandé) :
#    - makensis (NSIS 3) dans le PATH
#    - Le binaire Windows « rose-20.exe » déjà compilé (cargo build
#      --release --target x86_64-pc-windows-msvc ou gnu), placé dans
#      rose/target/release/rose-20.exe
#  Sortie : installateur/dist/Rose-Setup-<version>.exe (+ SHA256SUMS.txt)
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

RACINE="$(cd "$(dirname "$0")/.." && pwd)"
I="$RACINE/installateur"

# ── configuration ──────────────────────────────────────────────────────
VERSION="${VERSION:-5.0.0}"
APP_NAME="Rose"
EXE_SRC="$RACINE/rose/target/release/rose-20.exe"   # binaire Windows (croisé)
MAKENSIS="${MAKENSIS:-makensis}"
DIST="$I/dist"

# ── préparation du staging ─────────────────────────────────────────────
STAGE="$(mktemp -d /tmp/opencode/rose-stage.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$STAGE/bin"
if [ -f "$EXE_SRC" ]; then
  cp "$EXE_SRC" "$STAGE/bin/rose-20.exe"
  echo "✓ binaire Windows trouvé : $EXE_SRC"
else
  echo "⚠ ATTENTION: $EXE_SRC introuvable — placeholder (l'installeur ne fonctionnera pas sous Windows)"
  touch "$STAGE/bin/rose-20.exe"
fi

# LLVM-C.dll (AOT/LLVM runtime) si disponible
LLVM_DLL="${LLVM_DLL:-/home/clausake/.local/llvm-win/bin/LLVM-C.dll}"
if [ -f "$LLVM_DLL" ]; then
  cp "$LLVM_DLL" "$STAGE/bin/LLVM-C.dll"
  echo "✓ LLVM-C.dll trouvé : $LLVM_DLL"
else
  echo "⚠ LLVM-C.dll introuvable — AOT/LLVM indisponible dans l'installeur"
fi

mkdir -p "$I/staging"
cp "$STAGE/bin/rose-20.exe" "$I/staging/rose-20.exe"
if [ -f "$STAGE/bin/LLVM-C.dll" ]; then
  cp "$STAGE/bin/LLVM-C.dll" "$I/staging/LLVM-C.dll"
fi

# fichiers docs optionnels
for f in README-PARTAGE.md TESTER.md; do
  if [ -f "$RACINE/$f" ]; then cp "$RACINE/$f" "$I/staging/"; fi
done

# ── build NSIS ─────────────────────────────────────────────────────────
mkdir -p "$DIST"
( cd "$I" && "$MAKENSIS" -V2 -DAPP_VERSION="$VERSION" rose.nsi )
if [ -f "$I/Rose-Setup-${VERSION}.exe" ]; then
  mv "$I/Rose-Setup-${VERSION}.exe" "$DIST/"
  ( cd "$DIST" && sha256sum "Rose-Setup-${VERSION}.exe" > SHA256SUMS.txt )
  echo "------------------------------------------------------------"
  echo "✓ Installeur produit : $DIST/Rose-Setup-${VERSION}.exe"
  echo "  empreinte : $(cat "$DIST/SHA256SUMS.txt")"
else
  echo "✗ Échec du build NSIS (makensis absent ? rose.nsi invalide ?)"
  exit 1
fi
