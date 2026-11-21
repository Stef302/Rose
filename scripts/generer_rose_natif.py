#!/usr/bin/env python3
"""Génère les compilateurs natifs composites .rose -> ELF / PE (auto-hébergés).

Composition DÉTERMINISTE de deux sources Rose déjà validées :
  * compilateur/emetteur.rose      : front-end source -> dump RIR (oracle `rose rir`)
  * compilateur/emetteur_elf.rose  : backend RIR -> ELF64 aarch64 (machine G5)
  * compilateur/emetteur_pe.rose   : backend RIR -> PE32+ aarch64 (machine G5)

Aucune de ces sources n'est modifiée. Les fichiers générés sont des
programmes Rose autonomes :
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
"""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parent.parent
EM = ROOT / "compilateur" / "emetteur.rose"
BACKENDS = {
    "elf": (ROOT / "compilateur" / "emetteur_elf.rose",
            ROOT / "compilateur" / "rose_natif.rose",
            dict(head_end=3654, dispatch_start=3658, quit_line=3811,
                 src_line="$src = lire_texte(argument(0))")),
    "pe": (ROOT / "compilateur" / "emetteur_pe.rose",
           ROOT / "compilateur" / "rose_natif_pe.rose",
           dict(head_end=1480, dispatch_start=1484, quit_line=1589,
                src_line="$src = lire_texte(argument(0))")),
}


def lines(p: Path):
    return p.read_text(encoding="utf-8").splitlines()


def replace_once(text: str, old: str, new: str, what: str) -> str:
    n = text.count(old)
    if n != 1:
        raise SystemExit(f"ERREUR {what}: attendu 1 occurrence, trouvé {n}")
    return text.replace(old, new, 1)


def build(backend: str) -> int:
    src, out, cfg = BACKENDS[backend]
    em = lines(EM)
    be = lines(src)

    assert em[3475].strip() == "}", f"em 3476 inattendu: {em[3475]!r}"
    assert "programme principal" in em[3477], em[3477]
    assert em[3479].strip() == '$src = ""', em[3479]
    assert em[3480] == 'si (nb_arguments() > 0) { $src = lire_texte(argument(0)) }', em[3480]
    assert be[cfg["head_end"] - 1].strip() == "}", be[cfg["head_end"] - 1]
    assert be[cfg["dispatch_start"] - 1] == "si (nb_arguments() >= 1) {", be[cfg["dispatch_start"] - 1]
    assert be[cfg["quit_line"] - 1] == "quitter(0)", be[cfg["quit_line"] - 1]

    # ── front-end : fonction retournant le dump RIR ───────────────────────
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
# Le front-end est une fonction : ses variables deviennent des slots locaux,
# ce qui garde la table des globales G5 sous la borne des 512 entrées.
# Le backend, lui, DOIT rester au niveau supérieur : ses $g5_* sont des
# globales lues par les routines g5_emit_* émises en code machine.
fn em_source_rir() -> chaine {
"""
    parts = [header, "\n".join(em[:3476]), glue, em_main, "\n}\n",
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
