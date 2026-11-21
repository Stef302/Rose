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


def build(backend: str) -> int:
    src, out = BACKENDS[backend]
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
    em_main = replace_once(em_main, old_tail, new_tail, "em sortie RIR")
    # HOST-2 : injection dans la copie du front-end, avant concaténation.
    em_head = replace_once(em_head, STD_SIG_ANCRE, STD_SIG_EVAL_EXPR,
                           "std_sig eval_expr")

    # ── backend : garder globales + fonctions, câbler le RIR ──────────────
    be_head = "\n".join(be[:cfg["head_end"]])
    be_g5 = "\n".join(be[cfg["dispatch_start"] - 1:cfg["quit_line"]])
    be_g5 = replace_once(be_g5, cfg["src_line"], "$src = em_source_rir()", f"{backend} entrée G5")
    be_tail = "\n".join(be[cfg["quit_line"]:])

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
    args = sys.argv[1:] or ["elf", "pe"]
    for a in args:
        if a not in BACKENDS:
            raise SystemExit(f"backend inconnu : {a} (attendu elf|pe)")
    return max(build(a) for a in args)


if __name__ == "__main__":
    sys.exit(main())
