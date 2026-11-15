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
# sans le mot « ROSE » gravé en bas du visuel d'origine. Mesuré sur
# `logo-rose.png` : la tige s'arrête à la ligne 903/1400, le mot « ROSE »
# commence à la ligne 904 → 0.645 coupe exactement entre les deux.
FRACTION_BAS = 0.645


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

    # ⚠⚠ Le fond du visuel source n'est PAS noir pur : c'est un dégradé
    # sombre dont l'alpha varie de 0 à ~165. La version précédente testait
    # `r,g,b < 40` SANS regarder l'alpha, et ne rendait transparents que les
    # pixels dont les trois canaux étaient sous 40. Résultat mesuré : une
    # BANDE de fond semi-opaque subsistait tout autour de la rose, et la
    # boîte englobante « pixels alpha > 8 » englobait TOUT le visuel
    # (273..1126 × 62..902) au lieu de la fleur (424..951 × 230..902).
    # La rose se retrouvait collée au coin supérieur gauche d'un carré
    # presque entièrement transparent : à l'écran, le logo était invisible
    # ou réduit à une tache — l'installeur paraissait VIDE.
    #
    # Le bon critère est donc : transparent si l'alpha est faible, OU si le
    # pixel est sombre (le dessin est en APLATS rouge/vert : le noir ne
    # porte aucune information, il n'est que fond et contours).
    def est_fond(p):
        return p[3] <= 8 or (p[0] < 40 and p[1] < 40 and p[2] < 40)

    for y in range(h):
        ligne = lignes[y]
        for x in range(w):
            o = x * 4
            if est_fond((ligne[o], ligne[o + 1], ligne[o + 2], ligne[o + 3])):
                ligne[o:o + 4] = b"\0\0\0\0"

    # recadrage : la rose seule (haut du visuel), sans le mot ROSE gravé.
    # La boîte englobante se calcule sur les pixels NON-FOND (le fond a
    # déjà été rendu transparent), sinon elle englobait tout le visuel et
    # ramenait le mot « ROSE » dans le cadre.
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
    # Le bord bas ne doit pas déborder sur le mot « ROSE » : on rogne juste
    # sous le dernier pixel de la tige (plus de marge en bas).
    y1 = min(y1, maxy + 1)
    largeur = x1 - x0
    hauteur = y1 - y0

    # ⚠⚠ PAS de canevas CARRÉ. La rose avec sa tige est plus HAUTE que large
    # (528 × 673 dans le visuel source). La centrer sur un carré ajoutait
    # deux bandes transparentes à gauche et à droite ; à l'écran, le logo
    # occupait alors une fraction de la place prévue et paraissait minuscule
    # — l'installeur semblait avoir « perdu » son logo. On garde donc les
    # proportions réelles et on met à l'échelle sur le plus grand côté.
    cote = max(largeur, hauteur)
    facteur = 1
    while cote // facteur > 192:
        facteur += 1
    # reste non divisible : on complète les DEUX dimensions en transparent
    reste_w = largeur % facteur
    reste_h = hauteur % facteur
    lw = largeur + (facteur - reste_w if reste_w else 0)
    lh = hauteur + (facteur - reste_h if reste_h else 0)
    canevas = []
    for y in range(lh):
        ligne = bytearray(lw * 4)
        sy = y0 + y
        if y < hauteur:
            src = lignes[sy]
            for x in range(largeur):
                o = (x + x0) * 4
                d = x * 4
                ligne[d : d + 4] = src[o : o + 4]
        canevas.append(ligne)
    petite = reduire(canevas, lw, lh, facteur)
    n_l, n_h = lw // facteur, lh // facteur
    temporaire = RACINE / "installateur" / "velopack" / "rose-installeur.png"
    ecrire_png(temporaire, n_l, n_h, petite)
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
    print(f"rose-installeur.png : {n_l}×{n_h}, {len(png)} o")
    print(f"Kare.woff2 : {len(police)} o")
    print(f"écrit : {SORTIE.relative_to(RACINE)} ({SORTIE.stat().st_size} o)")


if __name__ == "__main__":
    main()
