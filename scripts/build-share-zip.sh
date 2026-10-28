#!/usr/bin/env bash
# Construit rose-windows.zip pour la distribution (site GitHub Pages) :
#   - PLAT (pas de dossier englobant rose-windows/ : sinon « Extraire
#     tout » de l'Explorateur crée un doublon rose-windows\rose-windows) ;
#   - noms de fichiers avec la MARQUE UTF-8 du zip (sans elle, l'Extract
#     All de l'Explorateur et .NET lisent « dorés » en CP437 -> « dor├⌐s »
#     et les 31 tests dorés s'écrasent en PathNotFound) ;
#   - SHA256SUMS.txt intérieur = contenu seul (pas d'auto-référence) ;
#   - déploie dans docs/ (le site GitHub Pages ; previsualisation 18080)
set -eu
cd "$(dirname "$0")/.."
OUT="docs/rose-windows.zip"
SHARE="/tmp/opencode/share"

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$STAGE/bin" "$STAGE/moteur/tests"
cp moteur/cpp/rose-win64.exe      "$STAGE/bin/rose-win64.exe"
cp moteur/rust/rose-rs-win64.exe  "$STAGE/bin/rose-rs-win64.exe"
cp moteur/zig/rose-zig-win64.exe  "$STAGE/bin/rose-zig-win64.exe"
cp -r moteur/tests/dorés          "$STAGE/moteur/tests/dorés"
cp -r moteur/tests/roses          "$STAGE/moteur/tests/roses"
cp -r moteur/tests/bench          "$STAGE/bench"
cp tester.ps1 TESTER.md README-PARTAGE.md "$STAGE/"

# empreintes du contenu (avant d'ajouter le fichier d'empreintes lui-même)
( cd "$STAGE" && find . -type f ! -name SHA256SUMS.txt -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS.txt )

python3 - "$STAGE" "$OUT" <<'PY'
import os, sys, zipfile
src, out = sys.argv[1], sys.argv[2]
noms = []
for racine, _, fichiers in os.walk(src):
    for f in fichiers:
        noms.append(os.path.relpath(os.path.join(racine, f), src))
noms.sort()
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for n in noms:
        z.write(os.path.join(src, n), n.replace(os.sep, "/"))
print(f"{len(noms)} fichiers empaquetés, plateaux inclus")
PY

# empreinte du zip à la racine du site (hors zip, pas d'auto-référence)
( cd docs && sha256sum rose-windows.zip > SHA256SUMS.txt )

if [ -d "$SHARE" ]; then
  cp "$OUT" docs/SHA256SUMS.txt "$SHARE/"
  echo "partage legacy $SHARE mis à jour"
fi

python3 - "$OUT" <<'PY'
import sys, zipfile
z = zipfile.ZipFile(sys.argv[1])
mauvais = [i.filename for i in z.infolist()
           if not i.filename.startswith("rose-windows/")
           and any(ord(c) > 127 for c in i.filename)
           and not (i.flag_bits & 0x800)]
accents = [i.filename for i in z.infolist() if "dorés" in i.filename]
assert mauvais == [], f"noms non-ASCII sans marque UTF-8 : {mauvais[:3]}"
assert len(accents) >= 30, "dossier dorés absent ?"
print("marque UTF-8 OK,", len(accents), "entrées dorés, plat :",
      not any(i.filename.startswith("rose-windows/") for i in z.infolist()))
PY
