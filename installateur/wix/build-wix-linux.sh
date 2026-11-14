#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
#  Validation Linux de l'installeur WiX (produit un .msi sans UI).
#
#  Utilise wixl (msitools) dans un conteneur Debian. wixl ne gère pas
#  tous les éléments WiX (Environment, RemoveFolder, UIRef) : on construit
#  donc une copie épurée pour valider la charge utile (fichiers, registre,
#  fonctionnalités). La source de référence reste Product.wxs (Windows).
#
#  Sortie : installateur/dist/Rose-Setup-${VERSION}.msi (+ SHA256SUMS.txt)
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
DIST="$(cd "$HERE/../.." && pwd)/installateur/dist"
VERSION="${VERSION:-0.1.4}"

# préparation du staging épuré
STAGE="$(mktemp -d /tmp/opencode/rose-wix.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT
cp "$HERE"/*.wxs "$STAGE"/ 2>/dev/null || true
cp "$HERE"/Product.wxs "$STAGE"/Product.wxs
# fichiers embarqués
for f in rose.exe LLVM-C.dll rose.ico setup.ico license.rtf \
         README-PARTAGE.md conformite.txt; do
  if [ -f "$HERE/$f" ]; then cp "$HERE/$f" "$STAGE/"; fi
done
# copie depuis staging global si besoin
if [ ! -f "$STAGE/rose.exe" ] && [ -f "$HERE/../staging/rose.exe" ]; then
  cp "$HERE/../staging/rose.exe" "$STAGE/"
fi
if [ ! -f "$STAGE/LLVM-C.dll" ] && [ -f "$HERE/../staging/LLVM-C.dll" ]; then
  cp "$HERE/../staging/LLVM-C.dll" "$STAGE/"
fi

# épurage des éléments non supportés par wixl
python3 - "$STAGE/Product.wxs" <<'PY'
import re,sys
p=sys.argv[1]
s=open(p).read()
# retire Environment
s=s.replace('''            <Environment Id="EnvPath" Name="PATH" Value="[BINDIR]" Permanent="no"
                         Part="last" Action="set" System="no" />
''','')
# retire RemoveFolder
s=s.replace('            <RemoveFolder Id="RemoveRoseMenu" Directory="RoseMenuFolder" On="uninstall" />\n','')
open(p,'w').write(s)
PY

mkdir -p "$DIST"
docker run --rm -v "$STAGE:/work" debian:bookworm bash -c \
  "apt-get update -qq >/dev/null 2>&1; apt-get install -y wixl >/dev/null 2>&1; cd /work && wixl -a x64 -o Rose-Setup-${VERSION}.msi Product.wxs" \
  2>&1 | tail -20

if [ -f "$STAGE/Rose-Setup-${VERSION}.msi" ]; then
  cp "$STAGE/Rose-Setup-${VERSION}.msi" "$DIST/"
  ( cd "$DIST" && sha256sum "Rose-Setup-${VERSION}.msi" > SHA256SUMS.txt )
  echo "✓ MSI (validation) : $DIST/Rose-Setup-${VERSION}.msi"
else
  echo "✗ Échec du build wixl"; exit 1
fi
