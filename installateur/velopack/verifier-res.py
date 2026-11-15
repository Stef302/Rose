#!/usr/bin/env python3
"""Valide la section `.rsrc` produite par `pack-res.py` (et le cas échéant
liée dans `rose-setup.exe`) sans Windows.

Contrôles :
- en-tête COFF et en-tête de section ;
- arbre des ressources : racine → types → noms → langues → entrées de données ;
- NumberOfNamedEntries == 0 (toutes nos entrées sont numériques) ;
- chaque entrée de données tient dans la section, avec un offset aligné ;
- les ressources attendues existent : `RT_RCDATA 1` (rose PNG), `RT_FONT 2`
  (Kare TrueType), `RT_ICON 1..n` et `RT_GROUP_ICON 1` dont les tailles
  correspondent aux icônes ;
- la signature des octets (PNG, TTF, groupe d'icônes).

Usage :
    python3 installateur/velopack/verifier-res.py            # rose/rose-setup.res
    python3 installateur/velopack/verifier-res.py rose-setup.exe
"""

import struct
import sys
from pathlib import Path

RACINE = Path(__file__).resolve().parents[2]
RT_ICON, RT_GROUP_ICON, RT_RCDATA, RT_FONT, RT_MANIFEST = 3, 14, 10, 8, 24


def section_rsrc(donnees: bytes) -> bytes:
    """Extrait la section `.rsrc` d'un `.res` COFF ou d'un PE."""
    if donnees[:2] == b"MZ":
        pe = struct.unpack("<I", donnees[0x3C:0x40])[0]
        assert donnees[pe:pe + 4] == b"PE\0\0", "signature PE absente"
        nsec = struct.unpack("<H", donnees[pe + 6:pe + 8])[0]
        opt = pe + 24
        taille_opt = struct.unpack("<H", donnees[pe + 20:pe + 22])[0]
        debut = opt + taille_opt
        for i in range(nsec):
            o = debut + 40 * i
            nom = donnees[o:o + 8].rstrip(b"\0")
            if nom == b".rsrc":
                taille_brute, decalage = struct.unpack("<II", donnees[o + 16:o + 24])
                return donnees[decalage:decalage + taille_brute]
        raise AssertionError("section .rsrc absente du PE")
    # objet COFF : 20 o d'en-tête, puis une section de 40 o
    machine, nsec = struct.unpack("<HH", donnees[:4])
    assert machine == 0x8664, f"machine inattendue {machine:#x}"
    assert nsec == 1, f"{nsec} sections dans le .res"
    nom, taille_virt, va, taille_brute, decalage = struct.unpack("<8sIIII", donnees[20:44])
    assert nom.rstrip(b"\0") == b".rsrc", nom
    return donnees[decalage:decalage + taille_brute]


def entrees(rsrc: bytes, offset: int):
    nommees, ids = struct.unpack("<HH", rsrc[offset + 12:offset + 16])
    return [
        struct.unpack("<II", rsrc[offset + 16 + 8 * j:offset + 24 + 8 * j])
        for j in range(nommees + ids)
    ], nommees, ids


def main():
    cible = Path(sys.argv[1]) if len(sys.argv) > 1 else RACINE / "rose" / "rose-setup.res"
    if not cible.is_absolute():
        cible = (RACINE / cible) if (RACINE / cible).exists() else cible
    rsrc = section_rsrc(cible.read_bytes())
    print(f"{cible.name} : section .rsrc de {len(rsrc)} o")

    racine, nommees, ids = entrees(rsrc, 0)
    assert nommees == 0, f"NumberOfNamedEntries doit valoir 0, lu {nommees}"
    print(f"  racine : {ids} types")

    ressources = {}
    for type_, pointeur in racine:
        assert pointeur & 0x80000000, f"type {type_} sans sous-répertoire"
        noeuds, nn, ni = entrees(rsrc, pointeur & 0x7FFFFFFF)
        assert nn == 0, f"type {type_} : {nn} noms textuels inattendus"
        for nom, p2 in noeuds:
            assert p2 & 0x80000000, f"nom {nom} du type {type_} sans langue"
            langues, _, _ = entrees(rsrc, p2 & 0x7FFFFFFF)
            for langue, p3 in langues:
                # Windows résout par langue : 1033 (en-US) comme `llvm-rc`.
                assert langue == 1033, (
                    f"type {type_} nom {nom} : langue {langue} au lieu de 1033")
                o = p3 & 0x7FFFFFFF
                assert not p3 & 0x80000000, "entrée de données mal pointée"
                decalage, taille, page, reserve = struct.unpack("<IIII", rsrc[o:o + 16])
                assert decalage % 4 == 0, f"offset non aligné ({decalage})"
                assert decalage + taille <= len(rsrc), (
                    f"type {type_} nom {nom} : {decalage}+{taille} > {len(rsrc)}")
                ressources[(type_, nom)] = rsrc[decalage:decalage + taille]

    print(f"  {len(ressources)} ressources :")
    for (type_, nom), octets in sorted(ressources.items()):
        print(f"    type {type_:<2} nom {nom:<2} {len(octets):>7} o")

    png = ressources.get((RT_RCDATA, 1))
    assert png and png[:8] == b"\x89PNG\r\n\x1a\n", "rose PNG absente ou invalide"
    ttf = ressources.get((RT_FONT, 2))
    assert ttf and ttf[:4] in (b"\x00\x01\x00\x00", b"true"), "police Kare absente"
    groupe = ressources.get((RT_GROUP_ICON, 1))
    assert groupe, "groupe d'icônes absent"
    _, _, n = struct.unpack("<HHH", groupe[:6])
    assert n >= 1, "groupe d'icônes vide"
    attendues = []
    for i in range(n):
        o = 6 + 14 * i
        largeur, hauteur, _, _, plans, bits, taille, ident = struct.unpack("<BBBBHHIH", groupe[o:o + 14])
        icone = ressources.get((RT_ICON, ident))
        assert icone is not None, f"icône {ident} référencée mais absente"
        assert len(icone) == taille, f"icône {ident} : {len(icone)} o au lieu de {taille}"
        attendues.append(f"{largeur or 256}x{hauteur or 256}")

    icones = sorted(n for (t, n) in ressources if t == RT_ICON)
    assert icones == list(range(1, n + 1)), f"identifiants d'icônes non contigus : {icones}"
    print(f"  groupe d'icônes : {n} tailles ({', '.join(attendues)})")

    # Manifeste : les règles Win32 que le CHARGEUR impose. Un manifeste
    # refusé fait afficher « Cette application ne peut pas s'exécuter sur
    # votre PC » avant même l'exécution du programme.
    manifeste = ressources.get((RT_MANIFEST, 1))
    if manifeste is not None:
        assert manifeste[:5] == b"<?xml", "le manifeste ne commence pas par la déclaration XML"
        assert b"<!--" not in manifeste.split(b"?>")[0], "commentaire avant la déclaration XML"
        non_ascii = sum(1 for b in manifeste if b > 127)
        assert non_ascii == 0, f"manifeste : {non_ascii} octet(s) non-ASCII (activation stricte)"
        assert b'processorArchitecture="*"' in manifeste, \
            'processorArchitecture doit valoir "*" (une valeur explicite fait échouer l\'activation)'
        assert b"PerMonitorV2" in manifeste, "manifeste : PerMonitorV2 absent"
        print(f"  manifeste : {len(manifeste)} o, ASCII, PerMonitorV2")

    print("RESSOURCES VALIDES")


if __name__ == "__main__":
    main()
