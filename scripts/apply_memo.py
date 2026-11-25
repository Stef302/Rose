#!/usr/bin/env python3
"""Applique l'optimisation de mémoïsation des fonctions pures au moteur C0.

Patch déterministe sur compilateur/moteur_complet.rose (un seul fichier).
"""
import sys
from pathlib import Path

p = Path(sys.argv[1] if len(sys.argv) > 1 else "compilateur/moteur_complet.rose")
s = p.read_text(encoding="utf-8")


def rep(old, new, count=1):
    global s
    n = s.count(old)
    if n != count:
        raise SystemExit(f"REMPLACEMENT introuvable/ambigu ({n} au lieu de {count}) :\n{old[:200]}")
    s = s.replace(old, new, count)


# 1. Nouvelles globales (le cache est rempli par joindre, mutation en place).
rep('''$glob_nom = [""]
''', '''$glob_nom = [""]
$fn_pure = [0]          # 1 = fonction pure, mémoïsable (calculé après parse_rir)
$cache_key = [""]       # clés d'appels 1-based
$cache_val = [""]       # valeurs correspondantes
''')

# 2. Analyse de pureté + cache, insérés juste avant call_fn.
rep('''fn call_fn($m: liste<chaine>, $name: chaine, $args: liste<chaine>) -> liste<chaine> {
  $fi = find_fn($name)
  si ($fi < 1) { retour pack_ret("v:", $m) }
  $base = longueur($m)
  $ns = $fn_nslots[$fi]
  $j = 0
  tant_que ($j < $ns) { joindre($m, "v:"); $j = $j + 1 }
  $pn = $fn_pn[$fi]
  $j = 1
  tant_que ($j <= $pn et $j < longueur($args)) {
    $m = lset($m, $base + $j - 1, $args[$j])
    $j = $j + 1
  }
  retour exec_fn($m, $base, $fi)
}
''', '''# ── Mémoïsation des fonctions pures ────────────────────────────────────
# Une fonction est dite pure quand aucun de ses opcodes ne modifie la mémoire
# (`affect`, `expr`, `afficher`) et qu'elle n'appelle que des fonctions pures
# ou elle-même. Elle ne lit aucun global. Le résultat d'un appel ne dépend
# alors que de ses arguments : on peut le mémoriser. Cela transforme les
# récursions pures (fib, factorielle, …) de O(2^n) en O(n) appels réels.
fn calculer_purete() -> liste<entier> {
  $nf = longueur($fn_nom) - 1
  $pure = [0]
  $i = 1
  tant_que ($i <= $nf) { joindre($pure, 1); $i = $i + 1 }
  $tour = 0
  $change = vrai
  tant_que ($change et $tour < 16) {
    $change = faux
    $tour = $tour + 1
    $i = 1
    tant_que ($i <= $nf) {
      si ($pure[$i] == 1) {
        $imp = 0
        $gi = $fn_first[$i] + 1
        $fin = $fn_first[$i] + $fn_nops[$i]
        tant_que ($gi <= $fin et $imp == 0) {
          $st = $op_start[$gi]
          $en = $st + $op_len[$gi] - 1
          $j = $st
          tant_que ($j <= $en et $imp == 0) {
            $t = $tok[$j]
            si ($t == "affect" ou $t == "affect_ast" ou $t == "afficher"
                ou $t == "afficher_abstrait" ou $t == "expr"
                ou $t == "surcharge" ou $t == "appel_surcharge") { $imp = 1 }
            si (($t == "app" ou $t == "appel" ou $t == "appel_rir") et $j < $en) {
              $ff = find_fn($tok[$j + 1])
              si ($ff < 1) { $imp = 1 } sinon { si ($pure[$ff] == 0) { $imp = 1 } }
            }
            si ($t == "v" et $j < $en) {
              si (glob_idx($tok[$j + 1]) > 0) { $imp = 1 }
            }
            si ($t == "slot" et $j < $en) {
              $nm = $slot_nom[$fn_soff[$i] + entier($tok[$j + 1])]
              si (glob_idx($nm) > 0) { $imp = 1 }
            }
            $j = $j + 1
          }
          $gi = $gi + 1
        }
        si ($imp == 1) { $pure = iset($pure, $i, 0); $change = vrai }
      }
      $i = $i + 1
    }
  }
  retour $pure
}
fn cache_key_of($name: chaine, $args: liste<chaine>) -> chaine {
  $key = $name
  $j = 1
  tant_que ($j < longueur($args)) {
    $v = $args[$j]
    $key = $key + "|" + chaine(longueur($v)) + ":" + $v
    $j = $j + 1
  }
  retour $key
}
fn cache_get($key: chaine) -> chaine {
  $k = longueur($cache_key) - 1
  tant_que ($k >= 1) {
    si ($cache_key[$k] == $key) { retour $cache_val[$k] }
    $k = $k - 1
  }
  retour ""
}
fn cache_put($key: chaine, $v: chaine) -> entier {
  si (longueur($cache_key) < 200001) {
    joindre($cache_key, $key)
    joindre($cache_val, $v)
  }
  retour 0
}
fn call_fn_direct($m: liste<chaine>, $fi: entier, $args: liste<chaine>) -> liste<chaine> {
  $base = longueur($m)
  $ns = $fn_nslots[$fi]
  $j = 0
  tant_que ($j < $ns) { joindre($m, "v:"); $j = $j + 1 }
  $pn = $fn_pn[$fi]
  $j = 1
  tant_que ($j <= $pn et $j < longueur($args)) {
    $m = lset($m, $base + $j - 1, $args[$j])
    $j = $j + 1
  }
  retour exec_fn($m, $base, $fi)
}
fn call_fn($m: liste<chaine>, $name: chaine, $args: liste<chaine>) -> liste<chaine> {
  $fi = find_fn($name)
  si ($fi < 1) { retour pack_ret("v:", $m) }
  si ($fi < longueur($fn_pure) et $fn_pure[$fi] == 1) {
    $key = cache_key_of($name, $args)
    $hit = cache_get($key)
    si (longueur($hit) > 0) { retour pack_ret($hit, $m) }
    $pr = call_fn_direct($m, $fi, $args)
    cache_put($key, $pr[1])
    retour $pr
  }
  retour call_fn_direct($m, $fi, $args)
}
''')

# 3. Calcul de la pureté juste après le parse du RIR du programme.
rep('''$mc_rir = sous_chaine($mc_res, 3, longueur($mc_res))
parse_rir($mc_rir)
''', '''$mc_rir = sous_chaine($mc_res, 3, longueur($mc_res))
parse_rir($mc_rir)
# Analyse de pureté inutile si le programme n'a qu'une fonction ($principal).
si (longueur($fn_nom) > 2) { $fn_pure = calculer_purete() }
''')

p.write_text(s, encoding="utf-8")
print(f"PATCHÉ : {p} — {len(s.splitlines())} lignes")
