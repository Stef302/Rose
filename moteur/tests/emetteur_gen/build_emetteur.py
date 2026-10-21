#!/usr/bin/env python3
"""build_emetteur.py — construit rose/emetteur.rose à partir de
rose/verifieur.rose : patches ciblés (assert sur le nombre d'occurrences)
+ module d'émission RIR inséré avant le programme principal."""
import re, sys

SRC = "/home/clausake/Stefan/ROSE/rose/verifieur.rose"
DST = "/home/clausake/Stefan/ROSE/rose/emetteur.rose"

src = open(SRC).read()

def rep(old, new, count):
    global src
    n = src.count(old)
    assert n == count, f"motif {old[:70]!r} : {n} occurrences, attendu {count}"
    src = src.replace(old, new)

# ─── P0 : en-têtes ───
rep("# verifieur.rose — jalon M4.2 : le vérificateur auto-hébergé ROSE.",
    "# emetteur.rose — jalon M4.3 : l'émetteur RIR auto-hébergé ROSE.", 1)

rep("""# Lit le programme passé en argument et reproduit la chaîne du moteur :
# lexage, analyse descendante (AST en pools de nœuds plats), repli de
# constantes, vérification de types. Affiche « vérifié » si le vérificateur
# accepte ; sinon recopie le message exact de l'oracle `rose verif`
# (« Rose : erreur — [Ligne L, Colonne 0] message »). Validation :
# moteur/tests/verifieur.py — 49 fichiers acceptés + 51 rejets de
# moteur/tests/verifs/, identiques à l'oracle octet pour octet.""",
    """# Lit le programme passé en argument et rejoue TOUTE la chaîne du moteur :
# lexage, analyse descendante (AST en pools de nœuds plats), repli de
# constantes (les flottants repliés portent le texte exact que le moteur
# écrit dans le RIR), vérification de types — enrichie de deux notes :
# le type vérifié de chaque nœud et le type de lecture de chaque « lire » —
# puis ÉMISSION : le dump RIR octet pour octet de l'oracle `rose rir`.
# Rejet : le message exact, comme verifieur.rose. Validation :
# moteur/tests/emetteur.py — les 103 fichiers de verifieur.py plus
# l'émetteur lui-même, par les trois voies.""", 1)

# ─── P1 : paragraphe du repli (la marque F devient texte exact) ───
rep("""# La vue d'un nœud vaut « 1|texte|drapeau » si le nœud est (ou devient) un
# littéral après repli, « 0 » sinon. Les flottants repliés portent la marque
# opaque « F » : tout calcul flottant fini reste flottant, /0 flottant ne
# lève rien (miroir du nullptr), NaN/∞ non plus — le verdict est identique.""",
    """# La vue d'un nœud vaut « 1|texte|drapeau » si le nœud est (ou devient) un
# littéral après repli, « 0 » sinon. Les flottants repliés portent le texte
# exact de texte_flottant : la représentation la plus courte qui se relit à
# l'identique, « .0 » ajouté si elle n'a ni point ni exposant. flot_txt le
# reconstruit depuis chaine() — même repr, sauf entiers flottants où chaine
# tronque, et le signe du zéro se relit dans 1.0 / v.
# /0 flottant ne plie pas (miroir du nullptr) ; NaN/∞ non plus.""", 1)

# ─── P2 : check_expr devient ce_raw ───
rep("fn check_expr(", "fn ce_raw(", 1)

# ─── P3 : signatures — la famille « -> chaine » (ce_raw, check_bin, check_appel) ───
NOTES_P = ("$err: liste<chaine>, $tni: liste<entier>, $tnt: liste<chaine>,\n"
           "              $tri: liste<entier>, $trt: liste<chaine>) -> chaine {")
for fnname in ["ce_raw", "check_bin", "check_appel"]:
    m = re.search(rf"fn {fnname}\([^{{]+\) -> chaine \{{", src)
    assert m, fnname
    seg = m.group(0)
    old_tail = "$err: liste<chaine>) -> chaine {"
    assert seg.count(old_tail) == 1, fnname
    indent = " " * (13 if fnname == "check_bin" else (14 if fnname == "ce_raw" else 15))
    new_tail = ("$err: liste<chaine>, $tni: liste<entier>, $tnt: liste<chaine>,\n"
                + indent + "$tri: liste<entier>, $trt: liste<chaine>) -> chaine {")
    src = src[:m.start()] + seg.replace(old_tail, new_tail) + src[m.end():]

# check_instr & parcourir (2 signatures identiques en queue)
rep("$ass: entier, $ra: chaine, $err: liste<chaine>) {",
    ("$ass: entier, $ra: chaine, $err: liste<chaine>,\n"
     "               $tni: liste<entier>, $tnt: liste<chaine>,\n"
     "               $tri: liste<entier>, $trt: liste<chaine>) {"), 2)

# ─── P4 : sites d'appel dans la région des passes (exclut les repli_*) ───
i1 = src.index("fn ce_raw(")
i2 = src.index("# comparution lexicale")
region = src[i1:i2]
A_OLD = "$foi, $fot, $fof, $err)"
A_NEW = "$foi, $fot, $fof, $err, $tni, $tnt, $tri, $trt)"
B_OLD = "$foi, $fot, $fof, $ass, $ra, $err)"
B_NEW = "$foi, $fot, $fof, $ass, $ra, $err, $tni, $tnt, $tri, $trt)"
na, nb = region.count(A_OLD), region.count(B_OLD)
assert na >= 20 and nb >= 5, (na, nb)
region = region.replace(B_OLD, B_NEW).replace(A_OLD, A_NEW)
src = src[:i1] + region + src[i2:]

# vargs (auxiliaire de check_appel, dans la région) charrie aussi les notes
rep("$foi: liste<entier>, $fot: liste<chaine>, $fof: liste<chaine>,\n         $err: liste<chaine>) {",
    ("$foi: liste<entier>, $fot: liste<chaine>, $fof: liste<chaine>,\n"
     "         $err: liste<chaine>, $tni: liste<entier>, $tnt: liste<chaine>,\n"
     "         $tri: liste<entier>, $trt: liste<chaine>) {"), 1)

# ─── P6 : appels de parcourir dans le main ───
rep("$fnoms, $fparams, $fret, $foi, $fot, $fof, 1, $fret[$ix], $err)",
    "$fnoms, $fparams, $fret, $foi, $fot, $fof, 1, $fret[$ix], $err, $tni, $tnt, $tri, $trt)", 1)
rep("$fnoms, $fparams, $fret, $foi, $fot, $fof, 0, \"vide|vide\", $err)",
    "$fnoms, $fparams, $fret, $foi, $fot, $fof, 0, \"vide|vide\", $err, $tni, $tnt, $tri, $trt)", 1)

# ─── P7 : initialisations des notes ───
rep("$err = [\"\"]",
    "$err = [\"\"]\n$tni = [0]\n$tnt = [\"\"]\n$tri = [0]\n$trt = [\"\"]", 1)

# ─── P8 : note du type de lecture dans check_instr ───
rep("""  si ($kd == "lire") {
    $nom = $ino[$i]
    $t = env_get($nom, $vn, $vt)
    si ($t == "") {
      reserver($nom, $lg0, $err)
      env_put($nom, "entier|vide", $vn, $vt)
    }
    sinon {
      $bb = BASE($t)
      si ($bb != "entier" et $bb != "flottant" et $bb != "rationnel") {
        lever($lg0, "Ne peut pas lire dans une variable " + nomty($t) + " (entier/flottant attendus)", $err)
      }
    }
    retour
  }""",
    """  si ($kd == "lire") {
    $nom = $ino[$i]
    $t = env_get($nom, $vn, $vt)
    si ($t == "") {
      reserver($nom, $lg0, $err)
      env_put($nom, "entier|vide", $vn, $vt)
      si (longueur($err) == 1) { joindre($tri, $i); joindre($trt, "1") }
    }
    sinon {
      $bb = BASE($t)
      si ($bb != "entier" et $bb != "flottant" et $bb != "rationnel") {
        lever($lg0, "Ne peut pas lire dans une variable " + nomty($t) + " (entier/flottant attendus)", $err)
      }
      sinon {
        $nu = "1"
        si ($bb == "rationnel") { $nu = "2" }
        sinon si ($bb == "flottant") { $nu = "3" }
        joindre($tri, $i)
        joindre($trt, $nu)
      }
    }
    retour
  }""", 1)

# ─── P9 : unaire flottant — texte exact au lieu de la marque F ───
rep("""    si (vue_flag($va) == "1") {
      stocker($i, "F", "1", $foi, $fot, $fof)
      retour "1|F|1"
    }""",
    """    si (vue_flag($va) == "1") {
      $fv = val_d($va)
      $zn = faux
      si ($fv == 0.0 et non texte_neg(vue_texte($va))) { $zn = vrai }
      $rs = flot_txt(-$fv, $zn)
      stocker($i, $rs, "1", $foi, $fot, $fof)
      retour "1|" + $rs + "|1"
    }""", 1)

# ─── P10 : binaire flottant — calcul réel, nullptr miroirs fidèles ───
rep("""      si ($op == "+" ou $op == "-" ou $op == "*" ou $op == "/" ou $op == "^") {
        stocker($i, "F", "1", $foi, $fot, $fof)
        retour "1|F|1"
      }
      retour "0"
    }""",
    """      si ($op == "+" ou $op == "-" ou $op == "*" ou $op == "/" ou $op == "^") {
        $fa = val_d($va)
        $fbv = val_d($vb)
        $plie = 1
        $rf = 0.0
        $zn = faux
        si ($op == "+") { $rf = $fa + $fbv }
        sinon si ($op == "-") { $rf = $fa - $fbv }
        sinon si ($op == "*") {
          $rf = $fa * $fbv
          si ($rf == 0.0 et texte_neg(vue_texte($va)) != texte_neg(vue_texte($vb))) { $zn = vrai }
        }
        sinon si ($op == "/") {
          si ($fbv == 0.0) { $plie = 0 }
          sinon { $rf = $fa / $fbv }
        }
        sinon { $rf = $fa ^ $fbv }
        si ($plie == 1) {
          $rs = flot_txt($rf, $zn)
          si (contient($rs, "inf") ou contient($rs, "nan")) { $plie = 0 }
          si ($plie == 1) {
            stocker($i, $rs, "1", $foi, $fot, $fof)
            retour "1|" + $rs + "|1"
          }
        }
        retour "0"
      }
      retour "0"
    }""", 1)

# ─── P14 : garde d'exposant exacte (miroir mpz_fits_slong_p, non 200000) ───
rep("      si ($p2 > 200000 ou 0 - $p2 > 200000) { retour \"0\" }",
    "      si ($p2 > 9223372036854775807 ou 0 - $p2 > 9223372036854775807) { retour \"0\" }", 1)

# ─── P15 : rpui en exponentiation binaire (2^30000000 est pliable, la boucle
# naïve ne s'achèverait jamais) — valeur identique, verdict inchangé ───
rep("""fn rpui($b: entier, $e: entier) -> entier {
  $res = 1
  $k = 0
  tant_que ($k < $e) {
    $res = $res * $b
    $k = $k + 1
  }
  retour $res
}""", """fn rpui($b: entier, $e: entier) -> entier {
  $res = 1
  $f = $b
  $k = $e
  tant_que ($k > 0) {
    si ($k % 2 == 1) { $res = $res * $f }
    $f = $f * $f
    $k = $k div 2
  }
  retour $res
}""", 1)

# ─── P16 : vue_texte / vue_flag positionnels — la vue a une forme fixée
# « 1|texte|drapeau » (drapeau vide, « 1 » ou « q ») : plus de parcours
# caractère par caractère, qui couterait quadratic sur les plis geants. ───
rep("""fn vue_texte($v: chaine) -> chaine {
  $n = longueur($v)
  $d = 0
  $ok = 1
  tant_que ($ok == 1) {
    si ($d >= $n) { $ok = 0 }
    sinon si (ordre($v[$d]) == 124) { $ok = 0 }
    sinon { $d = $d + 1 }
  }
  $f = $n - 1
  $ok = 1
  tant_que ($ok == 1) {
    si ($f <= $d) { $ok = 0 }
    sinon si (ordre($v[$f]) == 124) { $ok = 0 }
    sinon { $f = $f - 1 }
  }
  retour sous_chaine($v, $d + 1, $f)
}""", """fn vue_texte($v: chaine) -> chaine {
  $n = longueur($v)
  si ($n < 3) { retour "" }
  si (ordre($v[$n - 1]) == 124) { retour sous_chaine($v, 2, $n - 1) }
  retour sous_chaine($v, 2, $n - 2)
}""", 1)

rep("""fn vue_flag($v: chaine) -> chaine {
  $n = longueur($v)
  $f = $n - 1
  $ok = 1
  tant_que ($ok == 1) {
    si ($f <= 0) { $ok = 0 }
    sinon si (ordre($v[$f]) == 124) { $ok = 0 }
    sinon { $f = $f - 1 }
  }
  retour sous_chaine($v, $f + 1, $n)
}""", """fn vue_flag($v: chaine) -> chaine {
  $n = longueur($v)
  si ($n < 3) { retour "" }
  si (ordre($v[$n - 1]) == 124) { retour "" }
  retour sous_chaine($v, $n - 1, $n)
}""", 1)

# ─── P17 : le pli rationnel porte son drapeau « q » (sinon = entier), le
# drapeau evite au verificateur de ratisser le texte pour demeler
# entier de rationnel — foi du drapeau, comme le type du noeud C++. ───
rep("""    $r = ""
    si ($op == "+") { $r = rnorm($p1 * $q2 + $p2 * $q1, $q1 * $q2) }
    sinon si ($op == "-") { $r = rnorm($p1 * $q2 - $p2 * $q1, $q1 * $q2) }
    sinon si ($op == "*") { $r = rnorm($p1 * $p2, $q1 * $q2) }
    sinon si ($op == "/") {
      si (rnul($tb)) { lever($l, "Division par zéro dans une constante", $err); retour "0" }
      $r = rnorm($p1 * $q2, $q1 * $p2)
    }""", """    $r = ""
    $np = 0
    $nq = 1
    si ($op == "+") { $np = $p1 * $q2 + $p2 * $q1; $nq = $q1 * $q2 }
    sinon si ($op == "-") { $np = $p1 * $q2 - $p2 * $q1; $nq = $q1 * $q2 }
    sinon si ($op == "*") { $np = $p1 * $p2; $nq = $q1 * $q2 }
    sinon si ($op == "/") {
      si (rnul($tb)) { lever($l, "Division par zéro dans une constante", $err); retour "0" }
      $np = $p1 * $q2; $nq = $q1 * $p2
    }""", 1)

rep("""      $r = rnorm($num, $den)
    }
    sinon si ($op == "div") {
      si (rnul($tb)) { lever($l, "division par zéro dans une constante", $err); retour "0" }
      $r = chaine($p1 * $q2 div ($q1 * $p2))
    }
    sinon { retour "0" }
    stocker($i, $r, "", $foi, $fot, $fof)
    retour "1|" + $r + "|"
  }""", """      $np = $num; $nq = $den
    }
    sinon si ($op == "div") {
      si (rnul($tb)) { lever($l, "division par zéro dans une constante", $err); retour "0" }
      $r = chaine($p1 * $q2 div ($q1 * $p2))
      stocker($i, $r, "", $foi, $fot, $fof)
      retour "1|" + $r + "|"
    }
    sinon { retour "0" }
    $r = rnorm($np, $nq)
    $fl = ""
    si (non rden1($np, $nq)) { $fl = "q" }
    stocker($i, $r, $fl, $foi, $fot, $fof)
    retour "1|" + $r + "|" + $fl
  }""", 1)

# ─── P18 : le verificateur lit le drapeau, pas le texte ───
rep("  si ($vue != \"0\") { retour t_from_texte(vue_texte($vue), vue_flag($vue)) }",
    "  si ($vue != \"0\") { retour t_entree(vue_flag($vue)) }", 1)

# ─── P19 : repli de factorielle sur litteral entier — miroir de la branche
# Factorielle du replier C++ (mpz_set_str, 0 <= n <= 20000, n<2 rend 1).
# Hors plage ou non entier : appel d'execution, comme le moteur. ───
rep("    si ($kd == \"fac\") { retour \"0\" }", """    si ($kd == "fac") {
      si ($va != "0" et vue_flag($va) == "") {
        $t = vue_texte($va)
        si (non contient_barre($t)) {
          $n = entier($t)
          si ($n >= 0 et $n <= 20000) {
            $acc = 1
            $j = 2
            tant_que ($j <= $n) { $acc = $acc * $j; $j = $j + 1 }
            $r = chaine($acc)
            stocker($i, $r, "", $foi, $fot, $fof)
            retour "1|" + $r + "|"
          }
        }
      }
      retour "0"
    }""", 1)

MODULE = open("/tmp/opencode/em_module.txt").read()
MAIN3 = open("/tmp/opencode/em_main.txt").read()

# ─── P11 : module d'émission avant le programme principal ───
marker = "# ══════════════════ programme principal ══════════════════"
assert src.count(marker) == 1
src = src.replace(marker, MODULE + "\n" + marker, 1)

# ─── P12 : passe 3 dans le main ───
rep('si (longueur($err) > 1) { >> $err[1] } sinon { >> "vérifié" }', MAIN3, 1)

open(DST, "w").write(src)
print(f"émis {DST} : {len(src.splitlines())} lignes")
