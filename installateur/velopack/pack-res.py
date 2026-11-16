#!/usr/bin/env python3
"""Emballe les ressources Windows de l'installeur Rose dans `rose-setup.res`.

Win32 ne sait pas lire une police WOFF2 ni une image PNG directement : on
construit une table des ressources qui contient

- `RT_RCDATA 10 1` : la rose (PNG, 176 px) ;
- `RT_FONT   8  2` : la police Kare convertie en TrueType ;
- `RT_ICON` + `RT_GROUP_ICON 1` : toutes les tailles de `rose_setup.ico`
  (16, 32, 48, 64, 128, 256) — Win32 choisit celle qui convient, et la grande
  sert de `WM_SETICON` de la fenêtre ;
- `RT_MANIFEST 24 1` : le manifeste (DPI PerMonitorV2, common controls v6).

Sortie : `rose/rose-setup.res`, lié au seul binaire `rose-setup` par
`build.rs` (`cargo:rustc-link-arg-bin=rose-setup`).

⚠ HISTORIQUE — pourquoi ce script passe par `llvm-rc`.

La première version écrivait un objet COFF à la main (en-tête de fichier +
section `.rsrc`). Deux bugs en sont sortis, invisibles au contrôle
structurel mais FATALS sur Windows :

1. `OffsetToData` est lu par Windows comme une **RVA**, pas comme un offset
   de section. L'objet n'émettait aucune relocation, donc le lieur laissait
   la valeur telle quelle (`0x2e8`) : Windows la résolvait dans `.text`, et
   lisait du CODE MACHINE à la place du manifeste. Résultat : « Cette
   application ne peut pas s'exécuter sur votre PC ».
2. L'en-tête de section était écrit avec 6 champs `I` au lieu de 7 :
   `PointerToRelocations` manquait, et tout le reste glissait d'un cran.

`llvm-rc` produit le format `.res` MSVC canonique, que `lld-link` sait lier :
il calcule lui-même les RVAs et applique les relocations. Il génère aussi
`RT_GROUP_ICON` à partir d'un `.ico`. On lui donne un `.rc` généré et on
n'a plus une seule ligne de format binaire à maintenir.

Dépendances : fontTools (déjà utilisé pour le site), llvm-rc, Python 3.

Usage : python3 installateur/velopack/pack-res.py
"""

import struct
import subprocess
import tempfile
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._g_l_y_f import Glyph, GlyphComponent
from fontTools.ttLib.tables.ttProgram import Program

RACINE = Path(__file__).resolve().parents[2]
PNG = RACINE / "installateur" / "velopack" / "rose-installeur.png"
WOFF2 = RACINE / "docs" / "assets" / "fonts" / "kare" / "Kare.woff2"
# Icône de la fenêtre et du setup : `rose_setup.ico`, la BOÎTE D'INSTALLATION
# avec la rose (convertie depuis `icone_installeur.svg`).
#
# ⚠ Trois icônes, trois rôles — ne pas les confondre :
#   `rose.ico`       = TYPE DE FICHIER (document « .ROSE ») **et** LOGO DE
#                      BASE de l'APPLICATION (raccourcis, barre des tâches) ;
#   `rose_shell.ico` = CONSOLE (terminal + invite + rose), pour
#                      rose-shell.exe et rose-aot.exe SEULEMENT — plus
#                      jamais sur rose.exe (correction du 16/11) ;
#   `rose_setup.ico` = INSTALLEUR (boîte + flèches), pour Rose-win-Setup.exe
#                      et la fenêtre de l'assistant.
ICO = RACINE / "installateur" / "assets" / "rose_setup.ico"
MANIFESTE = RACINE / "rose" / "rose-setup.manifest"
# Binaires compilés, embarqués dans l'installeur (binaire unique).
_REL = RACINE / "rose" / "target" / "x86_64-pc-windows-msvc" / "release"
MOTEUR = _REL / "rose.exe"
# Le shell : sans lui, l'installation ne contient QUE `rose.exe`.
#
# ⚠ `rose-aot.exe` N'EST PAS EMBARQUÉ : le stub compilé ici n'a pas la
# feature `llvm`, il ne peut donc rien compiler. `rose.exe` gère déjà `orc`
# et `link` et télécharge à la demande le vrai helper LLVM.
SHELL = _REL / "rose-shell.exe"
# Icônes à poser dans `bin\` : `rose.ico` sert au TYPE DE FICHIER `.rose`
# (association Windows) ET devient `rose_app.ico` (icône de l'APPLICATION :
# raccourcis, profil Windows Terminal, barre des tâches).
ICONE_FICHIER = RACINE / "installateur" / "assets" / "rose.ico"
# ⚠⚠ CORRECTION DU 16/11 : `rose_app.ico` portait `rose_shell.ico` (fenêtre de
# terminal). L'utilisateur veut le LOGO DE BASE sur l'APPLICATION (raccourcis,
# profil Windows Terminal, barre des tâches) — donc `rose.ico`.
# `rose_shell.ico` reste UNIQUEMENT sur les binaires console `rose-shell.exe`
# et `rose-aot.exe` (via leurs `.rc`).
ICONE_APP = RACINE / "installateur" / "assets" / "rose.ico"
SORTIE = RACINE / "rose" / "rose-setup.res"

# Identifiants des ressources RT_RCDATA embarquées. Ils doivent rester
# synchronisés avec `setup_gui.rs` (voir `ressource()`).
RCDATA_PNG = 1      # rose affichée dans la fenêtre
RCDATA_MOTEUR = 3   # rose.exe
# L'id 4 (anciennement rose-aot.exe) est LIBRE : ne pas le réutiliser.
RCDATA_SHELL = 5        # rose-shell.exe
RCDATA_ICO_FICHIER = 6  # rose.ico  (type de fichier .rose)
RCDATA_ICO_APP = 7      # rose.ico (application : logo de base)

# Langue des ressources : 1033 = en-US, la valeur qu'émet `llvm-rc`/`rc.exe`.
LANGUE = 1033

# Types de ressources (identifiants numériques Win32).
RT_ICON = 3
RT_GROUP_ICON = 14
RT_FONT = 8
RT_RCDATA = 10
RT_MANIFEST = 24


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
    with tempfile.NamedTemporaryFile(suffix=".ttf", delete=False) as tmp:
        temporaire = Path(tmp.name)
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


def verifier_res(chemin: Path, attendus: set[int]) -> None:
    """Relit le `.res` et vérifie que chaque type attendu est présent.

    Un `.res` vide ou tronqué se lierait sans erreur : le contrôle doit
    porter sur le CONTENU, pas sur le code de sortie de `llvm-rc`.
    """
    d = chemin.read_bytes()
    assert d[:4] == b"\x00\x00\x00\x00", "en-tête .res invalide"
    types: set[int] = set()
    off = 32  # taille de l'en-tête .res
    while off + 8 <= len(d):
        taille, hsize = struct.unpack("<II", d[off:off + 8])
        # Entrée .res : DataSize(4) HeaderSize(4) puis
        #   ordinal WORD (0xFFFF si type NUMÉRIQUE) + type WORD,
        #   nom WORD (0xFFFF si nom NUMÉRIQUE) + nom WORD.
        # Un type numérique est donc à l'offset +10, en WORD.
        type_ = struct.unpack("<H", d[off + 10:off + 12])[0]
        types.add(type_)
        off += hsize + taille
        if off % 4:
            off += 4 - off % 4
    manquants = attendus - types
    assert not manquants, f"types absents du .res : {sorted(manquants)}"
    print(f"  types présents : {sorted(t for t in types if t > 0)}")


def main():
    ttf = woff2_vers_ttf(WOFF2)
    icones = entrees_ico(ICO)
    # Moteur embarqué : c'est CE qui permet à `Rose-win-Setup.exe` d'être un
    # fichier unique. Sans lui, l'installeur n'avait aucune source à copier
    # et retombait sur lui-même — d'où la boucle de lancement.
    moteur = MOTEUR.read_bytes() if MOTEUR.exists() else None
    if moteur is None:
        # ⚠ Un installeur SANS moteur embarqué n'a rien à installer : il se
        # copiait lui-même sous `rose.exe`, créant une boucle de lancement
        # (60 processus chez l'utilisateur). On refuse de produire un tel
        # fichier plutôt que de le publier.
        raise SystemExit(
            f"moteur absent : {MOTEUR}\n"
            "Compilez d'abord `cargo build --release --target "
            "x86_64-pc-windows-msvc --bin rose`, sinon l'installeur n'aurait "
            "rien à installer et se copierait lui-même sous rose.exe."
        )
    # ⚠ Le moteur embarqué doit être une application CONSOLE (sous-système 3).
    # Un binaire GUI (2) est l'installeur : l'embarquer puis l'installer sous
    # `rose.exe` produit une boucle de fenêtres Windows Terminal.
    if moteur[:2] != b"MZ":
        raise SystemExit(f"moteur invalide (pas un PE) : {MOTEUR}")
    pe_off = struct.unpack("<I", moteur[0x3C:0x40])[0]
    ss = struct.unpack("<H", moteur[pe_off + 24 + 68:pe_off + 24 + 70])[0]
    if ss != 3:
        raise SystemExit(
            f"le moteur {MOTEUR} est en sous-système {ss} (GUI=2 attendu 3=console) : "
            "c'est probablement l'installeur, pas le moteur Rose."
        )

    with tempfile.TemporaryDirectory() as tmpdir:
        tmp = Path(tmpdir)
        ttf_path = tmp / "kare.ttf"
        ttf_path.write_bytes(ttf)

        # `.rc` : une ligne par ressource. Les chemins sont absolus pour que
        # le fichier soit indépendant du répertoire courant.
        #
        # ⚠ Pour les icônes, `llvm-rc` NE découpe PAS un `.ico` : donné à
        # RT_ICON, il embarque le fichier entier (136 282 o pour notre ICO)
        # comme UNE icône, sans `RT_GROUP_ICON` — Windows n'a alors aucune
        # icône utilisable. On extrait donc chaque taille en RT_ICON 1..n et
        # on écrit le `RT_GROUP_ICON 1` à la main.
        #
        # ⚠⚠ Le fichier donné à `RT_ICON` doit contenir les OCTETS BRUTS de
        # l'image (BITMAPINFOHEADER + pixels), PAS un conteneur `.ico`.
        # `llvm-rc` recopie le fichier tel quel : en lui donnant un `.ico`
        # d'un seul élément, chaque RT_ICON commençait par l'en-tête ICO
        # `00 00 01 00 …` (reserved/type/count) que Windows n'attend pas à
        # cet endroit. Résultat mesuré : la ressource existait, le groupe
        # aussi, mais AUCUNE icône ne s'affichait — ni dans la fenêtre, ni
        # dans l'Explorateur. On écrit donc `e["donnees"]` seul.
        lignes = [
            f'1 {RT_RCDATA} "{PNG}"',
            f'2 {RT_FONT} "{ttf_path}"',
        ]
        if moteur is not None:
            # RT_RCDATA id 3 : le moteur `rose.exe` lui-même. `setup_gui` le
            # lit par `FindResourceW` et l'écrit dans `bin\rose.exe`.
            moteur_path = tmp / "moteur.exe"
            moteur_path.write_bytes(moteur)
            lignes.append(f'{RCDATA_MOTEUR} {RT_RCDATA} "{moteur_path}"')
        # ── Binaires et icônes COMPAGNONS ───────────────────────────────
        # ⚠ Sans eux, l'installation ne contenait QUE `rose.exe` (mesuré) :
        # `installer()` copiait `rose-shell.exe` et les icônes depuis le
        # dossier de la SOURCE — or la source est le moteur extrait dans
        # `%TEMP%\rose-setup-payload\`, qui ne contient que `rose.exe`.
        # Les fichiers compagnons n'ont donc jamais été posés.
        for res_id, chemin, nom in [
            (RCDATA_SHELL, SHELL, "rose-shell.exe"),
            (RCDATA_ICO_FICHIER, ICONE_FICHIER, "rose.ico"),
            (RCDATA_ICO_APP, ICONE_APP, "rose_app.ico"),
        ]:
            if chemin.exists():
                copie = tmp / nom
                copie.write_bytes(chemin.read_bytes())
                lignes.append(f'{res_id} {RT_RCDATA} "{copie}"')
            else:
                print(f"  ⚠ {nom} absent ({chemin}) — ne sera pas installé")
        for i, e in enumerate(icones, start=1):
            brut = tmp / f"icone{i}.bin"
            brut.write_bytes(e["donnees"])
            lignes.append(f'{i} {RT_ICON} "{brut}"')

        groupe = struct.pack("<HHH", 0, 1, len(icones))
        for i, e in enumerate(icones, start=1):
            # ⚠ La taille inscrite dans le groupe doit être celle que
            # `llvm-rc` écrit réellement dans l'ENTRÉE DE DONNÉES du `.res` :
            # Taille de l'image telle qu'elle est stockée dans RT_ICON : les
            # OCTETS BRUTS du DIB, sans l'en-tête ICO ni celui du `.res`.
            # (Avec l'ancien conteneur `.ico`, il fallait +22 ; maintenant
            # c'est exactement `len(donnees)`.)
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
        groupe_path = tmp / "groupe.bin"
        groupe_path.write_bytes(groupe)
        lignes.append(f'1 {RT_GROUP_ICON} "{groupe_path}"')

        if MANIFESTE.exists():
            lignes.append(f'1 {RT_MANIFEST} "{MANIFESTE}"')

        rc = tmp / "rose-setup.rc"
        rc.write_text("\n".join(lignes) + "\n", encoding="ascii")

        res = subprocess.run(
            ["llvm-rc", str(rc), "-fo", str(SORTIE)],
            capture_output=True, text=True,
        )
        if res.returncode != 0:
            raise SystemExit(f"llvm-rc a échoué :\n{res.stdout}{res.stderr}")

    attendus = {RT_RCDATA, RT_FONT, RT_ICON, RT_GROUP_ICON}
    if MANIFESTE.exists():
        attendus.add(RT_MANIFEST)
    verifier_res(SORTIE, attendus)

    print(f"{SORTIE.relative_to(RACINE)} : {SORTIE.stat().st_size} o")
    print(f"  rose PNG {PNG.stat().st_size} o | Kare TTF {len(ttf)} o")
    print("  icônes    " + " ".join(f"{e['largeur']}px" for e in icones))


if __name__ == "__main__":
    main()
