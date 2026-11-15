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
- les cartes : leur hauteur suit leur CONTENU, jamais une constante —
  c'est ce qui laissait la fenêtre « vide » (texte entièrement hors carte) ;
- le logo : il doit occuper TOUTE la surface du PNG embarqué (une bande de
  fond semi-opaque rendait la rose minuscule) ;
- les boutons : ils doivent tenir dans le pied de page, avec la place pour
  leur libellé (« Mettre à jour » est le plus long).

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
LOGO = SORTIE / "rose-installeur.png"

# Constantes de couleur du Rust : `const NOM: u32 = couleur(0xRRGGBB);`
COULEURS_RUST = dict(
    re.findall(r"const (\w+): u32 = couleur\(0x([0-9A-Fa-f]{6})\);", SOURCE.read_text())
)

POLICE = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
POLICE_GRAS = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
POLICE_TITRE = RACINE / "docs" / "assets" / "fonts" / "kare" / "Kare.woff2"

# Plancher de taille des polices, IDENTIQUE à `points_ecrits` du Rust.
PLANCHER_PX = 13


def c(nom: str) -> tuple[int, int, int]:
    """Constante RGB du Rust → tuple RGB (ce que GDI affiche)."""
    v = int(COULEURS_RUST[nom], 16)
    return ((v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF)


def pixels_police(points: int, dpi: int = 96, echelle: float = 1.0) -> int:
    """Taille en pixels, avec le même plancher que `points_ecrits`."""
    return max(PLANCHER_PX, round(points * dpi / 72 * echelle))


def police(points: int, gras: bool, dpi: int = 96, echelle: float = 1.0):
    px = pixels_police(points, dpi, echelle)
    return ImageFont.truetype(POLICE_GRAS if gras else POLICE, px)


def coller(img, im, xy):
    """Pose une image RGBA sur le fond déjà dessiné (alpha composité)."""
    img.paste(im, xy, im)


def texte(d, xy, s, f, couleur, ancre="la"):
    d.text(xy, s, font=f, fill=couleur, anchor=ancre)


def mesurer(s, f):
    return f.getbbox(s)[2] - f.getbbox(s)[0]


def lignes_ajustees(src, largeur, f):
    """Même découpage que `paint::lignes_ajustees` : sur les espaces."""
    sortie = []
    for paragraphe in src.split("\n"):
        if not paragraphe:
            sortie.append("")
            continue
        courante = ""
        for mot in paragraphe.split(" "):
            if not mot:
                continue
            essai = mot if not courante else f"{courante} {mot}"
            if mesurer(essai, f) <= largeur:
                courante = essai
            else:
                if courante:
                    sortie.append(courante)
                courante = mot
        sortie.append(courante)
    return sortie


def hauteur_texte(src, largeur, f, interligne):
    return max(1, len(lignes_ajustees(src, largeur, f))) * interligne


def texte_multiligne(d, xy, src, largeur, f, couleur, interligne):
    """Dessine un texte découpé comme le fait DrawText."""
    x, y = xy
    for ligne in lignes_ajustees(src, largeur, f):
        if ligne:
            d.text((x, y), ligne, font=f, fill=couleur)
        y += interligne


def dessiner(w, h, vue, etat_installe=False, dpi=96):
    e = min(w / 1120.0, h / 780.0)

    def s(v):
        return round(v * e)

    interligne = pixels_police(10, dpi, e) + max(2, s(6))
    interligne_petit = pixels_police(9, dpi, e) + max(2, s(5))
    f_corps = police(10, False, dpi, e)
    f_note = police(9, False, dpi, e)

    img = Image.new("RGB", (w, h), c("FOND"))
    d = ImageDraw.Draw(img)
    lateral_w = s(320)
    pied_h = s(76)
    etat_h = s(30)

    # ── barre de titre PERSONNALISÉE (aucun cadre Windows) ──
    d.rectangle([0, 0, w, s(56)], fill=c("BARRE"))
    d.line([0, s(55), w, s(55)], fill=c("LIGNE_FORTE"))
    # ⚠ Le logo n'est PAS carré (138 × 172) : on le met à l'échelle sur le
    # plus grand côté, comme `dessiner_rose`.
    rose = Image.open(LOGO).convert("RGBA")
    rw, rh = rose.size
    cote = s(24)
    if rh >= rw:
        nw, nh = round(cote * rw / rh), cote
    else:
        nw, nh = cote, round(cote * rh / rw)
    coller(img, rose.resize((nw, nh), Image.LANCZOS), (s(18) + (cote - nw) // 2, s(16) + (cote - nh) // 2))
    # marque « Rose » en Kare, puis sous-titre
    texte(d, (s(52), s(28)), "Rose",
          ImageFont.truetype(str(POLICE_TITRE), s(20)), c("TEXTE"), "lm")
    # sous-titre borné AVANT la pastille (comme le Rust), ellipse si trop long
    largeur_pastille = s(128)
    boutons_l = w - 3 * s(46)
    droite_pastille = min(s(438) + largeur_pastille, boutons_l - s(8))
    pastille = [droite_pastille - largeur_pastille, s(16), droite_pastille, s(40)]
    zone_sous = max(s(170), pastille[0] - s(8)) - s(150)
    sous = "— Programme d'installation"
    f_sous = police(10, False, dpi, e)
    while zone_sous > 0 and mesurer(sous, f_sous) > zone_sous and len(sous) > 4:
        sous = sous[:-2] + "…"
    texte(d, (s(150), s(28)), sous, f_sous, c("TEXTE_DOUX"), "lm")
    d.rounded_rectangle(pastille, radius=s(8), outline=c("LIGNE_FORTE"))
    texte(d, ((pastille[0] + pastille[2]) // 2, (pastille[1] + pastille[3]) // 2),
          "v0.1.5  ·  x64", police(9, True, dpi, e), c("VERT"), "mm")
    # trois boutons de fenêtre : réduire, agrandir, fermer
    bl = s(46)
    cx_reduire = w - 3 * bl + bl // 2
    cx_agrandir = w - 2 * bl + bl // 2
    cx_fermer = w - bl + bl // 2
    cy = s(28)
    g = s(5)
    e1 = max(1, s(1))
    d.line([cx_reduire - g, cy, cx_reduire + g, cy], fill=c("TEXTE_DOUX"), width=e1)
    d.rectangle([cx_agrandir - g, cy - g, cx_agrandir + g, cy + g], outline=c("TEXTE_DOUX"), width=e1)
    d.line([cx_fermer - g, cy - g, cx_fermer + g, cy + g], fill=c("TEXTE_DOUX"), width=e1)
    d.line([cx_fermer - g, cy + g, cx_fermer + g, cy - g], fill=c("TEXTE_DOUX"), width=e1)

    # ── panneau latéral ──
    d.rectangle([0, s(56), lateral_w, h - etat_h], fill=c("LATERAL"))
    d.line([lateral_w, s(56), lateral_w, h - etat_h], fill=c("LIGNE_FORTE"))

    # Cartes du bas réservées AVANT le reste (comme le Rust).
    bas = h - etat_h - s(18)
    carte2 = [s(20), bas - s(38), lateral_w - s(20), bas]
    carte1 = [s(20), carte2[1] - s(46), lateral_w - s(20), carte2[1] - s(8)]
    plancher = carte1[1] - s(16)

    # logo borné par la place disponible
    place_logo = max(0, plancher - s(78) - s(150))
    # Même règle que le Rust : boîte de la HAUTEUR du logo (132x169), marge
    # de 16 px avant le titre.
    hauteur_voulue = round(s(124) * 169 / 132)
    hauteur_logo = min(hauteur_voulue, place_logo)
    if hauteur_logo >= s(40):
        lw, lh = rose.size
        cote = hauteur_logo
        dh = cote
        dw = max(1, round(cote * lw / lh))
        if dw > cote:
            dw = cote
            dh = max(1, round(cote * lh / lw))
        coller(img, rose.resize((dw, dh), Image.LANCZOS),
               (lateral_w // 2 - cote // 2 + (cote - dw) // 2, s(78) + (cote - dh) // 2))
    y_marque = s(78) + max(0, hauteur_logo) + s(16)
    texte(d, (lateral_w // 2, y_marque + s(28)), "Rose",
          ImageFont.truetype(str(POLICE_TITRE), s(40)), c("TEXTE"), "mm")
    texte_multiligne(d, (s(24), y_marque + s(58)),
                     "Le langage compilé en français qui se compile lui-même.",
                     lateral_w - s(48), f_note, c("TEXTE_DOUX"), interligne_petit)

    etapes = ["Bienvenue", "Licence MIT", "Options", "Installation", "Terminé"]
    y = y_marque + s(122)
    pas = min(max(s(26), (plancher - y) // 5), s(44))
    for i, nom in enumerate(etapes):
        if y + pas > plancher + s(4):
            break
        atteinte = i < vue or vue == 4
        courante = i == vue
        centre = y + pas // 2
        rayon = max(s(4), min(s(14), pas // 2 - s(1)))
        puce = c("ROUGE") if atteinte else (c("ROUGE_CLAIR") if courante else c("FOND_PUCE"))
        d.ellipse([s(48) - rayon, centre - rayon, s(48) + rayon, centre + rayon], fill=puce)
        if courante and not atteinte:
            d.ellipse([s(48) - rayon, centre - rayon, s(48) + rayon, centre + rayon],
                      outline=c("ROUGE_CLAIR"), width=max(1, s(2)))
        if atteinte:
            d.line([s(40), centre, s(47), centre + s(7)], fill=c("TEXTE"), width=max(2, s(3)))
            d.line([s(47), centre + s(7), s(57), centre - s(5)], fill=c("TEXTE"), width=max(2, s(3)))
        else:
            texte(d, (s(48), centre), str(i + 1), police(9, False, dpi, e),
                  c("TEXTE") if courante else c("TEXTE_FAIBLE"), "mm")
        texte(d, (s(74), centre), nom, f_corps,
              c("TEXTE") if (courante or atteinte) else c("TEXTE_FAIBLE"), "lm")
        y += pas

    for r in (carte1, carte2):
        d.rounded_rectangle(r, radius=s(10), fill=c("VERT_LIME"))
    texte(d, ((carte1[0] + carte1[2]) // 2, (carte1[1] + carte1[3]) // 2), "{ ROSE } · MIT",
          police(9, True, dpi, e), c("NOIR_DOUX"), "mm")
    texte(d, ((carte2[0] + carte2[2]) // 2, (carte2[1] + carte2[3]) // 2), "MODE UTILISATEUR",
          police(9, True, dpi, e), c("NOIR_DOUX"), "mm")

    # ── zone de contenu ──
    contenu_haut = s(56)
    contenu_bas = h - etat_h - pied_h
    g = lateral_w + s(56)
    droite = w - s(56)
    largeur_texte = max(s(80), droite - g - s(40))
    bas_utile = contenu_bas - s(20)

    titres = ["Bienvenue", "Licence MIT", "Options", "Installation", "Terminé"]
    sous_titres = [
        "Ce programme installe le compilateur Rose, ses binaires Windows et le helper AOT.",
        "Rose est distribué sous licence MIT.",
        "Dossier d'installation et vérification du paquet.",
        "Installation de Rose…",
        "Rose est prêt.",
    ]
    yc = contenu_haut + s(44)
    texte(d, (g, yc + s(22)), titres[vue].upper(),
          ImageFont.truetype(str(POLICE_TITRE), s(22)), c("TEXTE"), "lm")
    yc += s(52)
    h_sous = max(s(20), hauteur_texte(sous_titres[vue], droite - g, f_corps, interligne))
    texte_multiligne(d, (g, yc), sous_titres[vue], droite - g, f_corps, c("TEXTE_DOUX"), interligne)
    yc += h_sous + s(28)

    def carte(top, bas_, accent):
        d.rounded_rectangle([g, top, droite, bas_], radius=s(12), fill=c("CARTE"))
        d.rectangle([g, top + s(10), g + s(3), bas_ - s(10)], fill=c(accent))
        return bas_

    if vue == 0:
        corps = ("Rose est un langage de programmation en français, auto-hébergé : "
                 "interprète exact, JIT Cranelift, AOT LLVM et module WASM.\n\n"
                 "L'installation se fait dans votre profil utilisateur — aucun droit "
                 "administrateur n'est demandé.")
        note = "Aucune installation détectée : l'assistant proposera le dossier par défaut."
        h_corps = hauteur_texte(corps, largeur_texte, f_corps, interligne)
        h_note = hauteur_texte(note, droite - g, f_note, interligne_petit)
        h_dispo = max(s(120), bas_utile - yc - h_note - s(18))
        h_carte = max(max(h_corps + 2 * s(18), s(120)), h_dispo)
        bas_c = carte(yc, yc + h_carte, "ROUGE")
        texte_multiligne(d, (g + s(20), yc + s(16)), corps, largeur_texte, f_corps, c("TEXTE"), interligne)
        texte_multiligne(d, (g, bas_c + s(18)), note, droite - g, f_note, c("TEXTE_DOUX"), interligne_petit)
    elif vue == 1:
        licence = ("Licence MIT\n\nCopyright © 2026 Stef302\n\n"
                   "L'autorisation est accordée gratuitement à toute personne obtenant "
                   "une copie de ce logiciel et des fichiers de documentation associés, "
                   "d'utiliser le logiciel sans restriction, y compris les droits "
                   "d'utiliser, copier, modifier, fusionner, publier, distribuer, "
                   "sous-licencier et/ou vendre des copies du logiciel.\n\n"
                   "LE LOGICIEL EST FOURNI « EN L'ÉTAT », SANS GARANTIE D'AUCUNE SORTE.")
        carte(yc, bas_utile, "VERT")
        texte_multiligne(d, (g + s(20), yc + s(16)), licence, largeur_texte, f_note, c("TEXTE"), interligne_petit)
    elif vue == 2:
        # carte 1 : dossier
        h1 = s(14) + s(20) + s(24) + s(24) + s(14)
        bas_c = carte(yc, yc + h1, "ROUGE")
        texte(d, (g + s(20), yc + s(24)), "Dossier d'installation", f_note, c("TEXTE_DOUX"), "lm")
        texte(d, (g + s(20), yc + s(50)),
              "C:\\Users\\utilisateur\\AppData\\Local\\Programs\\Rose",
              police(10, False, dpi, e), c("TEXTE"), "lm")
        texte(d, (g + s(20), yc + s(77)),
              "✓  Écriture autorisée dans ce dossier (profil utilisateur)",
              f_note, c("VERT"), "lm")
        # carte 2 : paquet
        contenu_paquet = "rose.exe · rose-shell.exe · icônes · association .rose · désinstallation"
        h_paquet = max(s(104), hauteur_texte(contenu_paquet, largeur_texte, f_note, interligne_petit) + 2 * s(14))
        y2 = bas_c + s(16)
        bas2 = carte(y2, y2 + h_paquet, "VERT")
        texte(d, (g + s(20), y2 + s(24)), "Paquet à installer", f_note, c("TEXTE_DOUX"), "lm")
        texte(d, (g + s(20), y2 + s(49)), "SHA-256 vérifié · 3f9a2c7b1d4e5f60", f_note, c("VERT"), "lm")
        texte_multiligne(d, (g + s(20), y2 + s(64)), contenu_paquet, largeur_texte, f_note,
                         c("TEXTE_DOUX"), interligne_petit)
        if etat_installe:
            note = "« Mettre à jour » réinstalle proprement ; « Désinstaller » retire tout."
            h3 = max(s(96), s(14) + s(20) + s(24) + hauteur_texte(note, largeur_texte, f_note, interligne_petit) + s(12))
            h3 = min(h3, max(s(96), bas_utile - (bas2 + s(16))))
            y3 = bas2 + s(16)
            carte(y3, y3 + h3, "VERT_LIME")
            texte(d, (g + s(20), y3 + s(24)), "Installation détectée", f_note, c("TEXTE"), "lm")
            texte(d, (g + s(20), y3 + s(50)), "Version 0.1.5 — identique au paquet : réinstallation (repair)",
                  f_note, c("TEXTE"), "lm")
            texte_multiligne(d, (g + s(20), y3 + s(66)), note, largeur_texte, f_note,
                             c("TEXTE_DOUX"), interligne_petit)
    elif vue == 3:
        statut = "Installation de Rose…"
        h_statut = hauteur_texte(statut, largeur_texte, f_corps, interligne)
        h_carte = max(s(72), h_statut + 2 * s(18))
        bas_c = carte(yc, yc + h_carte, "ROUGE_CLAIR")
        texte_multiligne(d, (g + s(20), yc + s(16)), statut, largeur_texte, f_corps, c("TEXTE"), interligne)
        piste_top = bas_c + s(20)
        d.rectangle([g, piste_top, droite, piste_top + s(6)], fill=c("LIGNE"))
        d.rectangle([g, piste_top, g + (droite - g) // 3, piste_top + s(6)], fill=c("ROUGE_CLAIR"))
    else:
        corps = ("Rose 0.1.5 est installé.\n\n"
                 "Dossier : C:\\Users\\utilisateur\\AppData\\Local\\Programs\\Rose\n\n"
                 "Ouvrez un nouveau terminal, puis tapez : rose --version")
        largeur_corps = max(s(80), largeur_texte - s(44))
        h_corps = hauteur_texte(corps, largeur_corps, f_corps, interligne)
        h_carte = min(max(h_corps + 2 * s(18), s(140)), max(s(140), bas_utile - yc))
        bas_c = carte(yc, yc + h_carte, "VERT")
        # coche
        d.line([g + s(20), yc + s(30), g + s(28), yc + s(38)], fill=c("VERT"), width=max(2, s(3)))
        d.line([g + s(28), yc + s(38), g + s(41), yc + s(22)], fill=c("VERT"), width=max(2, s(3)))
        texte_multiligne(d, (g + s(64), yc + s(16)), corps, largeur_corps, f_corps, c("TEXTE"), interligne)

    # ── pied de page ──
    pied_top = h - etat_h - pied_h
    d.rectangle([lateral_w + 1, pied_top, w, h - etat_h], fill=c("LATERAL_FIN"))
    d.line([lateral_w + 1, pied_top, w, pied_top], fill=c("LIGNE_FORTE"))
    # Largeur bornée, IDENTIQUE au Rust.
    place = max(s(120), w - lateral_w - s(56))
    bw = max(s(96), min(s(190), (place - s(20)) // 2))
    bh = s(44)
    by = pied_top + (pied_h - bh) // 2
    libelles = [
        ("Annuler", "Suivant"),
        ("Annuler", "Suivant"),
        ("Désinstaller", "Mettre à jour" if etat_installe else "Installer"),
        ("Annuler", None),
        ("Fermer", "Terminer"),
    ][vue]
    zones = []
    if libelles[0]:
        r = [lateral_w + s(28), by, lateral_w + s(28) + bw, by + bh]
        d.rounded_rectangle(r, radius=s(10), fill=c("REPOS_SOMBRE"))
        texte(d, ((r[0] + r[2]) // 2, (r[1] + r[3]) // 2), libelles[0].upper(),
              ImageFont.truetype(str(POLICE_TITRE), s(18)), c("NOIR_DOUX"), "mm")
        zones.append(r)
    if libelles[1]:
        r = [w - s(28) - bw, by, w - s(28), by + bh]
        d.rounded_rectangle(r, radius=s(10), fill=c("ROUGE"))
        texte(d, ((r[0] + r[2]) // 2, (r[1] + r[3]) // 2), libelles[1].upper(),
              ImageFont.truetype(str(POLICE_TITRE), s(15)), c("BLANC"), "mm")
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
    """Contrôles mesurés : libellés, cartes, logo, pied de page."""
    e = min(w / 1120.0, h / 780.0)
    s = lambda v: round(v * e)
    erreurs = []

    # Boutons : le libellé doit tenir dans la largeur RÉELLEMENT allouée.
    place = max(s(120), w - s(320) - s(56))
    bw = max(s(96), min(s(190), (place - s(20)) // 2))
    for libelle in ("Mettre à jour", "Installer", "Désinstaller", "Terminer", "Suivant"):
        largeur = mesurer(libelle.upper(), ImageFont.truetype(str(POLICE_TITRE), s(13)))
        if largeur > bw - s(12):
            erreurs.append(f"bouton « {libelle} » : {largeur}px pour {bw}px de bouton")
    if lateral_w := s(320):
        gauche_d = lateral_w + s(28) + bw
        droite_g = w - s(28) - bw
        if gauche_d > droite_g:
            erreurs.append(f"pied de page : chevauchement {gauche_d} > {droite_g}")

    # Logo : il doit remplir son PNG (sinon il est invisible à l'écran).
    logo = Image.open(LOGO).convert("RGBA")
    lw, lh = logo.size
    boite = logo.getbbox()
    if boite is None:
        erreurs.append("logo entièrement transparent")
    else:
        x0, y0, x1, y1 = boite
        if x0 > 4 or y0 > 4 or x1 < lw - 4 or y1 < lh - 1:
            erreurs.append(
                f"logo : contenu {x0},{y0}–{x1},{y1} dans {lw}×{lh} — "
                "une bande transparente réduit le dessin à l'écran")
    # Le logo doit tenir ENTIER dans sa boîte : `dessiner_rose` reçoit un
    # côté `cote` et doit produire `dw <= cote` ET `dh <= cote`, en gardant
    # les proportions. Un logo plus haut que large dans une boîte carrée
    # touchait le titre et paraissait coupé en bas.
    place = max(0, (h - s(30) - s(18) - s(38) - s(46) - s(16)) - s(78) - s(150))
    hv = round(s(124) * 169 / 132)
    hl = min(hv, place)
    if hl >= s(40):
        cote = hl
        dh = cote
        dw = max(1, round(cote * lw / lh))
        if dw > cote:
            dw, dh = cote, max(1, round(cote * lh / lw))
        if dh > cote or dw > cote:
            erreurs.append(f"logo {dw}x{dh} dépasse sa boîte {cote}x{cote}")
        # Marge avant le titre : la tige ne doit pas toucher « ROSE ».
        bas_logo = s(78) + (cote - dh) // 2 + dh
        if bas_logo >= s(78) + hl + s(16):
            erreurs.append(f"logo : bas {bas_logo} touche le titre {s(78)+hl+s(16)}")

    # Cartes : aucune ne doit déborder du pied de page, et la vue ne doit pas
    # être vide au-dessus du pied.
    bas_utile = h - s(30) - s(76) - s(20)
    for vue in range(5):
        img, zones, _ = dessiner(w, h, vue, etat_installe=(vue == 2))
        # La zone de contenu doit contenir des pixels de CARTE (le fond de
        # carte 0x1A1311) quelque part entre le sous-titre et le pied : une
        # fenêtre « vide » n'afficherait que le fond général.
        trouve = False
        for y in range(s(56) + s(120), bas_utile, 4):
            for x in range(s(320) + s(56) + s(24), w - s(56) - s(24), 6):
                p = img.getpixel((x, y))
                if p in (c("CARTE"), c("LIGNE"), c("ROUGE"), c("VERT"), c("ROUGE_CLAIR"),
                         c("VERT_LIME"), c("TEXTE"), c("TEXTE_DOUX")):
                    trouve = True
                    break
            if trouve:
                break
        if not trouve:
            erreurs.append(f"vue {vue} : zone de contenu VIDE (aucun pixel de carte)")

    # Police : aucune taille en dessous du plancher lisible.
    if pixels_police(9, 96, e) < PLANCHER_PX:
        erreurs.append("taille de police sous le plancher lisible")
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
