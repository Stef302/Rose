#!/usr/bin/env python3
"""Comparateur de moteurs Rose — parité C++ / Rust / Zig / Python / Julia.

Le moteur C++ est la référence absolue. Ce script écrit lui-même ses
programmes de test (aucune dépendance à des fichiers extérieurs) puis
compare stdout, stderr et code de sortie de chaque moteur disponible.

Niveaux de fidélité attendus :
  * C++    référence (vérificateur + FFI + bundle)
  * Rust   jumeau exact : mêmes messages, mêmes sorties, bundle interopérable
  * Zig    standalone sans vérificateur : programmes valides identiques,
           fonctions externes (@dll/@sym) et modules réservés à C++/Rust
  * Python moteur vivant (pipeline v2) : golden uniquement
  * Julia  moteur vivant pur Julia : golden uniquement

Usage :  python3 parite.py          # tous les moteurs trouvés
         python3 parite.py rust     # un seul moteur comparé au C++
"""
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
MOTEUR = os.path.join(HERE, "..")

PROGRAMMES = [
    # (nom, source, stdin ou None)
    ("arithmétique exacte", ">> 2 ^ 100\n>> 2 ^ 64\n>> 1/3 + 1/6\n>> 7/2 * 2/7\n>> 40!\n>> 100!\n", None),
    ("divisions euclidiennes", ">> 7 / 2\n>> -7 / 2\n>> 7 % 3\n>> -7 % 3\n>> 7 % -3\n>> 7 div 2\n>> -7 div 2\n>> -7.0 div 2\n>> 1/2 div 1/3\n", None),
    ("puissances", ">> 2 ^ -3\n>> (1/2) ^ -2\n>> 9 ^ 0.5\n>> 2 ^ 100000000000000000000\n", None),
    ("puissances flottantes", "$a = -8.0\n$b = 1/3\n>> $a ^ $b\n>> 4.0 ^ 0.5\n>> (0.0 - 2.0) ^ 2.0\n", None),
    ("repli flottant", ">> 2.0 ^ 100000000000000000000\n>> -8.0 ^ (1/3)\n>> 1.5 + 1.5\n", None),
    ("partie entier = plancher", ">> partie_entiere(7/2)\n>> partie_entiere(-7/2)\n>> partie_entiere(-2.5)\n>> partie_entiere(2.5)\n>> partie_entiere(4)\n", None),
    ("flottants machine (%g)", ">> 0.1 + 0.2\n>> 1.5\n>> 2.5\n>> 12.5\n>> 10.25\n>> -2.5\n>> 2.0 ^ 40\n>> 1.0 / (2 ^ 40)\n>> 100.0\n>> 3.0\n>> 0.5\n>> -0.75\n", None),
    ("rationnels périodiques", ">> 1/3\n>> 1/6\n>> 1/7\n>> 22/7\n>> 5/2\n>> -1/3\n", None),
    ("conversions de chaînes", '>> entier("42")\n>> entier("7/2")\n>> entier("-7/2")\n>> entier("-2.9")\n>> flottant("2.5")\n>> flottant("1e3")\n>> chaine(3/2)\n>> chaine(2.5)\n>> chaine(vrai)\n', None),
    ("stdlib", ">> factorielle(5)\n>> fibonacci(90)\n>> pgcd(12, 18)\n>> ppcm(4, 6)\n>> est_premier(97)\n>> est_premier(91)\n>> nb_chiffres(2 ^ 1000)\n>> abs(-5)\n>> abs(-2.5)\n>> abs(1/3 - 1)\n>> min(1/2, 0.6)\n>> max(3, 2.5)\n>> puissance(2, 64)\n>> puissance(1/2, -3)\n>> fraction(3, 4)\n>> nombre_pi(12)\n>> compter_premiers(1000)\n", None),
    ("grosses fonctions", ">> 40!\n>> fibonacci(10000)\n>> compter_premiers(100000)\n>> nombre_pi(200)\n", None),
    ("contrôles", "$x = 0\npour $i de 1 vers 5 { $x = $x + $i }\n>> $x\n$n = 10\ntant_que ($n > 5) { $n = $n - 1 }\n>> $n\n$N = 1\ntant_que ($N < 100000) { $N = $N * 3 }\n>> $N\nsi ($x > 10) { >> \"grand\" } sinon { >> \"petit\" }\n>> vrai et faux\n>> vrai ou faux\n>> non faux\n", None),
    ("si imbriqués sur une ligne", "fn f(n: entier) -> entier { $m = n % 3\n  si ($m == 0) { si (n < 10) { retour 1 }\n    retour 2 }\n  retour n + 100 }\n>> f(0)\n>> f(9)\n>> f(10)\n>> f(11)\n$z = 0\nsi (f(10) > 5) { $z = $z + 1 } si (f(9) > 5) { $z = $z + 10 }\n>> $z\n$u = 0\n$v = 7\nsi ($v < 5) { $u = 1 } sinon { si ($v > 6) { $u = 2 }\n  $u = $u * 10 + 5 }\n>> $u\n", None),
    ("fonctions utilisateur", "fn fact(n: entier) -> entier {\n    si (n <= 1) { retour 1 }\n    retour n * fact(n - 1)\n}\n>> fact(20)\nfn collatz_long(n: entier) -> entier {\n    $c = 0\n    tant_que ($n != 1) {\n        si ($n % 2 == 0) { $n = $n div 2 } sinon { $n = 3 * $n + 1 }\n        $c = $c + 1\n    }\n    retour $c\n}\n>> collatz_long(27)\nfn demi(x: flottant) -> flottant { retour x / 2.0 }\n>> demi(7)\n", None),
    ("grande récursion", "fn compte(n: entier) -> entier {\n    si (n == 0) { retour 0 }\n    retour 1 + compte(n - 1)\n}\n>> compte(20000)\n", None),
    ("listes", "$l = [1, 2, 3]\n>> $l[0]\n>> longueur($l)\njoindre($l, 4)\n>> $l\n>> $l == [1, 2, 3, 4]\n>> [1, 2] != [1, 3]\n>> 1 < 2 et 2 <= 2\n>> \"a\" + \"b\"\n", None),
    ("logique et égalités", ">> 1/3 + 2/3 ?= 1\n>> 0.1 + 0.2 ?= 0.3\n>> 1 == 1.0\n>> 1/2 == 0.5\n>> [1/2] == [1/2]\n>> 1 < 2\n", None),
    ("lecture clavier", "<< $a\n>> $a * 2\n", "21\n"),
    ("types : ajustement de logement", "$q = 1/2\n$q = 3\n>> $q + 1/2\n$w = 2.0\n>> $w\n>> $w + 1\n", None),
    ("variables", "$y = 7\n$y = 5\n>> $y\n$x = 2\n>> $x + 1\n", None),
    ("factorielle postfixée", ">> 0 !\n>> 5!\n>> 3 ! + 1\n", None),
    ("v3 chaînes unicode",
     '>> "escalier"[2]\n>> ordre("a")\n>> ordre("é")\n>> caractere(233)\n'
     '>> sous_chaine("bonjour", 1, 4)\n$p = ["li", "gne", "!"]\njoindre($p, "?")\n'
     '>> concatener($p)\n>> longueur("été")\n>> "été"[1]\n>> sous_chaine("été", 1, 2)\n', None),
    ("v3 quitter code 3", '>> "avant"\nquitter(3)\n>> "jamais"\n', None),
    ("v3 notation symbolique",
     '>> 2 + 3 ⋅ 4\n>> 6 × 7 ÷ 2\n>> 3 ≤ 5 et 5 ≥ 5 et 5 ≠ 4\n>> √9\n>> √2\n>> √(1/4)\n'
     '>> 2³ + 5²\n>> π\n>> π ⋅ 2.0 ²\n$s = "sym"\n>> $s\n>> longueur($s)\n'
     '$s ← "rose"\n>> $s\nfn aire(r: flottant) → flottant { retour π ⋅ r ² }\n>> aire(2.0)\n', None),
    ("v3 mixtes rationnel-flottant",
     ">> 1/4 + 0.5\n>> 1/2 * 0.1\n>> 1/3 - 0.25\n>> 2/3 / 0.5\n>> (1/4) ^ 0.5\n"
     "$a = 1\n>> (a/4) + 0.5\n>> (a/2) * 0.1\n>> (a/3) - 0.25\n>> (a*2/3) / 0.5\n"
     ">> (a/2) ^ 0.5\n>> (a/4) ^ (3/2)\n", None),
    ("v3 tables",
     '$t = table()\n'
     'inserer($t, "nom", "Alice")\n'
     'inserer($t, "age", 30)\n'
     'inserer($t, "nom", "Bob")\n'
     '>> $t\n'
     '>> longueur($t)\n'
     '>> cle_existe($t, "nom")\n'
     '>> cle_existe($t, "x")\n'
     '>> $t["nom"]\n'
     '>> $t["age"]\n'
     '>> supprimer($t, "age")\n'
     '>> $t\n'
     '>> cles($t)\n'
     '>> valeurs($t)\n'
     '$u = table()\n'
     'inserer($u, "nom", "Bob")\n'
     '>> $t == $u\n'
     '>> $t != $u\n', None),
    ("hasard reproductible",
     'graine(42)\n'
     '>> hasard(64)\n'
     '>> hasard(64)\n'
     '>> hasard(1000)\n'
     '>> hasard(1000000000000000000000)\n'
     'graine(42)\n'
     '>> hasard(64)\n', None, {"python", "julia"}),
    ("tampons d'octets",
     '$al = octets("abcXYZ-_")\n'
     '$buf = tampon(8)\n'
     '>> longueur($buf)\n'
     'remplir_hasard($buf, $al)\n'
     '>> longueur($buf)\n'
     'tampon_pose($buf, 0, 65)\n'
     '>> tampon_case($buf, 0)\n'
     '>> octets("abc") == octets("abc")\n'
     '>> octets("abc") == octets("abd")\n'
     '>> longueur(octets("été"))\n'
     '>> chaine(octets("abc"))\n'
     'graine(7)\n'
     '$x1 = tampon(16)\n'
     'remplir_hasard($x1, $al)\n'
     'graine(7)\n'
     '$x2 = tampon(16)\n'
     'remplir_hasard($x2, $al)\n'
     '>> $x1 == $x2\n'
     '>> chaine($x1) != chaine(octets("zzzzzzzzzzzzzzzz"))\n', None, {"python", "julia"}),
    ("remplissage Lemire et mono",
     'graine(5)\n'
     '$al7 = octets("abcdefg")\n'
     '$b3 = tampon(200)\n'
     'remplir_hasard($b3, $al7)\n'
     '$s = 0\n'
     'pour $j de 0 vers 200 { $s = $s + tampon_case($b3, $j) }\n'
     '>> $s\n'
     '$un = octets("x")\n'
     '$bf = tampon(5)\n'
     'remplir_hasard($bf, $un)\n'
     '>> chaine($bf)\n', None, {"python", "julia"}),
    # Flottants JIT (M2.12) : accumulation, comparaisons et appels mixtes.
    ("flottants accumulation",
     '$s = 1.0\n'
     'pour i de 1 vers 8 { $s = $s * 1.1 }\n'
     '>> $s\n'
     '>> $s / 3\n'
     '>> $s < 2.2\n'
     'fn f(x: flottant) -> flottant { retour x * x - 2.0 }\n'
     '>> f(1.5)\n'
     '>> f($s)\n'
     '>> 0.1 + 0.2 == 0.3\n', None),
    ("flottants mixtes entier",
     '$a = 3\n'
     '$b = 0.5\n'
     '>> $a * $b\n'
     '>> $a + $b\n'
     '>> $b - $a\n'
     '>> $a < $b + 3\n'
     'fn echelle(x: flottant, k: entier) -> flottant { retour x * k }\n'
     '>> echelle(2.5, 3)\n'
     '>> echelle($b, $a)\n', None),
    # Builtins numériques (helpers natifs du JIT C++, interpreteurs ailleurs).
    ("builtins numeriques",
     '>> pgcd(48, 36)\n'
     '>> ppcm(4, 6)\n'
     '>> fibonacci(25)\n'
     '>> est_premier(89)\n'
     '>> nb_chiffres(2 ^ 128)\n'
     '$a = 0\n'
     'pour k de 1 vers 100 { $a = $a + pgcd(k, 6) }\n'
     '>> $a\n', None),
    # Listes et chaînes (helpers natifs du JIT C++, interpreteurs ailleurs).
    ("listes et chaines",
     '>> longueur(concatener(["ab", "cd", "ef"]))\n'
     '$l = [1, 2, 3]\n'
     'joindre($l, 4)\n'
     '>> $l[0] + $l[3]\n'
     '>> longueur($l)\n'
     '$t = "alpha"\n'
     '>> concatener(["x", $t])\n', None),
]

ERREURS_RUNTIME = [
    ("division par zéro", ">> 5 % 0\n"),
    ("modulo non-entier", ">> 1 % 0.5\n"),
    ("modulo rationnel", ">> (1/2) % 3\n"),
    ("modulo par zéro", ">> 1 % 0\n"),
    ("factorielle négative", ">> factorielle(-1)\n"),
    ("index hors limites", "$l = [1]\n>> $l[3]\n"),
    ("index chaîne hors limites", '>> "abc"[5]\n'),
    ("ordre non caractère", '>> ordre("ab")\n'),
    ("sous_chaine bornes invalides", '>> sous_chaine("abc", 2, 1)\n'),
    ("lecture fichier absent", '>> lire_texte("absent_xyz_rose.txt")\n'),
    ("tampon non affichable", ">> tampon(4)\n"),
    ("index hors tampon", '$b = tampon(2)\n>> tampon_case($b, 5)\n'),
    ("nom réservé par le moteur", "$t1 = 1\n>> $t1\n"),
    ("chaîne non numérique", '>> entier("zuz")\n'),
    ("exposant démesuré", ">> 2 ^ 100000000000000000000\n"),
    ("zéro puissance négative", ">> 0 ^ -1\n"),
    ("variable sans valeur", ">> $inconnue\n"),
    ("division flottante par zéro", ">> 1 / 0\n"),
    ("notation scientifique interdite", ">> 1e16\n"),
    ("?= unaire interdit", ">> ?= 1\n"),
]


def moteur_cmd(nom):
    if nom == "cpp":
        return [os.path.join(MOTEUR, "cpp", "rose")]
    if nom == "rust":
        return [os.path.join(MOTEUR, "rust", "rose")]
    if nom == "zig":
        return [os.path.join(MOTEUR, "zig", "rose")]
    if nom == "python":
        return [sys.executable, os.path.join(MOTEUR, "python", "engine.py")]
    if nom == "julia":
        jl = os.environ.get("JULIA", "julia")
        if shutil.which(jl) is None:
            return None
        return [jl, "--project=" + os.path.join(MOTEUR, ".."),
                os.path.join(MOTEUR, "julia", "engine.jl")]
    return None


def _monter_pile():
    # Julia compte sur la pile du processus (pas d'option CLI) : 512 Mo
    import resource
    limite_dure = resource.getrlimit(resource.RLIMIT_STACK)[1]
    resource.setrlimit(resource.RLIMIT_STACK,
                       (min(512 * 1024 * 1024, limite_dure if limite_dure != resource.RLIM_INFINITY
                            else 512 * 1024 * 1024), limite_dure))


def lancer(cmd, fichier, stdin=None, augmenter_pile=False):
    try:
        p = subprocess.run(cmd + [fichier], input=stdin,
                           capture_output=True, text=True, timeout=120,
                           preexec_fn=_monter_pile if augmenter_pile else None)
        return (p.returncode, p.stdout, p.stderr)
    except subprocess.TimeoutExpired:
        return (-99, "", " TIMEOUT")


def main():
    cibles = sys.argv[1:] or ["rust", "zig", "python", "julia"]
    ref = moteur_cmd("cpp")
    if ref is None or not os.path.exists(ref[0]):
        print("moteur C++ de référence introuvable — arr.")
        return 1
    tmp = tempfile.mkdtemp(prefix="rose-parite-")
    echecs = {}
    total = 0
    with tempfile.TemporaryDirectory() as td:
        for nom_cible in cibles:
            cmd = moteur_cmd(nom_cible)
            if cmd is None or not os.path.exists(cmd[-1] if cmd[-1].endswith("rose") else cmd[0]):
                print(f"— {nom_cible} : indisponible, ignoré")
                continue
            echecs[nom_cible] = []
            for i, item in enumerate(PROGRAMMES):
                nom, source, stdin = item[0], item[1], item[2]
                sauts = item[3] if len(item) > 3 else set()
                if nom_cible in sauts:
                    continue
                chemin = os.path.join(td, f"p{i:02d}.rose")
                with open(chemin, "w", encoding="utf-8") as f:
                    f.write(source)
                a = lancer(ref, chemin, stdin)
                b = lancer(cmd, chemin, stdin, augmenter_pile=(nom_cible == "julia"))
                total += 1
                if a[0] == b[0] and a[1] == b[1] and a[2] == b[2]:
                    continue
                # Zig / moteurs vivants : messages d'erreur propres acceptés si code égal
                echecs[nom_cible].append((nom, a, b))
            for nom, source in ERREURS_RUNTIME:
                if nom_cible in ("zig", "python", "julia"):
                    continue  # vérificateur réservé C++/Rust
                chemin = os.path.join(td, "err.rose")
                with open(chemin, "w", encoding="utf-8") as f:
                    f.write(source)
                a = lancer(ref, chemin)
                b = lancer(cmd, chemin)
                total += 1
                if a != b:
                    echecs[nom_cible].append(("erreur " + nom, a, b))
            n = len(echecs[nom_cible])
            etat = "PARFAIT" if n == 0 else f"{n} divergence(s)"
            print(f"— {nom_cible:7s} : {etat}")
            for nom, a, b in echecs[nom_cible]:
                print(f"    · {nom}")
                if a[1] != b[1]:
                    d1 = "\n      ".join(map(lambda l: repr(l),
                                             _diff_lignes(a[1], b[1])))
                    print(f"      stdout diverge :\n      {d1}")
                elif a[2] != b[2]:
                    print(f"      stderr : attendu {a[2].strip()!r}, reçu {b[2].strip()!r}")
                else:
                    print(f"      code : attendu {a[0]}, reçu {b[0]}")
    print(f"\nParité : {total} comparaisons, "
          f"{sum(len(v) for v in echecs.values())} divergences")
    return 0 if all(not v for v in echecs.values()) else 1


def _diff_lignes(a, b):
    la, lb = a.splitlines(), b.splitlines()
    out = []
    for i in range(max(len(la), len(lb))):
        x = la[i] if i < len(la) else "<absent>"
        y = lb[i] if i < len(lb) else "<absent>"
        if x != y:
            out.append(f"attendu {x!r} reçu {y!r}")
    return out[:4]


if __name__ == "__main__":
    sys.exit(main())
