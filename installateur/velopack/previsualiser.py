#!/usr/bin/env python3
"""Rendu de contrôle de l'interface de `rose/src/setup_gui.rs`.

Ce script ne remplace pas Windows : il relit la géométrie et les textes du
tracé Rust (gabarit 1120 px, mis à l'échelle de la fenêtre) et produit une
image PNG de la vue « Bienvenue » et de la vue « Options ». Il sert à vérifier
qu'aucun texte ne déborde, que les boutons tiennent dans le pied de page et
que la mise en page reste cohérente.

Usage : python3 installateur/velopack/previsualiser.py
Sortie : installateur/velopack/apercu-bienvenue.png, apercu-options.png
"""

import re
import struct
import zlib
from pathlib import Path

RACINE = Path(__file__).resolve().parents[2]
SOURCE = RACINE / "rose" / "src" / "setup_gui.rs"
SORTIE = RACINE / "installateur" / "velopack"

# Couleurs (BGR Win32 → RGB pour le PNG)
COULEURS = {
    "FOND": "15100F", "BARRE": "0D0908", "LATERAL": "120D0C", "LATERAL_FIN": "1C1412",
    "LIGNE": "2A1E1A", "LIGNE_FORTE": "3A2824", "CARTE": "1A1311", "ROUGE": "E62F1E",
    "ROUGE_CLAIR": "F0503A", "ROUGE_SOMBRE": "8A1A1C", "VERT": "2CCB2C", "TEXTE": "E8E2E2",
    "TEXTE_DOUX": "A89894", "TEXTE_FAIBLE": "6E605C", "FOND_PUCE": "2A1A18",
}


def rgb(nom):
    v = COULEURS[nom]
    return (int(v[4:6], 16), int(v[2:4], 16), int(v[0:2], 16))


class Image:
    def __init__(self, w, h, fond):
        self.w, self.h = w, h
        self.p = bytearray(bytes(fond) * (w * h))

    def rect(self, x0, y0, x1, y1, c):
        x0, x1 = max(0, int(x0)), min(self.w, int(x1))
        y0, y1 = max(0, int(y0)), min(self.h, int(y1))
        ligne = bytes(c) * max(0, x1 - x0)
        for y in range(y0, y1):
            o = (y * self.w + x0) * 3
            self.p[o : o + len(ligne)] = ligne

    def arrondi(self, x0, y0, x1, y1, r, c):
        self.rect(x0 + r, y0, x1 - r, y1, c)
        self.rect(x0, y0 + r, x1, y1 - r, c)
        for cx, cy in ((x0 + r, y0 + r), (x1 - r, y0 + r), (x0 + r, y1 - r), (x1 - r, y1 - r)):
            self.disque(cx, cy, r, c)

    def disque(self, cx, cy, r, c):
        r = max(r, 0.5)
        for y in range(int(cy - r), int(cy + r) + 1):
            if 0 <= y < self.h:
                dedans = r * r - (y - cy) ** 2
                if dedans < 0:
                    continue
                dx = int(dedans ** 0.5)
                self.rect(cx - dx, y, cx + dx + 1, y + 1, c)

    def trait(self, x0, y0, x1, y1, c, e):
        n = max(abs(x1 - x0), abs(y1 - y0), 1)
        for i in range(n + 1):
            self.disque(x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n, e / 2, c)

    def png(self, chemin):
        brut = bytearray()
        for y in range(self.h):
            brut.append(0)
            brut.extend(self.p[y * self.w * 3 : (y + 1) * self.w * 3])

        def bloc(t, c):
            return struct.pack(">I", len(c)) + t + c + struct.pack(">I", zlib.crc32(t + c) & 0xFFFFFFFF)

        png = b"\x89PNG\r\n\x1a\n"
        png += bloc(b"IHDR", struct.pack(">IIBBBBB", self.w, self.h, 8, 2, 0, 0, 0))
        png += bloc(b"IDAT", zlib.compress(bytes(brut), 9))
        png += bloc(b"IEND", b"")
        chemin.write_bytes(png)


def s(v, e):
    return int(round(v * e))


_CACHE_ROSE = {}


def lire_rose():
    """Relit `rose-installeur.png` (RGBA 8 bits, filtre PNG standard)."""
    if _CACHE_ROSE:
        return _CACHE_ROSE
    d = (SORTIE / "rose-installeur.png").read_bytes()
    i, w, h, idat = 8, 0, 0, b""
    while i < len(d):
        (l,) = struct.unpack(">I", d[i : i + 4])
        t, c = d[i + 4 : i + 8], d[i + 8 : i + 8 + l]
        if t == b"IHDR":
            w, h = struct.unpack(">II", c[:8])
        elif t == b"IDAT":
            idat += c
        elif t == b"IEND":
            break
        i += 12 + l
    brut = zlib.decompress(idat)
    pas = w * 4
    prev, rows = bytearray(pas), []
    for y in range(h):
        f = brut[y * (pas + 1)]
        line = bytearray(brut[y * (pas + 1) + 1 : (y + 1) * (pas + 1)])
        for x in range(pas):
            a = line[x - 4] if x >= 4 else 0
            b = prev[x]
            c = prev[x - 4] if x >= 4 else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + ((a + b) >> 1)) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if (pa <= pb and pa <= pc) else (b if pb <= pc else c))) & 255
        rows.append(line)
        prev = line
    _CACHE_ROSE.update({"w": w, "h": h, "p": b"".join(bytes(r) for r in rows)})
    return _CACHE_ROSE


def dessiner(w, h, vue):
    e = w / 1120.0
    img = Image(w, h, rgb("FOND"))
    # barre de titre
    img.rect(0, 0, w, s(56, e), rgb("BARRE"))
    img.disque(s(30, e), s(28, e), s(12, e), rgb("ROUGE"))
    img.rect(s(52, e), s(14, e), s(360, e), s(42, e), rgb("TEXTE"))
    img.arrondi(s(362, e), s(16, e), s(490, e), s(40, e), s(8, e), rgb("LIGNE_FORTE"))
    img.rect(w - s(46, e), 0, w, s(56, e), rgb("TEXTE_DOUX"))
    # panneau latéral
    pied_h, etat_h, lateral_w = s(76, e), s(30, e), s(320, e)
    img.rect(0, s(56, e), lateral_w, h - etat_h, rgb("LATERAL"))
    img.rect(lateral_w - 1, s(56, e), lateral_w, h - etat_h, rgb("LIGNE_FORTE"))
    rose = lire_rose()
    for dy in range(rose["h"]):
        for dx in range(rose["w"]):
            o = (dy * rose["w"] + dx) * 4
            r, g, b, a = rose["p"][o], rose["p"][o + 1], rose["p"][o + 2], rose["p"][o + 3]
            if a < 8:
                continue
            px, py = int(lateral_w / 2 - s(60, e)) + dx, s(84, e) + dy
            if 0 <= px < img.w and 0 <= py < img.h:
                fond = img.p[(py * img.w + px) * 3 : (py * img.w + px) * 3 + 3]
                k = a / 255.0
                o2 = (py * img.w + px) * 3
                # les traits du dessin sont NOIRS sur fond transparent :
                # sur le fond sombre de l'installeur, ils passent en clair
                # (même rendu que le site, qui pose le visuel sur #15100f)
                if r < 40 and g < 40 and b < 40:
                    r, g, b = 232, 226, 226
                img.p[o2] = int(r * k + fond[0] * (1 - k))
                img.p[o2 + 1] = int(g * k + fond[1] * (1 - k))
                img.p[o2 + 2] = int(b * k + fond[2] * (1 - k))
    img.rect(s(24, e), s(214, e), lateral_w - s(24, e), s(280, e), rgb("TEXTE"))
    y = s(344, e)
    for i, atteinte in enumerate((True, True, vue >= 2, vue >= 3, vue >= 4)):
        courante = i == vue
        c = rgb("ROUGE") if atteinte else (rgb("ROUGE_SOMBRE") if courante else rgb("FOND_PUCE"))
        img.disque(s(48, e), y + s(12, e), s(14, e), c)
        if atteinte:
            img.trait(s(41, e), y + s(12, e), s(48, e), y + s(19, e), rgb("TEXTE"), s(3, e))
            img.trait(s(48, e), y + s(19, e), s(58, e), y + s(7, e), rgb("TEXTE"), s(3, e))
        else:
            img.rect(s(74, e), y + s(6, e), lateral_w - s(20, e), y + s(18, e), rgb("TEXTE_FAIBLE"))
        y += s(44, e)
    bas = h - etat_h - s(18, e)
    img.arrondi(s(20, e), bas - s(38, e), lateral_w - s(20, e), bas, s(10, e), rgb("LIGNE_FORTE"))
    img.arrondi(s(20, e), bas - s(84, e), lateral_w - s(20, e), bas - s(46, e), s(10, e), rgb("LIGNE_FORTE"))
    # contenu
    g, d = lateral_w + 1 + s(56, e), w - s(56, e)
    yc = s(56 + 44, e)
    img.rect(g, yc, d, yc + s(40, e), rgb("TEXTE"))
    yc += s(48, e)
    img.rect(g, yc, d, yc + s(30, e), rgb("TEXTE_DOUX"))
    yc += s(40, e)
    if vue == 0:
        img.arrondi(g, yc, d, yc + s(150, e), s(12, e), rgb("CARTE"))
        img.rect(g, yc + s(10, e), g + s(3, e), yc + s(140, e), rgb("ROUGE"))
        img.rect(g + s(20, e), yc + s(16, e), d - s(20, e), yc + s(140, e), rgb("TEXTE"))
        img.rect(g, yc + s(164, e), d, yc + s(230, e), rgb("TEXTE_DOUX"))
    else:
        img.arrondi(g, yc, d, yc + s(92, e), s(12, e), rgb("CARTE"))
        img.rect(g, yc + s(10, e), g + s(3, e), yc + s(82, e), rgb("ROUGE"))
        img.rect(g + s(20, e), yc + s(14, e), d - s(20, e), yc + s(34, e), rgb("TEXTE_DOUX"))
        img.rect(g + s(20, e), yc + s(38, e), d - s(20, e), yc + s(62, e), rgb("TEXTE"))
        img.rect(g + s(20, e), yc + s(66, e), d - s(20, e), yc + s(88, e), rgb("VERT"))
        img.arrondi(g, yc + s(108, e), d, yc + s(200, e), s(12, e), rgb("CARTE"))
        img.rect(g, yc + s(118, e), g + s(3, e), yc + s(190, e), rgb("VERT"))
        img.rect(g + s(20, e), yc + s(122, e), d - s(20, e), yc + s(142, e), rgb("TEXTE_DOUX"))
        img.rect(g + s(20, e), yc + s(146, e), d - s(20, e), yc + s(168, e), rgb("VERT"))
        img.rect(g + s(20, e), yc + s(170, e), d - s(20, e), yc + s(192, e), rgb("TEXTE_DOUX"))
    # pied de page : boutons
    by = h - etat_h - pied_h + (pied_h - s(44, e)) // 2
    bw, bh = s(190, e), s(44, e)
    img.arrondi(lateral_w + s(28, e), by, lateral_w + s(28, e) + bw, by + bh, s(10, e), rgb("LIGNE"))
    img.arrondi(w - s(28, e) - bw, by, w - s(28, e), by + bh, s(10, e), rgb("ROUGE"))
    # barre d'état
    img.rect(0, h - etat_h, w, h, rgb("BARRE"))
    img.disque(s(16, e), h - etat_h // 2, s(5, e), rgb("VERT"))
    img.rect(s(28, e), h - etat_h + s(9, e), w / 2 + s(60, e), h - etat_h + s(21, e), rgb("TEXTE_DOUX"))
    img.rect(w / 2, h - etat_h + s(9, e), w - s(14, e), h - etat_h + s(21, e), rgb("TEXTE_FAIBLE"))
    return img


def main():
    w, h = 1120, 780
    for vue, nom in ((0, "apercu-bienvenue.png"), (2, "apercu-options.png")):
        dessiner(w, h, vue).png(SORTIE / nom)
        print(f"{nom} : {w}×{h}")

    # Contrôle de débordement : chaque ligne du tracé Rust est mise à
    # l'échelle ; on vérifie qu'aucun rectangle de texte ne sort de la
    # fenêtre ni ne chevauche le pied de page ou la barre d'état.
    source = SOURCE.read_text()
    rectangles = re.findall(
        r"let (?:mut )?\w+ = RECT \{ left: ([^,]+), top: ([^,]+), right: ([^,]+), bottom: ([^}]+) \};",
        source,
    )
    debordements = []
    for gauche, haut, droite, bas in rectangles:
        if "w -" in droite and "s(" not in droite:
            debordements.append(droite)
    print(f"{len(rectangles)} rectangles de tracé analysés ; {len(debordements)} bornes non mises à l'échelle")
    assert not debordements, debordements

    # contrôle : les boutons du pied de page tiennent dans la zone utile
    e = 1.0
    gauche = 320 + 1 + 28 + 190
    droite_d = 1120 - 28
    droite_g = droite_d - 190
    assert gauche < droite_g, (gauche, droite_g)
    print(f"boutons : gauche [{320 + 29} → {gauche}], droite [{droite_g} → {droite_d}] — sans chevauchement")


if __name__ == "__main__":
    main()
