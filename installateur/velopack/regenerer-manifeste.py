#!/usr/bin/env python3
"""Régénère `releases.win.json` depuis les .nupkg RÉELLEMENT présents.

`pack-linux.sh` n'écrit qu'UNE entrée (la version qu'il vient d'empaqueter) :
à chaque release, le manifeste perd l'historique. Ce script relit les paquets
du dossier `Releases/` et recalcule toutes les empreintes sur les octets du
disque — aucune valeur n'est inventée ni recopiée d'un manifeste périmé.

Usage :
    python3 installateur/velopack/regenerer-manifeste.py [version ...]

Sans argument : TOUS les `Rose-<version>-full.nupkg` présents sont inclus.
Avec des versions : seules celles demandées sont incluses (utile pour limiter
le manifeste à une fenêtre de versions, ex. 0.1.76 → 0.1.79).

Écrit :
    installateur/velopack/Releases/releases.win.json       (source)
    docs/telechargements/releases.win.json                  (copie publiée sur le site)

Les deux fichiers sont identiques octet pour octet.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent.parent
RELEASES = RACINE / "installateur" / "velopack" / "Releases"
SITE = RACINE / "docs" / "telechargements"
NOM_MANIFESTE = "releases.win.json"
MOTIF_NUPKG = re.compile(r"^Rose-(\d+\.\d+\.\d{1,2})-full\.nupkg$")


def cle_version(version: str) -> tuple[int, ...]:
    """Même ordre que rose/src/velopack.rs::comparer_versions."""
    return tuple(int(p) for p in version.split("."))


def empreintes(chemin: Path) -> dict:
    octets = chemin.read_bytes()
    return {
        "PackageId": chemin.name,
        "Version": chemin.name[len("Rose-"):-len("-full.nupkg")],
        "Type": "Full",
        "FileName": chemin.name,
        "SHA1": hashlib.sha1(octets).hexdigest(),
        "SHA256": hashlib.sha256(octets).hexdigest(),
        "SHA512": base64.b64encode(hashlib.sha512(octets).digest()).decode(),
        "Size": len(octets),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("versions", nargs="*", help="versions à inclure (défaut : toutes)")
    args = ap.parse_args()

    presents: dict[str, Path] = {}
    for f in RELEASES.glob("Rose-*-full.nupkg"):
        m = MOTIF_NUPKG.fullmatch(f.name)
        if m:
            presents[m.group(1)] = f
    if not presents:
        raise SystemExit(f"aucun nupkg dans {RELEASES}")

    versions = args.versions or sorted(presents, key=cle_version, reverse=True)
    inconnues = [v for v in versions if v not in presents]
    if inconnues:
        raise SystemExit(f"nupkg absent pour : {', '.join(inconnues)}")

    manifeste = [empreintes(presents[v]) for v in
                 sorted(versions, key=cle_version, reverse=True)]
    contenu = json.dumps(manifeste, indent=2, ensure_ascii=False) + "\n"
    for cible in (RELEASES / NOM_MANIFESTE, SITE / NOM_MANIFESTE):
        cible.parent.mkdir(parents=True, exist_ok=True)
        cible.write_text(contenu, encoding="utf-8")

    for e in manifeste:
        print(f"{e['Version']:8s} {e['Size']:>10d} o  sha256 {e['SHA256'][:16]}…")
    print(f"manifeste : {len(manifeste)} entrée(s) → {RELEASES / NOM_MANIFESTE}")
    print(f"copie site: {SITE / NOM_MANIFESTE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
