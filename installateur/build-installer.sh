#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
#  Construit l'installeur Windows « Rose-Setup-<version>.exe » (NSIS 3).
<<<<<<< HEAD
#  Source des fichiers : le binaire Rust « rose/target/release/rose-20 »
#  (version Windows à compiler séparément pour Windows) + les icônes du dépôt.
#  Sortie : installateur/dist/.
=======
#  Source des fichiers : le zip de partage docs/rose-windows.zip (moteurs
#  C++ et Rust) + les icônes du dépôt. Sortie : installateur/dist/.
>>>>>>> c11bfb4ecbfdc958580c6022a30f016efd6069ab
#  makensis est cherché dans $PATH, puis $MAKENSIS, puis l'extraction
#  locale /tmp/opencode/apt/nsisroot (paquet nsis dépaqueté sans root).
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail
RACINE="$(cd "$(dirname "$0")/.." && pwd)"
I="$RACINE/installateur"
# Le binaire Windows Rust (à compiler séparément pour Windows, ex. via cargo cross)
EXE_SRC="$RACINE/rose/target/release/rose-20"
STAGE="$(mktemp -d /tmp/opencode/rose-stage.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$STAGE/bin"
if [ -f "$EXE_SRC" ]; then
  cp "$EXE_SRC" "$STAGE/bin/rose-20.exe"
else
  echo "ATTENTION: $EXE_SRC introuvable — l'installeur contiendra un placeholder"
  touch "$STAGE/bin/rose-20.exe"
fi

mkdir -p "$I/staging"
cp "$STAGE/bin/rose-20.exe" "$I/staging/rose-20.exe"

mkdir -p "$DIST"
( cd "$I" && "$MK" -V2 -DAPP_VERSION="$VERSION" rose.nsi )
mv "$I/Rose-Setup-${VERSION}.exe" "$DIST/"
( cd "$DIST" && sha256sum "Rose-Setup-${VERSION}.exe" > SHA256SUMS.txt )
ls -la "$DIST"
echo "empreinte : $(cat "$DIST/SHA256SUMS.txt")"
