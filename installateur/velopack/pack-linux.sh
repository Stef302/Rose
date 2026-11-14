#!/bin/sh
# Empacte le dossier `stage` de Rose en paquet Velopack Windows (.nupkg).
#
# Un paquet Velopack « full » est une archive ZIP qui contient à sa racine :
#   lib/net45/...        (ici : les binaires du moteur et l'installeur)
#   sq.version           (version + id du paquet, format SemVer Velopack)
# Produit :
#   installateur/velopack/Releases/Rose-<version>-full.nupkg
#   installateur/velopack/Releases/releases.win.json  (manifeste lu par `rose update`)
#
# Aucune dépendance : `zip` n'est pas requis, Python 3 suffit.
#
# Usage : sh installateur/velopack/pack-linux.sh [version]
set -eu

RACINE="$(cd "$(dirname "$0")/../.." && pwd)"
D="$RACINE/installateur/velopack"
STAGE="$D/stage"
RELEASES="$D/Releases"
VERSION="${1:-$(grep -m1 '^version' "$RACINE/rose/Cargo.toml" | cut -d'"' -f2)}"

[ -f "$STAGE/rose.exe" ] || { echo "stage incomplet — lancez build-linux.sh" >&2; exit 1; }

python3 - "$STAGE" "$RELEASES" "$VERSION" <<'PY'
import json, os, sys, zipfile, hashlib, base64

stage, releases, version = sys.argv[1], sys.argv[2], sys.argv[3]
os.makedirs(releases, exist_ok=True)
nom = f"Rose-{version}-full.nupkg"
cible = os.path.join(releases, nom)

# Horodatage FIXE de chaque entrée : deux exécutions du même stage donnent
# le même paquet, donc le même SHA-256 (sinon le manifeste publié et le
# fichier servi divergent à chaque build).
DATE_FIXE = (2026, 1, 1, 0, 0, 0)

def entree(z, arc, donnees):
    info = zipfile.ZipInfo(arc, date_time=DATE_FIXE)
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o644 << 16
    z.writestr(info, donnees)

with zipfile.ZipFile(cible, "w", zipfile.ZIP_DEFLATED) as z:
    for racine, _, fichiers in os.walk(stage):
        for f in sorted(fichiers):
            chemin = os.path.join(racine, f)
            arc = os.path.join("lib", "net45", os.path.relpath(chemin, stage))
            entree(z, arc.replace(os.sep, "/"), open(chemin, "rb").read())
    # sq.version : Velopack exige ce fichier à la racine du paquet
    entree(z, "sq.version",
           (f'<Package xmlns="http://schemas.microsoft.com/packaging/2010/07/nuspec.xsd">'
            f'<metadata><id>Rose</id><version>{version}</version>'
            f'<authors>Stef302</authors><description>Rose</description>'
            f'</metadata></Package>').encode())

octets = open(cible, "rb").read()
sha256 = hashlib.sha256(octets).hexdigest()
sha1 = hashlib.sha1(octets).hexdigest()
sha512 = base64.b64encode(hashlib.sha512(octets).digest()).decode()

manifeste = [{
    "PackageId": nom,
    "Version": version,
    "Type": "Full",
    "FileName": nom,
    "SHA1": sha1,
    "SHA256": sha256,
    "SHA512": sha512,
    "Size": len(octets),
}]
with open(os.path.join(releases, "releases.win.json"), "w") as f:
    json.dump(manifeste, f, indent=2)

print(f"paquet   : {cible} ({len(octets)} o)")
print(f"sha256   : {sha256}")
print(f"manifeste: {os.path.join(releases, 'releases.win.json')}")
PY

echo
echo "Publier : déposer Rose-$VERSION-full.nupkg et releases.win.json dans la"
echo "release GitHub (assets) — 'rose update --velopack' les y lira."
