#!/usr/bin/env python3
"""Emballe les ressources Windows de l'installeur Rose dans un `.res` COFF.

Win32 ne sait pas lire une police WOFF2 ni une image PNG directement : on
construit une table des ressources (section `.rsrc`) qui contient

- `RT_RCDATA  1  ROSE_PNG`   : la rose (PNG, 176 px) ;
- `RT_FONT    2  KARE_TTF`   : la police Kare convertie en TrueType ;
- `RT_ICON    1..n` + `RT_GROUP_ICON 1` : TOUTES les tailles de `setup.ico`
  (16, 32, 48, 64, 128, 256) — Win32 choisit celle qui convient, et la grande
  sert de `WM_SETICON` de la fenêtre.

Le fichier produit est un objet COFF complet : un en-tête de fichier, un
en-tête de section et une section `.rsrc` (lecture seule, initialisée). Les
pointeurs de la table des ressources sont relatifs au DÉBUT de la section.

Sortie : `rose/rose-setup.res`, lié au seul binaire `rose-setup` par
`build.rs` (`cargo:rustc-link-arg-bin=rose-setup`).

Dépendances : fontTools (déjà utilisé pour le site), Python 3 standard sinon.

Usage : python3 installateur/velopack/pack-res.py
"""

import struct
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._g_l_y_f import Glyph, GlyphComponent
from fontTools.ttLib.tables.ttProgram import Program

RACINE = Path(__file__).resolve().parents[2]
PNG = RACINE / "installateur" / "velopack" / "rose-installeur.png"
WOFF2 = RACINE / "docs" / "assets" / "fonts" / "kare" / "Kare.woff2"
ICO = RACINE / "installateur" / "assets" / "setup.ico"
SORTIE = RACINE / "rose" / "rose-setup.res"

RT_ICON = 3
RT_GROUP_ICON = 14
RT_RCDATA = 10
RT_FONT = 8


def woff2_vers_ttf(chemin: Path) -> bytes:
    """Convertit la police WOFF2 en TrueType simple (glyphes + métriques)."""
    f = TTFont(chemin)
    ordre = f.getGlyphOrder()
    glyphes = {}
    for g in ordre:
        source = f["glyf"][g]
        neuf = Glyph()
        neuf.numberOfContours = source.numberOfContours
        neuf.program = Program()
        if source.numberOfContours > 0:
            coords, ends, flags = source.getCoordinates(f["glyf"])
            neuf.coordinates = coords
            neuf.flags = flags
            neuf.endPtsOfContours = ends
        elif source.numberOfContours == -1:
            comps = []
            for c in source.components:
                nc = GlyphComponent()
                nc.glyphName = c.glyphName
                nc.x, nc.y = c.x, c.y
                nc.flags = c.flags
                nc.transform = getattr(c, "transform", [[1, 0], [0, 1]])
                comps.append(nc)
            neuf.components = comps
        glyphes[g] = neuf

    fb = FontBuilder(f["head"].unitsPerEm, isTTF=True)
    fb.setupGlyphOrder(ordre)
    fb.setupCharacterMap(f.getBestCmap())
    fb.setupGlyf(glyphes)
    fb.setupHorizontalMetrics({g: (f["hmtx"][g][0], f["hmtx"][g][1]) for g in ordre})
    fb.setupHorizontalHeader(ascent=f["hhea"].ascent, descent=f["hhea"].descent)
    fb.setupNameTable(
        {
            "familyName": "Kare",
            "styleName": "Regular",
            "uniqueFontIdentifier": "Kare-Regular",
            "fullName": "Kare",
            "psName": "Kare-Regular",
            "version": "Version 1.0",
        }
    )
    fb.setupOS2(
        sTypoAscender=f["hhea"].ascent,
        sTypoDescender=f["hhea"].descent,
        usWinAscent=f["hhea"].ascent,
        usWinDescent=-f["hhea"].descent,
    )
    fb.setupPost()
    temporaire = RACINE / "rose" / "kare.ttf"
    fb.font.save(str(temporaire))
    octets = temporaire.read_bytes()
    temporaire.unlink()
    return octets


def entrees_ico(chemin: Path):
    """Rend la liste des entrées de l'ICO, de la plus petite à la plus grande."""
    d = chemin.read_bytes()
    reservee, type_, n = struct.unpack("<HHH", d[:6])
    assert reservee == 0 and type_ == 1, "ICO attendu"
    entrees = []
    for i in range(n):
        o = 6 + 16 * i
        largeur = d[o] or 256
        hauteur = d[o + 1] or 256
        couleurs = d[o + 2]
        plans = struct.unpack("<H", d[o + 4 : o + 6])[0]
        bits = struct.unpack("<H", d[o + 6 : o + 8])[0]
        taille = struct.unpack("<I", d[o + 8 : o + 12])[0]
        decalage = struct.unpack("<I", d[o + 12 : o + 16])[0]
        entrees.append(
            {
                "largeur": largeur,
                "hauteur": hauteur,
                "couleurs": couleurs,
                "plans": plans or 1,
                "bits": bits or 32,
                "donnees": d[decalage : decalage + taille],
            }
        )
    entrees.sort(key=lambda e: e["largeur"] * e["hauteur"])
    return entrees


def aligner(donnees: bytes) -> bytes:
    reste = len(donnees) % 4
    return donnees + bytes(4 - reste) if reste else donnees


def noeud(entrees):
    """Nœud de répertoire : en-tête (16 o) + entrées (8 o) + sentinelle.

    `entrees` : liste de (nom ou type, pointeur). Le pointeur porte le bit 31
    (0x80000000) s'il désigne un sous-répertoire ; sinon c'est l'offset d'une
    entrée de données.
    """
    tete = struct.pack("<IIHHHH", 0, 0, 0, 0, len(entrees), len(entrees) & 0xffff)
    corps = b"".join(struct.pack("<II", a, b) for a, b in entrees)
    return tete + corps + struct.pack("<II", 0, 0)


def main():
    png = PNG.read_bytes()
    ttf = woff2_vers_ttf(WOFF2)
    icones = entrees_ico(ICO)

    # Ressources : (type, nom, octets), icônes identifiées 1..n.
    ressources = [(RT_RCDATA, 1, png), (RT_FONT, 2, ttf)]
    for i, e in enumerate(icones, start=1):
        ressources.append((RT_ICON, i, e["donnees"]))

    groupe = struct.pack("<HHH", 0, 1, len(icones))
    for i, e in enumerate(icones, start=1):
        groupe += struct.pack(
            "<BBBBHHIH",
            e["largeur"] % 256,
            e["hauteur"] % 256,
            e["couleurs"],
            0,
            e["plans"],
            e["bits"],
            len(e["donnees"]),
            i,
        )
    ressources.append((RT_GROUP_ICON, 1, groupe))

    # ── arbre .rsrc ────────────────────────────────────────────────────
    # Niveau 1 (racine)   : un enfant par type.
    # Niveau 2 (par type) : un enfant par nom.
    # Niveau 3 (par nom)  : une langue (id 0) -> entrée de données.
    # Chaque nœud = 16 o d'en-tête + 8 o par enfant + 8 o de sentinelle.
    n = len(ressources)
    noeud_enfant = 16 + 8 * 2  # un enfant + sentinelle
    offset_types = 16 + 8 * (n + 1)  # racine : n types + sentinelle
    offset_noms = offset_types + n * noeud_enfant
    offset_donnees = offset_noms + n * noeud_enfant
    offset_brut = offset_donnees + 16 * n  # 16 o par entrée de données
    offset_brut += (-offset_brut) % 4

    racine = noeud(
        [
            (0x8000 | type_, 0x80000000 | (offset_types + i * noeud_enfant))
            for i, (type_, _, _) in enumerate(ressources)
        ]
    )
    blocs_types = bytearray()
    blocs_noms = bytearray()
    for i, (type_, nom, _) in enumerate(ressources):
        blocs_types += noeud([(0x8000 | nom, 0x80000000 | (offset_noms + i * noeud_enfant))])
        blocs_noms += noeud([(0, 0x80000000 | (offset_donnees + i * 16))])

    # Entrées de données (offset relatif au début de la section, puis taille)
    # et octets bruts alignés sur 4.
    blocs_donnees = bytearray()
    corps = bytearray()
    decalage = offset_brut
    for _, _, octets in ressources:
        blocs_donnees += struct.pack("<II", decalage, len(octets))
        decalage += len(aligner(octets))
        corps += aligner(octets)

    section = (
        racine
        + bytes(blocs_types)
        + bytes(blocs_noms)
        + bytes(blocs_donnees)
        + bytes(offset_brut - len(racine) - len(blocs_types) - len(blocs_noms) - len(blocs_donnees))
        + bytes(corps)
    )
    assert len(racine) == offset_types
    assert offset_types + len(blocs_types) == offset_noms
    assert offset_noms + len(blocs_noms) == offset_donnees
    assert offset_donnees + len(blocs_donnees) <= offset_brut
    assert len(section) == offset_brut + len(corps)

    entete_fichier = struct.pack(
        "<HHIIIHH",
        0x8664,  # machine x86-64
        1,  # une section
        0,  # pas d'horodatage (reproductible)
        0,  # pas de table de symboles
        0,  # pas de symboles
        0,  # taille de l'en-tête optionnel
        0,  # caractéristiques
    )
    entete_section = struct.pack(
        "<8sIIIIIIHHI",
        b".rsrc\x00\x00\x00",
        len(section),  # taille virtuelle
        0,  # adresse virtuelle
        len(section),  # taille brute
        len(entete_fichier) + 40,  # décalage brut des données
        0,  # pas de relocations
        0,  # pas de numéros de ligne
        0,  # pas de relocations étendues
        0,  # pas de numéros de ligne étendus
        0x40000040,  # IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ
    )
    SORTIE.write_bytes(entete_fichier + entete_section + section)
    print(f"{SORTIE.relative_to(RACINE)} : {SORTIE.stat().st_size} o")
    print(f"  rose PNG  {len(png)} o | Kare TTF {len(ttf)} o")
    print("  icônes    " + " ".join(f"{e['largeur']}px" for e in icones))


if __name__ == "__main__":
    main()
