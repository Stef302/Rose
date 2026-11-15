#!/usr/bin/env python3
"""Convertit un SVG en icône Windows `.ico` multi-tailles.

Windows choisit l'image adaptée au contexte : 16 px dans la barre de titre et
l'Explorateur (vue « Détails »), 32 px dans la barre des tâches, 48 px sur le
bureau, 256 px en vue « Très grandes icônes ». Une icône qui ne contient
qu'une taille est donc étirée par Windows — d'où un rendu flou.

Deux pièges mesurés :

1. **`currentColor` n'est PAS résolu par CairoSVG.** Les deux SVG de la marque
   l'utilisent pour le vert lime (`color="#8CE026"` sur l'élément racine).
   Sans substitution, tous les traits sortent NOIRS (ou absents) : l'icône
   paraît cassée. On inline la couleur AVANT le rendu.
2. **Le rendu direct à 16 px est illisible.** On rend à haute résolution
   (1024 px) puis on RÉDUIT avec LANCZOS : c'est le seul moyen d'obtenir des
   traits propres à toutes les tailles, y compris 16 px.

⚠ Les entrées `RT_ICON` d'un PE doivent contenir le DIB BRUT, pas un
conteneur `.ico` : c'est `pack-res.py` qui découpe l'ICO. Ce script produit un
`.ico` STANDARD (pour l'Explorateur et l'installeur), pas une ressource PE.

Dépendances : cairosvg (pip), Pillow.
Usage :
    python3 installateur/velopack/svg-vers-ico.py entree.svg sortie.ico
"""

import io
import struct
import sys
from pathlib import Path

import cairosvg
from PIL import Image

# Tailles standard d'une icône Windows, de la plus petite à la plus grande.
# 256 est stocké en PNG COMPRESSÉ dans l'ICO (convention Vista+).
TAILLES = [16, 24, 32, 48, 64, 128, 256]
# Résolution de rendu : 4× la plus grande taille, pour une réduction nette.
RENDU = 1024


def rendre(svg: Path, taille: int) -> Image.Image:
    """Rend le SVG à `taille` px, en résolvant `currentColor`."""
    src = svg.read_text(encoding="utf-8")
    # Couleur de repli : celle de l'élément racine (`color="…"`), sinon le
    # vert lime de la marque Rose.
    import re
    m = re.search(r'<svg[^>]*\bcolor="(#[0-9A-Fa-f]{3,8})"', src)
    couleur = m.group(1) if m else "#8CE026"
    src = src.replace("currentColor", couleur)
    png = cairosvg.svg2png(bytestring=src.encode("utf-8"),
                           output_width=taille, output_height=taille)
    return Image.open(io.BytesIO(png)).convert("RGBA")


def dib(im: Image.Image) -> bytes:
    """Image RGBA → DIB 32 bits (BITMAPINFOHEADER + pixels BGRA + masque).

    ⚠ C'est le format qu'exige `RT_ICON` et, dans un `.ico`, celui des tailles
    ≤ 128 px. Le masque AND de 1 bit est OBLIGATOIRE : sans lui, Windows lit
    les pixels comme un masque et l'icône sort en noir ou transparente.
    """
    t = im.size[0]
    pixels = im.load()
    # BITMAPINFOHEADER : 40 octets. `biHeight` = 2 × hauteur (image + masque).
    entete = struct.pack("<IiiHHIIiiII",
                         40,          # biSize
                         t,           # biWidth
                         t * 2,       # biHeight (couleur + masque)
                         1,           # biPlanes
                         32,          # biBitCount
                         0,           # biCompression = BI_RGB
                         t * t * 4,   # biSizeImage
                         0, 0, 0, 0)
    corps = bytearray()
    # Les lignes sont stockées de BAS en HAUT, en BGRA.
    for y in range(t - 1, -1, -1):
        for x in range(t):
            r, v, b, a = pixels[x, y]
            corps += bytes((b, v, r, a))
    # Masque AND : 1 bit par pixel, aligné sur 4 octets par ligne.
    ligne_masque = ((t + 31) // 32) * 4
    masque = bytearray()
    for y in range(t - 1, -1, -1):
        ligne = bytearray(ligne_masque)
        for x in range(t):
            if pixels[x, y][3] == 0:
                ligne[x // 8] |= 0x80 >> (x % 8)
        masque += ligne
    return entete + bytes(corps) + bytes(masque)


def ecrire_ico(ico: Path, images: list[Image.Image]) -> None:
    """Écrit un `.ico` : DIB brut pour ≤ 128 px, PNG pour 256 px."""
    entrees = []
    donnees = bytearray()
    decalage = 6 + 16 * len(images)
    for im in images:
        t = im.size[0]
        if t >= 256:
            buf = io.BytesIO()
            im.save(buf, format="PNG")
            blob = buf.getvalue()
        else:
            blob = dib(im)
        entrees.append(struct.pack("<BBBBHHII",
                                   t % 256,      # largeur (0 = 256)
                                   t % 256,      # hauteur (0 = 256)
                                   0,            # couleurs (0 = vrai couleur)
                                   0,            # réservé
                                   1,            # plans
                                   32,           # bits par pixel
                                   len(blob),
                                   decalage))
        donnees += blob
        decalage += len(blob)
    ico.write_bytes(struct.pack("<HHH", 0, 1, len(images)) + b"".join(entrees) + bytes(donnees))


def convertir(svg: Path, ico: Path) -> None:
    if not svg.is_file():
        raise SystemExit(f"SVG introuvable : {svg}")
    # Rendu de référence, puis réduction pour chaque taille.
    base = rendre(svg, RENDU)
    images = []
    for t in TAILLES:
        im = base.resize((t, t), Image.LANCZOS)
        # Aucune icône ne doit être entièrement transparente : ce serait un
        # SVG dont le rendu a échoué.
        if im.getbbox() is None:
            raise SystemExit(f"rendu vide à {t} px : le SVG n'a rien produit")
        images.append(im)
    # ⚠⚠ NE PAS laisser Pillow choisir le format interne. `save(format="ICO")`
    # écrit TOUTES les tailles en PNG COMPRESSÉ — ce que Windows n'accepte
    # que pour 256 px. Les tailles 16 à 128 doivent être des DIB BRUTS (BMP),
    # sinon l'Explorateur et la barre de titre affichent l'icône générique.
    # On écrit donc le conteneur ICO à la main : BMP pour ≤ 128, PNG pour 256.
    ecrire_ico(ico, images)
    print(f"{ico.name} : {len(TAILLES)} tailles ({', '.join(f'{t}px' for t in TAILLES)})")
    for t in TAILLES:
        im = base.resize((t, t), Image.LANCZOS)
        opaques = sum(1 for p in im.convert("RGBA").split()[3].tobytes()[3::4] if p > 8)
        print(f"  {t:>3}px : {opaques:>6} pixels opaques")


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    convertir(Path(sys.argv[1]), Path(sys.argv[2]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
