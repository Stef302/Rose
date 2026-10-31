"""Rose v2 — conducteur : détecte les toolchains, compile et exécute.

Usage :
    rose run programme.rose [--cible c|python|julia|rust|zig]
    rose build programme.rose [-o sortie] [--cible c] [--opt 2] [--cc gcc|clang|zig]
    rose emettre programme.rose --cible c
    rose info
    rose version
"""
from __future__ import annotations
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:                       # grands entiers : repli de constantes, affichage, conversions
    sys.set_int_max_str_digits(200000000)
except Exception:
    pass

from .lexer import ErreurRose, tokeniser
from .parser import analyser
from .checker import Verificateur, replier_constants
from .codegen_c import generer_c
from .codegen_python import generer_python
from .codegen_julia import generer_julia
from .codegen_rust import generer_rust
from .codegen_zig import generer_zig

VERSION = "2.0.0"

# capacités refusées par cible (outre les refus déjà levés par les codegens)
REFUS = {
    "rust": {"rationnel", "chaine", "liste", "dynamique"},
    "zig": {"rationnel", "chaine", "liste", "dynamique"},
}
NOMS_CIBLES = {
    "c": "C (GMP)", "python": "Python 3", "julia": "Julia", "rust": "Rust", "zig": "Zig",
}


def _trouver(outils: list[str]) -> str | None:
    for o in outils:
        chemin = shutil.which(o)
        if chemin:
            return chemin
    return None


class Outillage:
    def __init__(self):
        env = os.environ
        racine = Path("/tmp/opencode/toolchain")
        self.cc = env.get("ROSE_CC") or _trouver(["gcc", "clang"])
        self.gmp_dir = self._detecter_gmp(env, racine)
        self.python = env.get("ROSE_PYTHON") or _trouver(["python3", "python"])
        self.julia = env.get("ROSE_JULIA") or _trouver(["julia"]) \
            or str(racine / "julia-1.10.4/bin/julia")
        self.rustc = env.get("ROSE_RUSTC") or _trouver(["rustc"]) \
            or str(Path.home() / ".cargo/bin/rustc")
        self.zig = env.get("ROSE_ZIG") or _trouver(["zig"]) or self._zig_tel(racine)
        for p in (self.julia, self.rustc, self.zig):
            if p and not Path(p).exists():
                if p == self.julia: self.julia = None
                if p == self.rustc: self.rustc = None
                if p == self.zig: self.zig = None

    @staticmethod
    def _zig_tel(racine: Path) -> str | None:
        for dossier in sorted(racine.glob("zig-*")):
            cand = dossier / "zig"
            if cand.exists():
                return str(cand)
        return None

    @staticmethod
    def _detecter_gmp(env, racine: Path) -> str | None:
        candidats = []
        if env.get("ROSE_GMP_DIR"):
            candidats.append(Path(env["ROSE_GMP_DIR"]))
        candidats.append(racine / "gmp-install")
        for c in candidats:
            if (c / "include" / "gmp.h").exists():
                return str(c)
        if Path("/usr/include/gmp.h").exists():
            return "/usr"
        if Path("/usr/include/aarch64-linux-gnu/gmp.h").exists() or \
           Path("/usr/lib/aarch64-linux-gnu/libgmp.a").exists() or \
           Path("/usr/lib/x86_64-linux-gnu/libgmp.a").exists():
            return "/usr"
        return None

    def disponible(self, cible: str) -> bool:
        return {"c": bool(self.cc and self.gmp_dir), "python": bool(self.python),
                "julia": bool(self.julia), "rust": bool(self.rustc),
                "zig": bool(self.zig)}[cible]

    def etat(self) -> dict:
        return {cible: self.disponible(cible) for cible in NOMS_CIBLES}


def compiler(source: str, cible: str, outillage: Outillage, optimiser: int = 2):
    """Analyse, vérifie, génère. Retourne (code_source, bibliothèques, extensions_liens)."""
    prog = analyser(source)
    replier_constants(prog)
    verif = Verificateur(prog)
    verif.verifier()
    refus = REFUS.get(cible, set())
    blocks = verif.fonctionnalites & refus
    if blocks:
        raise ErreurRose(
            f"la cible {NOMS_CIBLES[cible]} ne supporte pas : "
            + ", ".join(sorted(blocks))
            + " — utilise la cible C (GMP), Python ou Julia")
    if cible == "c":
        if not outillage.gmp_dir:
            raise ErreurRose("la cible C requiert GMP (installe GMP ou définit ROSE_GMP_DIR)")
        return generer_c(prog, verif.globales)
    if cible == "python":
        return generer_python(prog), set()
    if cible == "julia":
        return generer_julia(prog), set()
    if cible == "rust":
        return generer_rust(prog), set()
    if cible == "zig":
        return generer_zig(prog), set()
    raise ErreurRose(f"cible inconnue {cible!r}")


def _nom_sortie(fichier: Path, cible: str) -> Path:
    exe = {"c": "", "rust": "", "zig": ""}.get(cible)
    if exe is None:
        return fichier.with_suffix({"python": ".py", "julia": ".jl"}[cible])
    return fichier.with_suffix("")


def construire(fichier: Path, cible: str, outillage: Outillage, sortie: Path | None = None,
               optimiser: int = 2, cc: str | None = None) -> tuple[Path, str]:
    """Compile vers un exécutable (C/Rust/Zig) ou un fichier source (Python/Julia).

    Retourne (chemin_exécutable_ou_source, dossier_temporaire_à_conserver)."""
    source = fichier.read_text(encoding="utf-8")
    code, bibliotheques = compiler(source, cible, outillage, optimiser)
    sortie = sortie or _nom_sortie(fichier, cible)
    dossier = Path(tempfile.mkdtemp(prefix="rose2_"))
    if cible == "c":
        cc = cc or outillage.cc
        if cc == "zig":
            compil = [outillage.zig, "cc"]
        else:
            compil = [cc]
        flags = ["-O2" if optimiser >= 2 else "-O1" if optimiser == 1 else "-O0",
                 "-std=gnu11"]
        if outillage.gmp_dir != "/usr":
            flags += [f"-I{outillage.gmp_dir}/include"]
        cible_f = dossier / "programme.c"
        cible_f.write_text(code, encoding="utf-8")
        liens = []
        if outillage.gmp_dir != "/usr":
            liens.append(f"{outillage.gmp_dir}/lib/libgmp.a")
            liens.append(f"-L{outillage.gmp_dir}/lib")
        else:
            liens.append("-lgmp")
        for b in sorted(bibliotheques):
            liens.append(b if "/" in b or b.endswith((".so", ".a", ".dylib")) else f"-l{b}")
            if "/" in b or b.endswith((".so", ".a", ".dylib")):
                liens[-1] = b
        cmd = compil + flags + [str(cible_f), "-o", str(sortie)] + liens + ["-lm"]
        _lancer(cmd, f"compilation {cc}")
        return sortie, ""
    if cible == "python":
        (sortie if sortie.suffix == ".py" else sortie.with_suffix(".py")).write_text(
            code, encoding="utf-8")
        return (sortie if sortie.suffix == ".py" else sortie.with_suffix(".py")), ""
    if cible == "julia":
        chemin = sortie if sortie.suffix == ".jl" else sortie.with_suffix(".jl")
        chemin.write_text(code, encoding="utf-8")
        return chemin, ""
    if cible == "rust":
        rs = dossier / "programme.rs"
        rs.write_text(code, encoding="utf-8")
        cmd = [outillage.rustc, "-O", str(rs), "-o", str(sortie)]
        for b in sorted(bibliotheques):
            cmd += ["-l", f"dylib={b}"]
        _lancer(cmd, "compilation rustc")
        return sortie, ""
    if cible == "zig":
        zz = dossier / "programme.zig"
        zz.write_text(code, encoding="utf-8")
        cmd = [outillage.zig, "build-exe", str(zz), "-O", "ReleaseFast",
               "--name", sortie.stem, "-lc", "-lm"]
        for b in sorted(bibliotheques):
            cmd += ["-l", b]
        _lancer(cmd, "compilation zig", cwd=str(dossier))
        genere = dossier / sortie.stem
        if genere.exists() and genere.resolve() != Path(sortie).resolve():
            shutil.copy(genere, sortie)
        return sortie, ""
    raise ErreurRose(f"cible inconnue {cible!r}")


def _lancer(cmd: list[str], quoi: str, cwd: str | None = None):
    try:
        proc = subprocess.run([str(c) for c in cmd], capture_output=True, text=True, cwd=cwd)
    except FileNotFoundError:
        raise ErreurRose(f"outil introuvable : {cmd[0]} ({quoi})")
    if proc.returncode != 0:
        raise ErreurRose(f"{quoi} a échoué :\n{proc.stderr.strip()[:4000]}")
    return proc


def executer(chemin: Path, cible: str, outillage: Outillage, entree: str | None = None) -> int:
    chemin = Path(chemin).resolve()
    if cible in ("c", "rust", "zig"):
        proc = subprocess.run([str(chemin)], input=entree, text=True)
        return proc.returncode
    if cible == "python":
        proc = subprocess.run([outillage.python, str(chemin)], input=entree, text=True)
        return proc.returncode
    if cible == "julia":
        proc = subprocess.run([outillage.julia, "--startup=no", "--quiet", str(chemin)],
                              input=entree, text=True)
        return proc.returncode
    raise ErreurRose(f"cible inconnue {cible!r}")


def emettre(fichier: Path, cible: str, outillage: Outillage) -> str:
    source = fichier.read_text(encoding="utf-8")
    code, _ = compiler(source, cible, outillage)
    return code


def main(argv=None) -> int:
    parseur = argparse.ArgumentParser(prog="rose", description="Compilateur Rose v2 — langage de mathématiques multi-cibles")
    parseur.add_argument("action", choices=["run", "build", "emettre", "info", "version"])
    parseur.add_argument("fichier", nargs="?")
    parseur.add_argument("--cible", "-t", default="c", choices=list(NOMS_CIBLES))
    parseur.add_argument("-o", "--sortie")
    parseur.add_argument("--opt", type=int, default=2)
    parseur.add_argument("--cc", default=None)
    parseur.add_argument("--stdin", default=None, help="texte envoyé à stdin")
    args = parseur.parse_args(argv)

    if args.action == "version":
        print(f"Rose {VERSION}")
        return 0
    outillage = Outillage()
    if args.action == "info":
        print(f"Rose {VERSION} — état des toolchains")
        for cible, nom in NOMS_CIBLES.items():
            etat = "OK" if outillage.disponible(cible) else "ABSENTE"
            print(f"  {nom:<12} {etat}")
        print(f"  GMP          {'OK (' + outillage.gmp_dir + ')' if outillage.gmp_dir else 'ABSENT'}")
        return 0
    if not args.fichier:
        print(f"rose: un fichier .rose est requis (voir `rose --help`)", file=sys.stderr)
        return 2
    fichier = Path(args.fichier)
    if not fichier.exists():
        print(f"rose: fichier introuvable : {fichier}", file=sys.stderr)
        return 2
    if not outillage.disponible(args.cible):
        print(f"rose: la cible {NOMS_CIBLES[args.cible]} n'est pas disponible sur cette machine"
              f" (voir `rose info`)", file=sys.stderr)
        return 2
    try:
        if args.action == "emettre":
            sys.stdout.write(emettre(fichier, args.cible, outillage))
            return 0
        sortie = Path(args.sortie) if args.sortie else None
        chemin, _ = construire(fichier, args.cible, outillage, sortie, args.opt, args.cc)
        if args.action == "build":
            print(f"  {chemin}")
            return 0
        return executer(chemin, args.cible, outillage, args.stdin)
    except ErreurRose as err:
        print(f"rose: {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
