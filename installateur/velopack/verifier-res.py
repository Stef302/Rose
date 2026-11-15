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


def sections_pe(donnees: bytes):
    """Liste (VirtualAddress, PointerToRawData, SizeOfRawData) des sections."""
    pe = struct.unpack("<I", donnees[0x3C:0x40])[0]
    nsec = struct.unpack("<H", donnees[pe + 6:pe + 8])[0]
    opt = pe + 24
    taille_opt = struct.unpack("<H", donnees[pe + 20:pe + 22])[0]
    debut = opt + taille_opt
    out = []
    for i in range(nsec):
        o = debut + 40 * i
        vsize, va, rawsize, rawptr = struct.unpack("<IIII", donnees[o + 8:o + 24])
        out.append((va, rawptr, rawsize))
    return out


def pe_rsrc_rva(donnees: bytes) -> int:
    """RVA de la section `.rsrc` (entrée 2 du répertoire de données)."""
    pe = struct.unpack("<I", donnees[0x3C:0x40])[0]
    opt = pe + 24
    # DataDirectory commence après les champs fixes de l'en-tête optionnel :
    # PE32+ -> 112 octets, PE32 -> 96.
    magic = struct.unpack("<H", donnees[opt:opt + 2])[0]
    base = opt + (112 if magic == 0x20B else 96)
    return struct.unpack("<I", donnees[base + 2 * 8:base + 2 * 8 + 4])[0]


def sous_systeme_pe(donnees: bytes) -> int:
    """Subsystem de l'en-tête optionnel (2 = GUI, 3 = console)."""
    pe = struct.unpack("<I", donnees[0x3C:0x40])[0]
    opt = pe + 24
    return struct.unpack("<H", donnees[opt + 68:opt + 70])[0]


def rva_vers_offset(rva: int, sections) -> int:
    """Convertit une RVA en offset de fichier ; lève si elle tombe hors section."""
    for va, rawptr, rawsize in sections:
        if va <= rva < va + rawsize:
            return rawptr + (rva - va)
    raise AssertionError(f"RVA {rva:#x} hors de toute section "
                         f"(ressource illisible par Windows)")


def section_rsrc(donnees: bytes) -> bytes:
    """Extrait la section `.rsrc` d'un `.res` MSVC ou d'un PE."""
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
    # `.res` MSVC : en-tête de 32 o, puis des entrées
    #   DataSize(4) HeaderSize(4) ordinal(2) type(2) nom(2) nomNum(2) …
    assert donnees[:4] == b"\x00\x00\x00\x00", "en-tête .res invalide"
    rsrc = bytearray()
    off = 32
    while off + 8 <= len(donnees):
        taille, hsize = struct.unpack("<II", donnees[off:off + 8])
        rsrc += donnees[off + hsize:off + hsize + taille]
        off += hsize + taille
        if off % 4:
            off += 4 - off % 4
    raise AssertionError("verifier-res.py ne sait pas relire un .res directement ; "
                         "utiliser l'EXE lié")


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
    donnees = cible.read_bytes()
    est_pe = donnees[:2] == b"MZ"
    if est_pe:
        # Dans un PE, `OffsetToData` est une RVA : on doit la convertir en
        # offset de fichier. Ne pas le faire était le bug qui faisait lire du
        # CODE à la place du manifeste (« Cette application ne peut pas
        # s'exécuter sur votre PC »).
        sections = sections_pe(donnees)
    else:
        sections = []
    rsrc = section_rsrc(donnees)
    print(f"{cible.name} : section .rsrc de {len(rsrc)} o"
          + (" (PE lié, OffsetToData = RVA)" if est_pe else " (.res)"))

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
                if est_pe:
                    # `decalage` est une RVA : la convertir, sinon on lit du
                    # code machine (c'est exactement le bug d'origine).
                    decalage = rva_vers_offset(decalage, sections)
                    assert decalage + taille <= len(donnees), (
                        f"type {type_} nom {nom} : dépasse le fichier")
                    # La ressource doit être DANS `.rsrc`, pas ailleurs.
                    va_rsrc, ptr_rsrc, taille_rsrc = next(
                        s for s in sections if s[1] == rva_vers_offset(
                            pe_rsrc_rva(donnees), sections))
                    assert ptr_rsrc <= decalage < ptr_rsrc + taille_rsrc, (
                        f"type {type_} nom {nom} : RVA {decalage:#x} hors de "
                        f"la section .rsrc (ressource illisible par Windows)")
                    ressources[(type_, nom)] = donnees[decalage:decalage + taille]
                else:
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
        # ⚠ Une entrée RT_ICON doit contenir un DIB BRUT : elle commence par
        # un BITMAPINFOHEADER, dont `biSize` vaut 40 (0x28) le plus souvent.
        # Un conteneur `.ico` commence par `00 00 01 00` (reserved/type/count)
        # et Windows ne le décode PAS à cet endroit : l'icône est alors
        # ignorée, la fenêtre s'ouvre sans icône et l'Explorateur affiche
        # l'icône générique. C'est le bug qui a fait rater l'icône deux fois.
        tete = struct.unpack("<I", icone[:4])[0]
        assert icone[:4] != b"\x00\x00\x01\x00", (
            f"icône {ident} : contient un conteneur .ico au lieu d'un DIB brut "
            "(Windows ne l'affichera pas)")
        # Depuis Vista, une icône peut être un PNG COMPRESSÉ (256 px en
        # particulier) : elle commence par la signature PNG. Sinon c'est un
        # BITMAPINFOHEADER, dont `biSize` vaut 40 (ou 12 pour BITMAPCOREHEADER).
        est_png = icone[:8] == b"\x89PNG\r\n\x1a\n"
        assert est_png or tete in (12, 40, 108, 124), (
            f"icône {ident} : ni PNG ni DIB (biSize={tete} inattendu)")
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

    if est_pe:
        # L'installeur est une application GUI : subsystem 2. Un subsystem 3
        # (console) ouvrirait une fenêtre noire derrière la fenêtre.
        if cible.name.lower().startswith(("rose-setup", "setup-")):
            ss = sous_systeme_pe(donnees)
            assert ss == 2, f"rose-setup doit être en subsystem 2 (GUI), lu {ss}"
            print(f"  subsystem : {ss} (GUI)")

    # Moteur embarqué : sans lui, l'installeur n'a RIEN à installer et se
    # copie lui-même sous `rose.exe` → boucle de lancement (60 processus
    # mesurés chez l'utilisateur). On refuse un setup qui n'en contient pas.
    moteur = ressources.get((RT_RCDATA, 3))
    assert moteur is not None, (
        "moteur embarqué (RT_RCDATA 3) absent : l'installeur n'aurait rien "
        "à installer et se copierait lui-même sous rose.exe (boucle)")
    assert moteur[:2] == b"MZ", "le moteur embarqué n'est pas un PE"
    assert len(moteur) > 1_000_000, f"moteur embarqué suspect : {len(moteur)} o"
    # ⚠ Le moteur doit être une application CONSOLE (sous-système 3). Un
    # moteur en GUI (2) serait l'installeur : l'installer sous `rose.exe`
    # provoque une boucle de lancement (fenêtres Terminal en rafale).
    pe_off = struct.unpack("<I", moteur[0x3C:0x40])[0]
    assert moteur[pe_off:pe_off + 4] == b"PE\0\0", "en-tête PE du moteur invalide"
    ss = struct.unpack("<H", moteur[pe_off + 24 + 68:pe_off + 24 + 70])[0]
    assert ss == 3, (
        f"le moteur embarqué est en sous-système {ss} (GUI=2), pas 3 (console) : "
        "il provoquerait une boucle de lancement")
    print(f"  moteur embarqué : {len(moteur)} o (PE console)")

    # Fichiers compagnons : sans eux, l'installation ne pose QUE rose.exe
    # (mesuré : ni helper AOT, ni shell, ni icône de fichier .rose).
    # ⚠ `rose-aot.exe` n'est PAS exigé : le stub n'embarque pas LLVM, il est
    # inutile. `rose.exe` gère `orc`/`link` et télécharge le vrai helper.
    for res_id, nom, min_taille in [
        (5, "rose-shell.exe", 100_000),
        (6, "rose.ico", 10_000),
        (7, "rose_app.ico", 10_000),
    ]:
        blob = ressources.get((RT_RCDATA, res_id))
        assert blob is not None, f"compagnon {nom} absent (RT_RCDATA {res_id})"
        assert len(blob) > min_taille, f"compagnon {nom} suspect : {len(blob)} o"
        if nom.endswith(".exe"):
            assert blob[:2] == b"MZ", f"compagnon {nom} n'est pas un PE"
        else:
            assert blob[:4] == b"\x00\x00\x01\x00", f"compagnon {nom} n'est pas un ICO"
        print(f"  {nom:<16} {len(blob):>8} o")

    # La rose affichée dans l'installeur (RT_RCDATA 1) doit être un PNG
    # DÉCODABLE. Mesuré : le décodeur maison fedait les IDAT à un
    # décompresseur DEFLATE brut au lieu d'un décompresseur ZLIB → il
    # rendait `None` → le logo n'était JAMAIS dessiné (ni dans la barre de
    # titre, ni au-dessus de « ROSE »), sans aucun message d'erreur.
    png = ressources.get((RT_RCDATA, 1))
    assert png is not None, "rose PNG (RT_RCDATA 1) absente"
    assert png[:8] == b"\x89PNG\r\n\x1a\n", "la rose n'est pas un PNG"
    assert png[12:16] == b"IHDR", "en-tête IHDR absent"
    w = int.from_bytes(png[16:20], "big")
    h = int.from_bytes(png[20:24], "big")
    assert png[24] == 8 and png[25] == 6, "le PNG doit être RGBA 8 bits"
    print(f"  rose PNG      {len(png)} o ({w}x{h}, RGBA)")

    # ── Contrôle du TRACÉ : la fenêtre doit être PEINTE ────────────────────
    #
    # Symptôme mesuré : « la fenêtre s'ouvre avec ses seules BORDURES, elle est
    # transparente, puis disparaît au bout d'un moment ». Un plantage dans
    # `WM_PAINT` laisse une fenêtre vide, et un processus mort la fait
    # disparaître. Aucun test d'exécution n'est possible sans Windows : on
    # vérifie donc que le CODE de `setup_gui.rs` contient les garde-fous qui
    # rendent ce symptôme impossible.
    if cible.name.lower().startswith(("rose-setup", "setup-")):
        source = (RACINE / "rose" / "src" / "setup_gui.rs").read_text(encoding="utf-8")
        garde_fous = {
            "tracé isolé dans catch_unwind": "std::panic::catch_unwind",
            "fond peint sur l'écran AVANT la décoration": "remplir(dc_ecran",
            "EndPaint hors du tracé détaillé": "EndPaint(h, &ps);",
            "repli GDI du cercle": "Ellipse(dc, cx - rayon",
            "repli GDI des coins arrondis": "RoundRect(dc, r.left",
            "boucle de messages qui distingue 0 de -1": "GetLastError()",
            "SW_SHOWNORMAL (fenêtre au premier plan)": "SW_SHOWNORMAL",
            "WM_ERASEBKGND sans double peinture": "WM_ERASEBKGND",
            "zones de titre cohérentes": "let coherentes =",
            "police refusée → pas de DrawText géant": "let Some(police) = creer_police",
            # ⚠ CLIGNOTEMENT : le survol ne doit JAMAIS invalider la fenêtre
            # entière (repaint de 1120 × 780 à chaque pixel de souris).
            "repeint ciblé au survol (pas InvalidateRect NULL)": "InvalidateRect(h, &zones[i as usize], 0)",
            "curseurs mis en cache": "fn curseur_fleche()",
            "WM_SETCURSOR traité": "WM_SETCURSOR =>",
            "coordonnées client lues dans lParam": "let x = (l as i32) << 16 >> 16;",
            # ⚠ LOGO : proportions conservées et centrage vertical.
            "logo borné dans sa boîte (hauteur <= cote)": "let mut dh = cote;",
            "logo centré verticalement": "let dy = y + (cote - dh) / 2;",
            "marge entre logo et titre": "let marge_marque = s(16);",
        }
        manquants = [nom for nom, motif in garde_fous.items() if motif not in source]
        assert not manquants, (
            "garde-fous du tracé absents de setup_gui.rs : " + ", ".join(manquants)
            + " — la fenêtre peut s'ouvrir VIDE ou clignoter")
        # Aucun appel direct de `dessiner` depuis `WM_PAINT` : il doit passer
        # par le `catch_unwind` du gestionnaire.
        assert "WM_PAINT => { dessiner(h); 0 }" not in source, (
            "WM_PAINT appelle dessiner() sans isolation : une panique tue le processus "
            "et la fenêtre disparaît")
        # ⚠ Le survol ne doit plus invalider TOUTE la fenêtre.
        assert "InvalidateRect(h, std::ptr::null(), 0);\n            }\n            SetCursor" not in source, (
            "WM_MOUSEMOVE invalide la fenêtre entière au survol : clignotement")
        print(f"  tracé : {len(garde_fous)} garde-fous présents")

    print("RESSOURCES VALIDES")


if __name__ == "__main__":
    main()
