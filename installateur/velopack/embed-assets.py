#!/usr/bin/env python3
"""Prépare les images embarquées dans l'installeur Windows de Rose.

Sortie : rose/src/assets_installeur.rs (généré, ne pas éditer à la main) qui
contient, sous forme de constantes `&[u8]` :

- ROSE_PETALE_PNG : la rose dessinée (logo-rose.png recadré), affichée dans le
  panneau de gauche de l'installeur, mise à l'échelle de la fenêtre ;
- KARE_WOFF2 : la police d'affichage Kare (domaine public) pour le titre
  « ROSE » — la même que celle du site.

Aucune dépendance : lecture PNG maison (RGBA 8 bits), recadrage sur l'alpha et
ré-encodage PNG minimal (filtre 0, DEFLATE zlib).

Usage : python3 installateur/velopack/embed-assets.py
"""

import struct
import zlib
from pathlib import Path

RACINE = Path(__file__).resolve().parents[2]
SORTIE = RACINE / "rose" / "src" / "assets_installeur.rs"

# zone du logo à garder (fraction de la largeur/hauteur) : la rose seule,
# sans le mot « ROSE » gravé en bas du visuel d'origine
FRACTION_BAS = 0.62


def lire_png(chemin: Path):
    d = chemin.read_bytes()
    assert d[:8] == b"\x89PNG\r\n\x1a\n", f"{chemin} n'est pas un PNG"
    i = 8
    w = h = bd = ct = None
    idat = b""
    while i < len(d):
        (l,) = struct.unpack(">I", d[i : i + 4])
        t = d[i + 4 : i + 8]
        c = d[i + 8 : i + 8 + l]
        if t == b"IHDR":
            w, h, bd, ct = struct.unpack(">IIBB", c[:10])
        elif t == b"IDAT":
            idat += c
        elif t == b"IEND":
            break
        i += 12 + l
    assert bd == 8 and ct == 6, f"{chemin} : attendu RGBA 8 bits, obtenu {bd}/{ct}"
    brut = zlib.decompress(idat)
    bpp = 4
    pas = w * bpp
    precedent = bytearray(pas)
    lignes = []
    for y in range(h):
        f = brut[y * (pas + 1)]
        ligne = bytearray(brut[y * (pas + 1) + 1 : (y + 1) * (pas + 1)])
        if f == 1:
            for x in range(bpp, pas):
                ligne[x] = (ligne[x] + ligne[x - bpp]) & 255
        elif f == 2:
            for x in range(pas):
                ligne[x] = (ligne[x] + precedent[x]) & 255
        elif f == 3:
            for x in range(pas):
                a = ligne[x - bpp] if x >= bpp else 0
                ligne[x] = (ligne[x] + ((a + precedent[x]) >> 1)) & 255
        elif f == 4:
            for x in range(pas):
                a = ligne[x - bpp] if x >= bpp else 0
                b = precedent[x]
                c = precedent[x - bpp] if x >= bpp else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                ligne[x] = (ligne[x] + pr) & 255
        lignes.append(ligne)
        precedent = ligne
    return w, h, lignes


def ecrire_png(chemin: Path, w, h, lignes):
    brut = bytearray()
    for ligne in lignes:
        brut.append(0)
        brut.extend(ligne)

    def bloc(t, c):
        return struct.pack(">I", len(c)) + t + c + struct.pack(">I", zlib.crc32(t + c) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += bloc(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    png += bloc(b"IDAT", zlib.compress(bytes(brut), 9))
    png += bloc(b"IEND", b"")
    chemin.write_bytes(png)


def reduire(lignes, w, h, facteur):
    """Réduction par moyenne de blocs (alpha prémultiplié pour éviter les
    halos sombres autour des traits noirs du dessin)."""
    nw, nh = w // facteur, h // facteur
    sortie = []
    for y in range(nh):
        ligne = bytearray(nw * 4)
        for x in range(nw):
            r = g = b = a = 0
            for dy in range(facteur):
                src = lignes[y * facteur + dy]
                for dx in range(facteur):
                    o = (x * facteur + dx) * 4
                    alpha = src[o + 3]
                    r += src[o] * alpha
                    g += src[o + 1] * alpha
                    b += src[o + 2] * alpha
                    a += alpha
            n = facteur * facteur
            if a == 0:
                continue
            o = x * 4
            ligne[o] = r // a
            ligne[o + 1] = g // a
            ligne[o + 2] = b // a
            ligne[o + 3] = a // n
        sortie.append(ligne)
    return sortie


def rust_bytes(nom, donnees: bytes) -> str:
    lignes = []
    courant = ""
    for octet in donnees:
        morceau = f"0x{octet:02x},"
        if len(courant) + len(morceau) > 96:
            lignes.append(courant)
            courant = ""
        courant += morceau
    if courant:
        lignes.append(courant)
    corps = "\n".join("    " + l for l in lignes)
    return f"pub static {nom}: &[u8] = &[\n{corps}\n];\n"


def main():
    w, h, lignes = lire_png(RACINE / "docs" / "assets" / "img" / "logo-rose.png")

    # recadrage : la rose seule (haut du visuel), sans le mot ROSE gravé
    bas = int(h * FRACTION_BAS)
    minx, maxx, miny, maxy = w, 0, bas, 0
    for y in range(bas):
        ligne = lignes[y]
        for x in range(w):
            if ligne[x * 4 + 3] > 8:
                minx = min(minx, x)
                maxx = max(maxx, x)
                miny = min(miny, y)
                maxy = max(maxy, y)
    marge = 12
    x0 = max(0, minx - marge)
    x1 = min(w, maxx + marge)
    y0 = max(0, miny - marge)
    y1 = min(bas, maxy + marge)
    largeur = x1 - x0
    hauteur = y1 - y0

    # carré : on centre la rose sur un canevas transparent
    cote = max(largeur, hauteur)
    decal_x = (cote - largeur) // 2
    decal_y = (cote - hauteur) // 2
    canevas = []
    for y in range(cote):
        ligne = bytearray(cote * 4)
        sy = y - decal_y
        if 0 <= sy < hauteur:
            src = lignes[y0 + sy]
            for x in range(largeur):
                o = (x + x0) * 4
                d = (x + decal_x) * 4
                ligne[d : d + 4] = src[o : o + 4]
        canevas.append(ligne)

    # taille finale : ≤ 192 px (affiché 160 px, donc net sur écrans HiDPI)
    facteur = 1
    while cote // facteur > 192:
        facteur += 1
    if cote % facteur != 0:
        # reste non divisible : on complète le bord droit/bas en transparent
        reste = cote % facteur
        complete = cote + (facteur - reste)
        for _ in range(complete - cote):
            canevas.append(bytearray(complete * 4))
        for ligne in canevas:
            ligne.extend(bytes((complete - cote) * 4))
        cote = complete
    petite = reduire(canevas, cote, cote, facteur)
    n = cote // facteur
    temporaire = RACINE / "installateur" / "velopack" / "rose-installeur.png"
    ecrire_png(temporaire, n, n, petite)
    png = temporaire.read_bytes()

    police = (RACINE / "docs" / "assets" / "fonts" / "kare" / "Kare.woff2").read_bytes()

    entete = (
        "//! Images et police EMBARQUÉES dans l'installeur Windows de Rose.\n"
        "//!\n"
        "//! Fichier GÉNÉRÉ par `installateur/velopack/embed-assets.py` — ne pas\n"
        "//! éditer à la main. Contenu :\n"
        "//! - `ROSE_PETALE_PNG` : la rose (logo-rose.png recadré, 192 px) ;\n"
        "//! - `KARE_WOFF2` : la police d'affichage Kare (domaine public).\n"
        "\n"
        "#![allow(dead_code)]\n\n"
    )
    SORTIE.write_text(entete + rust_bytes("ROSE_PETALE_PNG", png) + "\n" + rust_bytes("KARE_WOFF2", police))
    print(f"rose-installeur.png : {n}×{n}, {len(png)} o")
    print(f"Kare.woff2 : {len(police)} o")
    print(f"écrit : {SORTIE.relative_to(RACINE)} ({SORTIE.stat().st_size} o)")


if __name__ == "__main__":
    main()
