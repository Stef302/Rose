#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
#  Construit l'installeur Windows « Rose-Setup-<version>.exe » (NSIS 3).
#  Source des fichiers : le zip de partage docs/rose-windows.zip (moteur
#  C++ uniquement) + les icônes du dépôt. Sortie : installateur/dist/.
#  makensis est cherché dans $PATH, puis $MAKENSIS, puis l'extraction
#  locale /tmp/opencode/apt/nsisroot (paquet nsis dépaqueté sans root).
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail
RACINE="$(cd "$(dirname "$0")/.." && pwd)"
I="$RACINE/installateur"
ZIP="$RACINE/docs/rose-windows.zip"
DIST="$I/dist"

VERSION="${ROSE_VERSION:-$(grep -o 'APP_VERSION "[0-9.]*"' "$I/rose.nsi" | cut -d'"' -f2)}"

MK="${MAKENSIS:-}"
if [ -z "$MK" ]; then
  MK="$(command -v makensis || true)"
fi
if [ -z "$MK" ] && [ -x /tmp/opencode/apt/nsisroot/usr/bin/makensis ]; then
  MK=/tmp/opencode/apt/nsisroot/usr/bin/makensis
  export NSISDIR=/tmp/opencode/apt/nsisroot/usr/share/nsis
fi
[ -n "$MK" ] || { echo "makensis introuvable — installez NSIS 3 ou dépaqueté le paquet nsis" >&2; exit 1; }

STAGE="$(mktemp -d /tmp/opencode/rose-stage.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT
unzip -qo "$ZIP" -d "$STAGE/zip" 'bin/rose-win64.exe' 'README-PARTAGE.md' 'TESTER.md'
mkdir -p "$I/staging"
cp "$STAGE/zip/bin/rose-win64.exe" "$I/staging/rose-win64.exe"
cp "$STAGE/zip/README-PARTAGE.md" "$STAGE/zip/TESTER.md" "$I/staging/"

mkdir -p "$DIST"
( cd "$I" && "$MK" -V2 -DAPP_VERSION="$VERSION" rose.nsi )
mv "$I/Rose-Setup-${VERSION}.exe" "$DIST/"
( cd "$DIST" && sha256sum "Rose-Setup-${VERSION}.exe" > SHA256SUMS.txt )
ls -la "$DIST"
echo "empreinte : $(cat "$DIST/SHA256SUMS.txt")"
