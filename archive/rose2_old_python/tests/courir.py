#!/usr/bin/env python3
"""Suite de tests du compilateur Rose v2.

Chaque cas : (nom, source, {cible: sortie_attendue} ou {cible: ("ERREUR", motif)}).
Sorties attendues validées par exécution sur la machine de développement.

Usage :  python3 tests/courir.py [cible ...]
"""
from __future__ import annotations
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))
try:
    sys.set_int_max_str_digits(200000000)
except Exception:
    pass

from rose2.driver import Outillage, compiler, construire, executer  # noqa: E402
from rose2.lexer import ErreurRose  # noqa: E402

TOUTES = ["c", "python", "julia", "rust", "zig"]

# programmes exacts, comparables entre cibles grande précision
EXACTS = [
    ("division exacte", ">> 1 / 2 + 1 / 3\n", "0.8[3]\n"),
    ("factorielle", ">> 20!\n", "2432902008176640000\n"),
    ("puissance exacte", ">> 2 ^ 64\n", "18446744073709551616\n"),
    ("fibonacci", ">> fibonacci(200)\n", "280571172992510140037611932413038677189525\n"),
    ("pgcd", ">> pgcd(1071, 462)\n", "21\n"),
    ("ppcm", ">> ppcm(4, 6)\n", "12\n"),
    ("exposant négatif → rationnel", ">> 10 ^ -2\n", "0.01\n"),
    ("puissance droite-assoc", ">> 2^3^2\n", "512\n"),
    ("factorielle postfixe", ">> 3! + 4!\n", "30\n"),
    ("modulo euclidien", ">> 7 % 3\n>> -7 % 3\n", "1\n2\n"),
    ("div entier", ">> 17 div 5\n>> -17 div 5\n", "3\n-4\n"),
    ("div flottant", ">> 7.0 div 2.0\n", "3\n"),
    ("nb_chiffres", ">> nb_chiffres(factorielle(1000))\n", "2568\n"),
    ("premiers", ">> est_premier(97)\n>> est_premier(91)\n", "vrai\nfaux\n"),
    ("crible", ">> compter_premiers(1000)\n", "168\n"),
    ("chaînes", '>> "bon" + "jour"\n>> "abc" < "abd"\n', "bonjour\nvrai\n"),
    ("listes", "$l = [1, 2, 3]\njoindre($l, 4)\n>> $l\n>> $l[2]\n>> longueur($l)\n",
     "[1, 2, 3, 4]\n3\n4\n"),
    ("sucre si", "$x = 7\n? ($x > 5) [>> \"grand\"]\n", "grand\n"),
    ("si/sinon", "si (2 > 3) { >> 1 } sinon { >> 2 }\n", "2\n"),
    ("tant_que", "$n = 1\ntant_que ($n < 100) { $n = $n * 2 }\n>> $n\n", "128\n"),
    ("pour", "$t = 0\npour k de 1 vers 11 { $t = $t + k }\n>> $t\n", "55\n"),
    ("fonction + retour", "fn f(n: entier) -> entier { retour n * n }\n>> f(9)\n", "81\n"),
    ("récursion", "fn fact(n: entier) -> entier {\n    si (n <= 1) { retour 1 }\n"
     "    retour n * fact(n - 1)\n}\n>> fact(10)\n", "3628800\n"),
    ("π", ">> nombre_pi(20)\n", "3.14159265358979323846\n"),
    ("π 100 chiffres", ">> nombre_pi(100)\n",
     "3.1415926535897932384626433832795028841971693993751058209749445923078164062862089986280348253421170680\n"),
    ("invoquation Unicode", ">> 13 ≤ 20\n>> 7 ≠ 9\n>> 4 ≥ 4\n", "vrai\nvrai\nvrai\n"),
    ("différent", ">> 5 != 9\n>> 5 ~= 5\n", "vrai\nvrai\n"),
    ("non", ">> non (3 > 5)\n", "vrai\n"),
    ("et/ou", ">> vrai et faux\n>> vrai ou faux\n", "faux\nvrai\n"),
    ("conversion", ">> entier(3.9)\n>> partie_entiere(22 / 7)\n>> abs(-13)\n", "3\n3\n13\n"),
    ("min/max", ">> min(3, 7)\n>> max(2.5, 2.7)\n", "3\n2.7\n"),
    ("fraction exacte", ">> fraction(1,3) + fraction(1,6)\n", "0.5\n"),
    ("chaine(rationnel)", ">> chaine(fraction(22, 7))\n", "22/7\n"),
]

# programmes machine (i64/f64) — toutes les cinq cibles
MACHINE = [
    ("arithmétique", ">> 3 + 5\n>> 20!\n>> 2 ^ 40\n",
     "8\n2432902008176640000\n1099511627776\n"),
    ("fibonacci machine", ">> fibonacci(90)\n", "2880067194370816120\n"),
    ("collatz", "fn c(n: entier) -> entier {\n    $k = 0\n"
     "    tant_que ($n != 1) {\n        si ($n % 2 == 0) { $n = $n div 2 } "
     "sinon { $n = 3 * $n + 1 }\n        $k = $k + 1\n    }\n    retour $k\n}\n>> c(27)\n", "111\n"),
    ("flottants", ">> 2.5 * 4.0\n>> 10.0 / 4.0\n>> 2 ^ 0.5 ≈ 1.414213562\n", "10\n2.5\nvrai\n"),
    ("premiers machine", ">> est_premier(233)\n>> est_premier(561)\n", "vrai\nfaux\n"),
]

# lectures stdin (entrée, sortie) — entier/flottant sur les 5 cibles,
# rationnel sur C/Python/Julia seulement
LECTURE = [
    ("lecture entier", "<< $n\n>> $n * $n\n", "7\n", "49\n", TOUTES),
    ("lecture truncature", "<< $n\n>> $n\n", "2.25\n", "2\n", TOUTES),
    ("lecture flottant pré-déclaré", "$d = 0.0\n<< $d\n>> $d + 0.5\n", "2.25\n", "2.75\n", TOUTES),
    ("lecture rationnel", "$q = fraction(1, 3)\n<< $q\n>> $q\n", "2/3\n", "0.[6]\n",
     ["c", "python", "julia"]),
]

# refus propres (message d'erreur, pas de plantage ni troncature silencieuse)
REFUS = [
    ("rationnel hors machine", ">> 1 / 2\n", ("rust", "cible")),
    ("chaîne hors machine", '>> "x"\n', ("rust", "cible")),
    ("liste hors machine", ">> [1, 2][0]\n", ("zig", "cible")),
    ("dépassement i64", ">> 2 ^ 63\n", ("rust", "i64")),
    ("dépassement i64 zig", ">> 2 ^ 63\n", ("zig", "i64")),
]


def lancer(source: str, cible: str, outillage: Outillage,
           stdin_data: str = "") -> tuple[str, str | None]:
    """Compile et exécute ; retourne (stdout, message_rose_si_erreur)."""
    import subprocess
    import tempfile
    dossier = Path(tempfile.mkdtemp(prefix="rose_test_"))
    fichier = dossier / "t.rose"
    fichier.write_text(source, encoding="utf-8")
    sortie = dossier / ("t.exe" if cible in ("c", "rust", "zig") else "t")
    try:
        chemin, _ = construire(fichier, cible, outillage, sortie)
        if cible == "c":
            cmd = [str(chemin.resolve())]
        elif cible in ("rust", "zig"):
            cmd = [str(chemin.resolve())]
        elif cible == "python":
            cmd = [outillage.python, str(chemin)]
        else:
            cmd = [outillage.julia, "--startup=no", "--quiet", str(chemin)]
        try:
            proc = subprocess.run(cmd, capture_output=True, text=True, timeout=120,
                                  input=stdin_data)
        except subprocess.TimeoutExpired:
            return "", "bout du temps (boucle infinie ?)"
        texte = proc.stdout
        if proc.returncode != 0:
            last = (proc.stderr.strip().splitlines() or ["code " + str(proc.returncode)])[-1]
            return texte, last
        return texte, None
    except ErreurRose as e:
        return "", str(e)


def main() -> int:
    cibles = sys.argv[1:] or TOUTES
    outillage = Outillage()
    reussis, echoues = 0, 0
    for groupe, cas_cibles in (("exacts", [(n, s, {c: o for c in ("c", "python", "julia")}
                                            ) for n, s, o in EXACTS]),
                               ("machine", [(n, s, {c: o for c in TOUTES}) for n, s, o in MACHINE]),
                               ("refus", [(f"{n}", s, {c: e for (c, e) in [(k, v)]})
                                          for n, s, (k, v) in REFUS])):
        for nom, source, attendus in cas_cibles:
            for cible, attendu in attendus.items():
                if cible not in cibles or not outillage.disponible(cible):
                    continue
                if groupe == "refus":
                    cible_refus, motif = cible, attendu
                    _, message = lancer(source, cible_refus, outillage)
                    ok = message is not None and motif in message.lower() or \
                        (message is not None and motif in message)
                    if ok:
                        reussis += 1
                    else:
                        echoues += 1
                        print(f"ÉCHEC refus [{nom} → {cible_refus}] : message attendu "
                              f"contenant « {motif} », reçu : {message!r}")
                    continue
                texte, message = lancer(source, cible, outillage)
                if message is not None:
                    echoues += 1
                    print(f"ÉCHEC [{nom} → {cible}] : erreur {message!r}")
                elif texte != attendu:
                    echoues += 1
                    print(f"ÉCHEC [{nom} → {cible}] : attendu {attendu!r}, reçu {texte!r}")
                else:
                    reussis += 1
    for nom, source, entree, attendu, cibles_ok in LECTURE:
        for cible in cibles_ok:
            if cible not in cibles or not outillage.disponible(cible):
                continue
            texte, message = lancer(source, cible, outillage, entree)
            if message is not None:
                echoues += 1
                print(f"ÉCHEC lecture [{nom} → {cible}] : erreur {message!r}")
            elif texte != attendu:
                echoues += 1
                print(f"ÉCHEC lecture [{nom} → {cible}] : attendu {attendu!r}, reçu {texte!r}")
            else:
                reussis += 1
    print(f"\nTests : {reussis} réussis, {echoues} échoués")
    return 1 if echoues else 0


if __name__ == "__main__":
    sys.exit(main())
