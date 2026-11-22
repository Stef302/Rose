#!/usr/bin/env python3
"""Génère les compilateurs natifs composites .rose -> ELF / PE (auto-hébergés).

Composition DÉTERMINISTE de deux sources Rose déjà validées :
  * compilateur/emetteur.rose      : front-end source -> dump RIR (oracle `rose rir`)
  * compilateur/emetteur_elf.rose  : backend RIR -> ELF64 aarch64 (machine G5)
  * compilateur/emetteur_pe.rose   : backend RIR -> PE32+ aarch64 (machine G5)

Aucune de ces sources n'est modifiée par CE script. Les fichiers générés sont
des programmes Rose autonomes :
  rose rose_natif.rose    prog.rose > prog.elf
  rose rose_natif_pe.rose prog.rose > prog.exe

Choix de composition (mesurés, voir rose/docs/SELFHOST_TOTAL.md) :
  - 0 collision de noms de fonctions entre le front-end et chaque backend ;
  - le front-end est enfermé dans `em_source_rir()` : ses 116 variables de
    `$principal` deviennent LOCALES, ce qui ramène le total des globales G5
    à 402 (< 512, borne de la table 0x60004000) ;
  - le backend G5 RESTE au niveau supérieur : ses 402 globales et ses
    routages `$g5_lenl`, `$g5_plist_l`, … sont lus par les fonctions
    `g5_emit_*` et doivent rester des globales ;
  - le « vieux compilateur » de fin de emetteur_elf.rose doit rester présent
    (ses globales sont lues par des fonctions conservées) mais est
    unreachable : le chemin G5 se termine par quitter(0).

Le dumpeur RIR du front-end est remplacé par une construction de chaîne
retournée : aucun fichier temporaire, aucun tube.

Injection HOST-2 (voir rose/docs/EVAL_EXPR_SELFHOST.md) :
  `eval_expr` est une primitive NATIVE du moteur Rust (front.rs / interp.rs),
  utilisée ~30 fois par le backend. Le front-end ROSE (emetteur.rose) ne la
  connaît pas : sans `std_sig`, il refuse de compiler sa propre source. On
  ajoute donc l'entrée `"chaine|chaine"` UNIQUEMENT dans la copie du front-end
  destinée au composite généré — `compilateur/emetteur.rose` reste intact
  (point fixe b3ed9262). L'implémentation Rose est, elle, dans
  `compilateur/emetteur_elf.rose` (sous-ensemble G5).

Repères du backend : ils sont trouvés PAR CONTENU, pas par numéro de ligne.
Les numéros figés dans les versions précédentes (3654/3658/3811 pour ELF)
étaient périmés depuis le correctif UTF-8 livré dans emetteur_elf.rose et
faisaient échouer la génération. La détection par contenu rend le script
robuste aux ajouts de lignes dans le backend.
"""

from pathlib import Path
import os
import sys

ROOT = Path(__file__).resolve().parent.parent
EM = ROOT / "compilateur" / "emetteur.rose"
BACKENDS = {
    "elf": (ROOT / "compilateur" / "emetteur_elf.rose",
            ROOT / "compilateur" / "rose_natif.rose"),
    "pe": (ROOT / "compilateur" / "emetteur_pe.rose",
           ROOT / "compilateur" / "rose_natif_pe.rose"),
}

# std_sig du front-end Rose : eval_expr est une primitive native du moteur
# Rust (front.rs std_sig), absente de la table Rose. Injectée dans le
# COMPOSITE uniquement, avec la signature « chaine|chaine ».
STD_SIG_ANCRE = '  si ($nom == "lire_texte") { retour "chaine|chaine" }\n'
STD_SIG_EVAL_EXPR = STD_SIG_ANCRE + \
    '  si ($nom == "eval_expr") { retour "chaine|chaine" }\n'
# OCTET_BRUT : `octet_brut` est définie comme `fn` Rose dans le backend ELF
# (voie Rust = 1 code point -> 1 octet) mais interceptée par le dispatch G5
# ($g5_octetll) pour écrire un octet BRUT sans UTF-8. Le front-end du
# composite doit connaître sa signature pour accepter sa propre source,
# exactement comme pour eval_expr (HOST-2). Injecté uniquement dans la copie
# générée : emetteur.rose reste intact (point fixe b3ed9262).
STD_SIG_INJECT = STD_SIG_EVAL_EXPR + \
    '  si ($nom == "octet_brut") { retour "chaine|entier" }\n'

# Garde de borne pour la queue « vieux compilateur » du backend : avec un
# seul argument (C0 rose_natif.rose), `$sortie = argument(1)` lève
# « Rose : erreur — index hors des limites des arguments » avant même
# d'atteindre le chemin G5. On ne modifie PAS emetteur_elf.rose (identité),
# seulement la copie concaténée dans le composite généré.
SORTIE_SANS_GARDE = '$sortie = argument(1)'
SORTIE_AVEC_GARDE = ('$sortie = ""\n'
                     'si (nb_arguments() > 1) { $sortie = argument(1) }')

# ── Diagnostic de phase (optionnel, DESACTIVE par défaut) ────────────────
# RNAT_PHASE=imports|lex|pools|repli|verif|rir|dump ajoute un marqueur et un
# `quitter(0)` à la fin de la phase demandée DANS LA COPIE GENEREE uniquement.
# Sert à mesurer le pic RSS phase par phase sans toucher à emetteur.rose.
# RNAT_OUT_ELF / RNAT_OUT_PE permettent de générer dans /tmp pour les mesures.
PHASES = ("imports", "lex", "pools", "repli", "verif", "rir", "dump")
PHASE_ANCRES = (
    ("imports", "# —— phase 1 : lexage vers $gen"),
    ("lex", "# —— phase 2 : descente vers les pools ——"),
    ("pools", "# —— tri des fonctions par nom (ordre std::map) ——"),
    ("repli", "# —— passe 2 : vérification ——"),
    ("verif", "# —— passe 3 : émission RIR (oracle « rose rir ») ——"),
)

# ── Correctif FRONT-END (injecté dans la copie générée, jamais dans la source) ──
# Le moteur Rust représente `+` sur chaînes par une rope O(1). Le binaire G5
# auto-hébergé, LUI, COPIE les deux opérandes à chaque `+` (g5_emit_cat) dans
# un tas bump sans libération : toute accumulation `s = s + morceau` en
# boucle fuit O(longueur²) octets. Les sites dominants mesurés sont
# `rir_patch_op` (boucle #ops x #labels avec sous_chaine/remplacer_tout) et
# le dump RIR concaténé ligne à ligne. On remplace ces accumulations par des
# listes de morceaux jointes en équilibre (`rnat_join`, O(n log n)) et on
# aplatit les chaînes déjà construites avec `sous_chaine(s, 0, longueur(s))`.
# La primitive `concatener` du runtime G5 est défectueuse (mesuré : rend 0 /
# segfaulte) : on ne l'utilise PAS. Contenu identique octet pour octet.
# `RNAT_FRONT_OPT=0` désactive pour l'A/B avant/après.
FRONT_OPT = os.environ.get("RNAT_FRONT_OPT", "1") != "0"

rnat_flat_fn = """fn rnat_flat($s: chaine) -> chaine {
  retour sous_chaine($s, 0, longueur($s))
}

# Joint équilibré : chaque niveau recopie au plus la longueur totale, donc
# O(n*log n) octets alloués au lieu de O(n^2) pour `s = s + morceau`.
# (Le `+` du runtime G5 COPIE les deux chaînes — pas de rope, contrairement
# au moteur Rust — et les ancres ne sont pas libérées : chaque concaténation
# en bouche est un leak.)
fn rnat_join($p: liste<chaine>, $d: entier, $f: entier) -> chaine {
  si ($f - $d <= 1) {
    si ($f - $d == 1) { retour $p[$d] }
    retour ""
  }
  $m = ($d + $f) div 2
  retour rnat_join($p, $d, $m) + rnat_join($p, $m, $f)
}

"""


def lines(p: Path):
    return p.read_text(encoding="utf-8").splitlines()


def replace_once(text: str, old: str, new: str, what: str) -> str:
    n = text.count(old)
    if n != 1:
        raise SystemExit(f"ERREUR {what}: attendu 1 occurrence, trouvé {n}")
    return text.replace(old, new, 1)


def trouver_reperes(be: list, backend: str) -> dict:
    """Repères du dispatch G5 trouvés par CONTENU (robustes aux ajouts)."""
    dispatch = [i for i, l in enumerate(be) if l == 'si (nb_arguments() >= 1) {']
    if len(dispatch) != 1:
        raise SystemExit(f"ERREUR {backend} dispatch: {len(dispatch)} occurrences")
    d = dispatch[0]
    quits = [i for i in range(d + 1, len(be)) if be[i] == 'quitter(0)']
    if not quits:
        raise SystemExit(f"ERREUR {backend}: quitter(0) introuvable après le dispatch")
    q = quits[0]
    # Dernière accolade de niveau 0 avant le dispatch = fin de la dernière
    # fonction du head (les fonctions du head sont toutes au niveau supérieur).
    fin_head = [i for i in range(d) if be[i] == '}']
    if not fin_head:
        raise SystemExit(f"ERREUR {backend}: '}}' de fin de head introuvable")
    h = fin_head[-1]
    src = '$src = lire_texte(argument(0))'
    if src not in be[d:q + 1]:
        raise SystemExit(f"ERREUR {backend}: ligne source G5 introuvable")
    return dict(head_end=h + 1, dispatch_start=d + 1, quit_line=q + 1,
                src_line=src)


def injecte_front_opt(em_head: str, em_main: str, new_tail: str):
    """Aplati UNE FOIS les chaînes accumulées par le front-end (copie générée)."""
    if not FRONT_OPT:
        return em_head, em_main, new_tail
    em_head = rnat_flat_fn + em_head
    em_head = replace_once(
        em_head, "  joindre($fcorps, $corps)",
        "  $corps = rnat_flat($corps)\n  joindre($fcorps, $corps)",
        "OPT fcorps")
    em_main = replace_once(
        em_main, "# —— tri des fonctions par nom (ordre std::map) ——",
        "$topb = rnat_flat($topb)\n# —— tri des fonctions par nom (ordre std::map) ——",
        "OPT topb")
    em_main = replace_once(
        em_main, '$err2 = [""]',
        '$topg = rnat_flat($topg)\n$err2 = [""]',
        "OPT topg")
    # remplacer_tout : morceaux dans une liste, jointure équilibrée en fin.
    old_rempl = """fn remplacer_tout($s: chaine, $de: chaine, $vers: chaine) -> chaine {
  $o = ""
  $i = 0
  $nd = longueur($de)
  $ns = longueur($s)
  tant_que ($i < $ns) {
    si ($i + $nd <= $ns et sous_chaine($s, $i, $i + $nd) == $de) {
      $o = $o + $vers
      $i = $i + $nd
    } sinon {
      $o = $o + $s[$i]
      $i = $i + 1
    }
  }
  retour $o
}"""
    new_rempl = """fn remplacer_tout($s: chaine, $de: chaine, $vers: chaine) -> chaine {
  $parts = [""]
  $cur = ""
  $i = 0
  $nd = longueur($de)
  $ns = longueur($s)
  tant_que ($i < $ns) {
    si ($i + $nd <= $ns et sous_chaine($s, $i, $i + $nd) == $de) {
      joindre($parts, $cur)
      joindre($parts, $vers)
      $cur = ""
      $i = $i + $nd
    } sinon {
      $cur = $cur + $s[$i]
      $i = $i + 1
    }
  }
  joindre($parts, $cur)
  retour rnat_join($parts, 1, longueur($parts))
}"""
    em_head = replace_once(em_head, old_rempl, new_rempl, "OPT remplacer_tout")
    # rir_patcher / rir_patch_op : O(#labels x #ops) -> O(1) table par op.
    old_patcher = """  $out = [""]
  $u = 1
  tant_que ($u < longueur($res)) {
    $ch = $res[$u]
    $ch = rir_patch_op($ch, $u, $labels, $cibles)
    joindre($out, $ch)
    $u = $u + 1
  }
  retour $out"""
    new_patcher = """  $out = [""]
  $tab = table()
  $z = 1
  tant_que ($z < longueur($labels)) {
    inserer($tab, $labels[$z], $cibles[$z])
    $z = $z + 1
  }
  $u = 1
  tant_que ($u < longueur($res)) {
    $ch = rir_patch_op($res[$u], $u, $labels, $cibles, $tab)
    joindre($out, $ch)
    $u = $u + 1
  }
  retour $out"""
    em_head = replace_once(em_head, old_patcher, new_patcher, "OPT rir_patcher")
    old_patchop_start = """fn rir_patch_op($ch: chaine, $pos: entier, $labels: liste<chaine>, $cibles: liste<entier>) -> chaine {"""
    new_patchop_start = """fn rir_patch_op($ch: chaine, $pos: entier, $labels: liste<chaine>, $cibles: liste<entier>, $tab: table) -> chaine {"""
    em_head = replace_once(em_head, old_patchop_start, new_patchop_start,
                           "OPT rir_patch_op signature")
    # Corps entier de rir_patch_op (entre la signature et la fonction suivante).
    debut = em_head.index(new_patchop_start)
    fin = em_head.index("\nfn cibles_distance(", debut)
    nouveau_corps = """
  $parts = [""]
  $cur = ""
  $i = 0
  $n = longueur($ch)
  tant_que ($i < $n) {
    si ($ch[$i] == "@") {
      $j = $i + 1
      $lab = ""
      tant_que ($j < $n et est_suite(ordre($ch[$j]))) {
        $lab = $lab + $ch[$j]
        $j = $j + 1
      }
      si (longueur($lab) > 0 et cle_existe($tab, $lab)) {
        joindre($parts, $cur)
        joindre($parts, chaine($tab[$lab]))
        $cur = ""
        $i = $j
      } sinon {
        $cur = $cur + "@"
        $i = $i + 1
      }
    } sinon {
      $cur = $cur + $ch[$i]
      $i = $i + 1
    }
  }
  joindre($parts, $cur)
  retour rnat_join($parts, 1, longueur($parts))
}

"""
    em_head = em_head[:debut] + new_patchop_start + nouveau_corps + em_head[fin + 1:]
    # piece() : une seule sous-chaine au lieu d'un `buf = buf + s[j]` par
    # caractère (O(n^2) allocations, appelé des milliers de fois).
    old_piece = """fn piece($s: chaine, $sep: entier, $k: entier) -> chaine {
  $n = longueur($s)
  $j = 0
  $no = 1
  $buf = ""
  tant_que ($j < $n) {
    si (ordre($s[$j]) == $sep) {
      si ($no == $k) { retour $buf }
      $no = $no + 1
      $buf = ""
    }
    sinon { $buf = $buf + $s[$j] }
    $j = $j + 1
  }
  si ($no == $k) { retour $buf }
  retour ""
}"""
    new_piece = """fn piece($s: chaine, $sep: entier, $k: entier) -> chaine {
  $n = longueur($s)
  $j = 0
  $no = 1
  $debut = 0
  tant_que ($j < $n) {
    si (ordre($s[$j]) == $sep) {
      si ($no == $k) { retour sous_chaine($s, $debut, $j) }
      $no = $no + 1
      $debut = $j + 1
    }
    $j = $j + 1
  }
  si ($no == $k) { retour sous_chaine($s, $debut, $n) }
  retour ""
}"""
    em_head = replace_once(em_head, old_piece, new_piece, "OPT piece")
    # em_corps / rir_corps : découpage du corps en indices via positions de
    # virgules + sous_chaine, sans accumulation `cur = cur + cs[j]`.
    corps_args = {
        "em_corps": ("", "$k, $tx, $tf, $a, $b, $as, $ac, $lg, $al, $ik, $ino, "
                     "$ie, $ie2, $ib1, $ib2, $ilg, $foi, $fot, $fof, $tni, $tnt, "
                     "$tri, $trt, $tc, $bc, $fb, $fl, $flb, $fp, $cc"),
        "rir_corps": ("$pc = ", "$pc, $k, $tx, $tf, $a, $b, $as, $ac, $lg, $al, "
                      "$ik, $ino, $ie, $ie2, $ib1, $ib2, $ilg, $foi, $fot, $fof, "
                      "$sn, $glst, $connus, $ops, $estprincipal, $dans_pour"),
    }
    for fn, (prefix, args) in corps_args.items():
        appel = "em_instr" if fn == "em_corps" else "rir_instr"
        sinon = ("    } sinon { $cur = $cur + $cs[$j] }\n" if fn == "rir_corps"
                 else "    }\n    sinon { $cur = $cur + $cs[$j] }\n")
        old = ("  $cur = \"\"\n  tant_que ($j < $n) {\n"
               "    $c = ordre($cs[$j])\n"
               "    si ($c == 44) {\n"
               "      si ($cur != \"\") {\n"
               f"        {prefix}{appel}(entier($cur), {args})\n"
               "        $cur = \"\"\n      }\n" + sinon +
               "    $j = $j + 1\n  }\n"
               "  si ($cur != \"\") {\n"
               f"    {prefix}{appel}(entier($cur), {args})\n  }}")
        new = ("  $debut = 0\n  tant_que ($j <= $n) {\n"
               "    $c = 10\n    si ($j < $n) { $c = ordre($cs[$j]) }\n"
               "    si ($j == $n ou $c == 44) {\n"
               "      si ($j > $debut) {\n"
               f"        {prefix}{appel}(entier(sous_chaine($cs, $debut, $j)), {args})\n"
               "        $debut = $j + 1\n      }\n"
               "    }\n"
               "    $j = $j + 1\n  }")
        em_head = replace_once(em_head, old, new, f"OPT {fn}")
    # dump RIR : un morceau par ligne, jointure équilibrée (plus de O(n^2)).
    new_tail = replace_once(
        new_tail,
        '$rnat_dump = ""\n$u = 1\ntant_que ($u < longueur($outl)) {\n'
        '  $rnat_dump = $rnat_dump + $outl[$u] + "\\n"\n'
        '  $u = $u + 1\n}\nretour $rnat_dump',
        '$rnat_parts = [""]\n$u = 1\ntant_que ($u < longueur($outl)) {\n'
        '  joindre($rnat_parts, $outl[$u] + "\\n")\n'
        '  $u = $u + 1\n}\n'
        '$rnat_dump = rnat_join($rnat_parts, 1, longueur($rnat_parts))\n'
        'retour $rnat_dump',
        "OPT dump RIR")
    return em_head, em_main, new_tail


def build(backend: str) -> int:
    src, out = BACKENDS[backend]
    override = os.environ.get("RNAT_OUT_ELF" if backend == "elf" else "RNAT_OUT_PE")
    if override:
        out = Path(override)
    phase = os.environ.get("RNAT_PHASE", "")
    if phase and phase not in PHASES:
        raise SystemExit(f"ERREUR RNAT_PHASE inconnu: {phase} (attendu {', '.join(PHASES)})")
    em = lines(EM)
    be = lines(src)
    cfg = trouver_reperes(be, backend)

    assert em[3475].strip() == "}", f"em 3476 inattendu: {em[3475]!r}"
    assert "programme principal" in em[3477], em[3477]
    assert em[3479].strip() == '$src = ""', em[3479]
    assert em[3480] == 'si (nb_arguments() > 0) { $src = lire_texte(argument(0)) }', em[3480]
    assert be[cfg["head_end"] - 1].strip() == "}", be[cfg["head_end"] - 1]
    assert be[cfg["dispatch_start"] - 1] == "si (nb_arguments() >= 1) {", be[cfg["dispatch_start"] - 1]
    assert be[cfg["quit_line"] - 1] == "quitter(0)", be[cfg["quit_line"] - 1]

    # ── front-end : fonction retournant le dump RIR ───────────────────────
    em_head = "\n".join(em[:3476])
    em_main = "\n".join(em[3477:])  # lignes 3478..fin
    old_tail = """si (longueur($err) > 1) { >> $err[1] } sinon {
  $u = 1
  tant_que ($u < longueur($outl)) {
    >> $outl[$u]
    $u = $u + 1
  }
}"""
    new_tail = """si (longueur($err) > 1) { retour $err[1] }
$rnat_dump = ""
$u = 1
tant_que ($u < longueur($outl)) {
  $rnat_dump = $rnat_dump + $outl[$u] + "\\n"
  $u = $u + 1
}
retour $rnat_dump"""
    # Correctif front-end d'abord (copie générée uniquement).
    em_head, em_main, new_tail = injecte_front_opt(em_head, em_main, new_tail)
    if phase == "rir":
        new_tail = '>> "PHASE_RIR_OK"\nquitter(0)\n' + new_tail
    if phase == "dump":
        fin = "retour $rnat_dump"
        new_tail = new_tail.replace(
            fin, '>> "PHASE_DUMP_OK"\nquitter(0)\n' + fin)
    em_main = replace_once(em_main, old_tail, new_tail, "em sortie RIR")
    # Injection des marqueurs de phase (diagnostic uniquement).
    for nom, ancre in PHASE_ANCRES:
        if phase == nom and ancre in em_main:
            em_main = replace_once(
                em_main, ancre,
                f'>> "PHASE_{nom.upper()}_OK"\nquitter(0)\n' + ancre,
                f"phase {nom}")
    # HOST-2 + OCTET_BRUT : injections dans la copie du front-end,
    # avant concaténation (une seule opération, ancre partagée).
    em_head = replace_once(em_head, STD_SIG_ANCRE, STD_SIG_INJECT,
                           "std_sig eval_expr + octet_brut")

    # ── backend : garder globales + fonctions, câbler le RIR ──────────────
    be_head = "\n".join(be[:cfg["head_end"]])
    be_g5 = "\n".join(be[cfg["dispatch_start"] - 1:cfg["quit_line"]])
    be_g5 = replace_once(be_g5, cfg["src_line"], "$src = em_source_rir()", f"{backend} entrée G5")
    be_tail = "\n".join(be[cfg["quit_line"]:])
    # Garde de borne (un seul argument) : absente du PE, présente dans l'ELF.
    # Appliquée à la copie concaténée, jamais à la source possédée.
    if SORTIE_SANS_GARDE in be_tail:
        be_tail = replace_once(be_tail, SORTIE_SANS_GARDE, SORTIE_AVEC_GARDE,
                               f"{backend} garde argument(1)")

    cible = "ELF64 aarch64 (Linux)" if backend == "elf" else "PE32+ aarch64 (Windows)"
    header = "\n".join([
        f"# rose_natif_{backend}.rose — compilateur .rose -> {cible}, généré (NE PAS ÉDITER).",
        "# Généré par scripts/generer_rose_natif.py depuis :",
        "#   compilateur/emetteur.rose      (front-end source -> RIR)",
        f"#   compilateur/emetteur_{backend}.rose  (backend RIR -> binaire G5)",
        f"# Usage : rose rose_natif_{backend}.rose programme.rose > programme.{'elf' if backend == 'elf' else 'exe'}",
        "#",
    ])

    glue = """
# ══════════════════ jonction des deux étages (généré) ══════════════════
# Le front-end est une fonction : ses variables deviennent LOCALES, ce qui
# garde la table des globales G5 sous la borne des 512 entrées.
# Le backend, lui, DOIT rester au niveau supérieur : ses $g5_* sont des
# globales lues par les routines g5_emit_* émises en code machine.
fn em_source_rir() -> chaine {
"""
    parts = [header, em_head, glue, em_main, "\n}\n",
             be_head, be_g5, be_tail]
    out.write_text("\n".join(parts).rstrip() + "\n", encoding="utf-8")
    print(f"généré {out} ({out.stat().st_size} octets)")
    return 0


def main() -> int:
    args = sys.argv[1:]
    if not args:
        # Un diagnostic de phase ne concerne que l'ELF : ne jamais écrire
        # le composite PE livré avec un `quitter(0)` d'instrumentation.
        args = ["elf"] if os.environ.get("RNAT_PHASE") else ["elf", "pe"]
    for a in args:
        if a not in BACKENDS:
            raise SystemExit(f"backend inconnu : {a} (attendu elf|pe)")
    return max(build(a) for a in args)


if __name__ == "__main__":
    sys.exit(main())
