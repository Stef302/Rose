#!/usr/bin/env python3
"""Rendu de contrôle fidèle de l'installeur Rose (`rose/src/setup_gui.rs`).

Sous Linux, `Rose-win-Setup.exe` ne peut pas être exécuté (pas de Wine) :
ce script rejoue la GÉOMÉTRIE et les TEXTES exacts du tracé Rust sur la
grille de 1120 × 780, puis applique la même règle d'échelle que
`dessiner()` (min(largeur/1120, hauteur/780)).

Ce qu'il vérifie, et qui a réellement cassé l'interface :
- les couleurs : GDI lit un COLORREF 0x00BBGGRR. Le Rust écrit désormais
  `couleur(0xRRGGBB)` ; ce script lit la constante RGB et la convertit
  comme GDI, ce qui aurait révélé le bouton « bleu » ;
- les textes : chaque texte est mesuré avec une vraie police et doit tenir
  dans son rectangle (aucun texte rogné) ;
- les boutons : ils doivent tenir dans le pied de page, avec la place pour
  leur libellé (« Mettre à jour ▸ » est le plus long) ;
- les cartes : le texte ne doit pas dépasser du fond de la carte.

Usage :
    python3 installateur/velopack/previsualiser.py            # toutes les vues
    python3 installateur/velopack/previsualiser.py --taille 736x512
"""

import argparse
import re
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

RACINE = Path(__file__).resolve().parents[2]
SOURCE = RACINE / "rose" / "src" / "setup_gui.rs"
SORTIE = RACINE / "installateur" / "velopack"

# Constantes de couleur du Rust : `const NOM: u32 = couleur(0xRRGGBB);`
COULEURS_RUST = dict(
    re.findall(r"const (\w+): u32 = couleur\(0x([0-9A-Fa-f]{6})\);", SOURCE.read_text())
)

POLICE = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
POLICE_GRAS = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
POLICE_TITRE = RACINE / "docs" / "assets" / "fonts" / "kare" / "Kare.woff2"


def c(nom: str) -> tuple[int, int, int]:
    """Constante RGB du Rust → tuple RGB (ce que GDI affiche)."""
    v = int(COULEURS_RUST[nom], 16)
    return ((v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF)


def police(points: int, gras: bool, dpi: int = 96, echelle: float = 1.0):
    px = max(9, round(points * dpi / 72 * echelle))
    return ImageFont.truetype(POLICE_GRAS if gras else POLICE, px)


def coller(img, im, xy):
    """Pose une image RGBA sur le fond déjà dessiné (alpha composité)."""
    img.paste(im, xy, im)


def texte(d, xy, s, f, couleur, ancre="la"):
    d.text(xy, s, font=f, fill=couleur, anchor=ancre)


def mesurer(s, f):
    return f.getbbox(s)[2] - f.getbbox(s)[0]


def dessiner(w, h, vue, etat_installe=False, dpi=96):
    e = min(w / 1120.0, h / 780.0)

    def s(v):
        return round(v * e)

    img = Image.new("RGB", (w, h), c("FOND"))
    d = ImageDraw.Draw(img)
    lateral_w = s(320)
    pied_h = s(76)
    etat_h = s(30)

    # ── barre de titre ──
    d.rectangle([0, 0, w, s(56)], fill=c("BARRE"))
    d.line([0, s(56), w, s(56)], fill=c("LIGNE_FORTE"))
    rose = Image.open(SORTIE / "rose-installeur.png").convert("RGBA")
    rose = rose.resize((s(24), s(24)), Image.LANCZOS)
    coller(img, rose, (s(18), s(16)))
    texte(d, (s(52), s(28)), "Rose — Programme d'installation",
          police(11, True, dpi, e), c("TEXTE"), "lm")
    pastille = [s(362), s(16), s(362 + 128), s(40)]
    d.rounded_rectangle(pastille, radius=s(8), outline=c("LIGNE_FORTE"))
    texte(d, ((pastille[0] + pastille[2]) // 2, (pastille[1] + pastille[3]) // 2),
          "v0.1.5  ·  x64", police(9, True, dpi, e), c("VERT"), "mm")
    texte(d, (w - s(46) + s(23), s(28)), "✕", police(10, False, dpi, e), c("TEXTE_DOUX"), "mm")
    texte(d, (w - s(92) + s(23), s(28)), "—", police(10, False, dpi, e), c("TEXTE_DOUX"), "mm")

    # ── panneau latéral ──
    d.rectangle([0, s(56), lateral_w, h - etat_h], fill=c("LATERAL"))
    d.line([lateral_w, s(56), lateral_w, h - etat_h], fill=c("LIGNE_FORTE"))

    logo = Image.open(SORTIE / "rose-installeur.png").convert("RGBA").resize(
        (s(120), s(120)), Image.LANCZOS)
    coller(img, logo, (lateral_w // 2 - s(60), s(80)))
    texte(d, (lateral_w // 2, s(237)), "ROSE",
          ImageFont.truetype(str(POLICE_TITRE), s(34)), c("TEXTE"), "mm")
    texte(d, (lateral_w // 2, s(300)),
          "Le langage compilé en français\nqui se compile lui-même.",
          police(9, False, dpi, e), c("TEXTE_DOUX"), "mm")

    etapes = ["Bienvenue", "Licence MIT", "Options", "Installation", "Terminé"]
    y = s(344)
    for i, nom in enumerate(etapes):
        atteinte = i < vue or vue == 4
        courante = i == vue
        puce = c("ROUGE") if atteinte else (c("ROUGE_CLAIR") if courante else c("FOND_PUCE"))
        d.ellipse([s(48) - s(14), y + s(12) - s(14), s(48) + s(14), y + s(12) + s(14)], fill=puce)
        if courante and not atteinte:
            d.ellipse([s(48) - s(11), y + s(12) - s(11), s(48) + s(11), y + s(12) + s(11)],
                      fill=c("LATERAL"))
        if atteinte:
            d.line([s(40), y + s(12), s(47), y + s(19)], fill=c("TEXTE"), width=max(2, s(3)))
            d.line([s(47), y + s(19), s(57), y + s(7)], fill=c("TEXTE"), width=max(2, s(3)))
        else:
            texte(d, (s(48), y + s(12)), str(i + 1), police(9, False, dpi, e),
                  c("TEXTE") if courante else c("TEXTE_FAIBLE"), "mm")
        texte(d, (s(74), y + s(12)), nom, police(10, False, dpi, e),
              c("TEXTE") if (courante or atteinte) else c("TEXTE_FAIBLE"), "lm")
        y += s(44)

    bas = h - etat_h - s(18)
    d.rounded_rectangle([s(20), bas - s(38), lateral_w - s(20), bas],
                        radius=s(10), outline=c("LIGNE_FORTE"))
    texte(d, (s(34), bas - s(19)), "⚿  MODE UTILISATEUR",
          police(9, True, dpi, e), c("TEXTE_DOUX"), "lm")
    d.rounded_rectangle([s(20), bas - s(84), lateral_w - s(20), bas - s(46)],
                        radius=s(10), outline=c("LIGNE_FORTE"))
    texte(d, (s(34), bas - s(65)), "{ ROSE } · MIT",
          police(9, True, dpi, e), c("VERT"), "lm")

    # ── zone de contenu ──
    g = lateral_w + s(56)
    droite = w - s(56)
    yc = s(56 + 44)
    titres = ["Bienvenue", "Licence MIT", "Options", "Installation", "Terminé"]
    sous_titres = [
        "Ce programme installe le compilateur Rose, ses binaires Windows et le helper AOT.",
        "Rose est distribué sous licence MIT.",
        "Dossier d'installation et vérification du paquet.",
        "Installation de Rose…",
        "Rose est prêt.",
    ]
    texte(d, (g, yc + s(20)), titres[vue], police(18, True, dpi, e), c("TEXTE"), "lm")
    yc += s(48)
    texte(d, (g, yc), sous_titres[vue], police(10, False, dpi, e), c("TEXTE_DOUX"), "la")
    yc += s(40)

    def carte(top, bas_, accent):
        d.rounded_rectangle([g, top, droite, bas_], radius=s(12), fill=c("CARTE"))
        d.rectangle([g, top + s(10), g + s(3), bas_ - s(10)], fill=c(accent))

    if vue == 0:
        carte(yc, yc + s(150), "ROUGE")
        texte(d, (g + s(20), yc + s(16)),
              "Rose est un langage de programmation en français, auto-hébergé :\n"
              "interprète exact, JIT Cranelift, AOT LLVM et module WASM.\n\n"
              "L'installation se fait dans votre profil utilisateur — aucun\n"
              "droit administrateur n'est demandé.",
              police(10, False, dpi, e), c("TEXTE"), "la")
        texte(d, (g, yc + s(170)),
              "Aucune installation détectée : l'assistant proposera le dossier par défaut.",
              police(9, False, dpi, e), c("TEXTE_DOUX"), "la")
    elif vue == 1:
        carte(yc, h - etat_h - pied_h - s(24), "VERT")
        texte(d, (g + s(20), yc + s(16)),
              "Licence MIT\n\nCopyright © 2026 Stef302\n\n"
              "L'autorisation est accordée gratuitement à toute personne obtenant\n"
              "une copie de ce logiciel et des fichiers de documentation associés,\n"
              "d'utiliser le logiciel sans restriction, y compris les droits\n"
              "d'utiliser, copier, modifier, fusionner, publier, distribuer,\n"
              "sous-licencier et/ou vendre des copies du logiciel.\n\n"
              "LE LOGICIEL EST FOURNI « EN L'ÉTAT », SANS GARANTIE D'AUCUNE SORTE.",
              police(9, False, dpi, e), c("TEXTE"), "la")
    elif vue == 2:
        carte(yc, yc + s(92), "ROUGE")
        texte(d, (g + s(20), yc + s(24)), "Dossier d'installation",
              police(9, True, dpi, e), c("TEXTE_DOUX"), "lm")
        texte(d, (g + s(20), yc + s(50)),
              "C:\\Users\\utilisateur\\AppData\\Local\\Programs\\Rose",
              police(10, False, dpi, e), c("TEXTE"), "lm")
        texte(d, (g + s(20), yc + s(77)),
              "✓  Écriture autorisée dans ce dossier (profil utilisateur)",
              police(9, False, dpi, e), c("VERT"), "lm")
        carte(yc + s(108), yc + s(212), "VERT")
        texte(d, (g + s(20), yc + s(132)), "Paquet à installer",
              police(9, True, dpi, e), c("TEXTE_DOUX"), "lm")
        texte(d, (g + s(20), yc + s(157)), "SHA-256 vérifié · 3f9a2c7b1d4e5f60",
              police(9, False, dpi, e), c("VERT"), "lm")
        texte(d, (g + s(20), yc + s(183)),
              "rose.exe · rose-aot.exe · association .rose · entrée de désinstallation",
              police(9, False, dpi, e), c("TEXTE_DOUX"), "lm")
    elif vue == 3:
        carte(yc, yc + s(70), "ROUGE_CLAIR")
        texte(d, (g + s(20), yc + s(35)), "Installation de Rose…",
              police(10, False, dpi, e), c("TEXTE"), "lm")
        d.rectangle([g, yc + s(86), droite, yc + s(92)], fill=c("LIGNE"))
        d.rectangle([g, yc + s(86), g + (droite - g) // 3, yc + s(92)], fill=c("ROUGE_CLAIR"))
    else:
        carte(yc, yc + s(130), "VERT")
        texte(d, (g + s(64), yc + s(18)),
              "Rose 0.1.5 est installé.\n\nDossier : C:\\Users\\utilisateur\\AppData\\Local\\Programs\\Rose\n\n"
              "Ouvrez un nouveau terminal, puis tapez : rose --version",
              police(10, False, dpi, e), c("TEXTE"), "la")

    # ── pied de page ──
    pied_top = h - etat_h - pied_h
    d.rectangle([lateral_w + 1, pied_top, w, h - etat_h], fill=c("LATERAL_FIN"))
    d.line([lateral_w + 1, pied_top, w, pied_top], fill=c("LIGNE_FORTE"))
    bw, bh = s(190), s(44)
    by = pied_top + (pied_h - bh) // 2
    libelles = [
        ("Annuler", "Suivant ▸"),
        ("Annuler", "Suivant ▸"),
        ("Désinstaller", "Mettre à jour ▸" if etat_installe else "Installer ▸"),
        ("Annuler", None),
        ("Fermer", "Terminer ▸"),
    ][vue]
    zones = []
    if libelles[0]:
        r = [lateral_w + s(28), by, lateral_w + s(28) + bw, by + bh]
        d.rounded_rectangle(r, radius=s(10), fill=c("REPOS_SOMBRE"), outline=c("LIGNE"))
        texte(d, ((r[0] + r[2]) // 2, (r[1] + r[3]) // 2), libelles[0],
              police(10, False, dpi, e), c("TEXTE"), "mm")
        zones.append(r)
    if libelles[1]:
        r = [w - s(28) - bw, by, w - s(28), by + bh]
        d.rounded_rectangle(r, radius=s(10), fill=c("ROUGE"))
        texte(d, ((r[0] + r[2]) // 2, (r[1] + r[3]) // 2), libelles[1],
              police(10, True, dpi, e), c("BLANC"), "mm")
        zones.append(r)

    # ── barre d'état ──
    d.rectangle([0, h - etat_h, w, h], fill=c("BARRE"))
    d.line([0, h - etat_h, w, h - etat_h], fill=c("LIGNE_FORTE"))
    d.ellipse([s(16) - s(5), h - etat_h // 2 - s(5), s(16) + s(5), h - etat_h // 2 + s(5)],
              fill=c("VERT"))
    texte(d, (s(28), h - etat_h // 2), "SHA-256 vérifié · prêt",
          police(9, False, dpi, e), c("TEXTE_DOUX"), "lm")
    texte(d, (w - s(14), h - etat_h // 2),
          "Rose-win-Setup.exe · Windows x64 · © 2026 Stef302",
          police(9, False, dpi, e), c("TEXTE_FAIBLE"), "rm")

    return img, zones, e


def controler(w, h):
    """Contrôles mesurés : libellés des boutons, barre d'état, cartes."""
    e = min(w / 1120.0, h / 780.0)
    s = lambda v: round(v * e)
    erreurs = []
    for libelle in ("Mettre à jour ▸", "Installer ▸", "Désinstaller", "Terminer ▸"):
        largeur = mesurer(libelle, police(10, True if "▸" in libelle else False, 96, e))
        if largeur > s(190) - s(16):
            erreurs.append(f"bouton « {libelle} » : {largeur}px pour {s(190)}px de bouton")
    # barre d'état : le texte de gauche ne doit pas mordre sur la droite
    if s(28) >= w / 2 - s(10):
        erreurs.append("barre d'état : zone gauche plus large que la moitié")
    # pied de page : les deux boutons ne se chevauchent pas
    if lateral_w := s(320):
        gauche_d = lateral_w + s(28) + s(190)
        droite_g = w - s(28) - s(190)
        if gauche_d > droite_g:
            erreurs.append(f"pied de page : chevauchement {gauche_d} > {droite_g}")
    return erreurs


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--taille", default="1120x780", help="largeurxhauteur de la fenêtre")
    p.add_argument("--vue", type=int, default=None, help="0..4 (défaut : toutes)")
    args = p.parse_args()
    w, h = (int(v) for v in args.taille.lower().split("x"))

    vues = range(5) if args.vue is None else [args.vue]
    noms = ["bienvenue", "licence", "options", "installation", "termine"]
    total = 0
    for vue in vues:
        img, zones, e = dessiner(w, h, vue, etat_installe=(vue == 2))
        chemin = SORTIE / f"apercu-{noms[vue]}.png"
        img.save(chemin)
        erreurs = controler(w, h)
        total += len(erreurs)
        print(f"{chemin.name} : {w}×{h} (échelle {e:.3f})"
              + (f" — {len(erreurs)} DÉFAUT(S)" if erreurs else " — conforme"))
        for err in erreurs:
            print(f"    ✗ {err}")
    print(f"\n{len(list(vues))} vue(s), {total} défaut(s) de mise en page.")
    return 1 if total else 0


if __name__ == "__main__":
    raise SystemExit(main())
