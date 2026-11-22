#!/usr/bin/env bash
# thorn/scripts/capacites.sh — catalogue commenté et reproductible des capacités Thorn.
#
# Usage :
#   bash thorn/scripts/capacites.sh            # catalogue lisible + sorties réelles + statut
#   bash thorn/scripts/capacites.sh --check    # statut seul, code 0 si tout passe
#   bash thorn/scripts/capacites.sh --dump     # sorties brutes concaténées (pour verifier_proprete.sh)
#
# Autonome : aucun fichier du dépôt n'est modifié. Les deux fichiers de démonstration
# (ordres + décidabilité) sont générés dans /tmp/opencode/thorn_capacites/.
# Aucun commit, aucune publication. Ne dépend que de ./rose/rose (binaire du dépôt).
set -u

RACINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$RACINE" || exit 1
ROSE="./rose/rose"
WORK="/tmp/opencode/thorn_capacites"
MODE="${1:---catalogue}"

mkdir -p "$WORK"

# ── Fichier généré : ordres M1 + décidabilité calculée + refus d'une preuve fausse ──
cat > "$WORK/demo_ordres.rose" <<'EOF'
# Généré par thorn/scripts/capacites.sh — jalon M1 (ordres, décision, refus).
importer "thorn/src/ordres.rose"

$cpt = table()
inserer($cpt, "r", 0)
inserer($cpt, "e", 0)

fn dem_test_dec(nom: chaine, r: table, attendu: booleen, cpt: table) {
  $t = ord_tab(r["terme"])
  $b = ord_tab(r["but"])
  $v = verifier($t, ta_ctx())
  $ok = (r["ok"] == vrai) et noy_est_ok($v) et
        convertible(noy_type_res($v), $b) et
        ((r["vrai"] == vrai) == attendu)
  si ($ok) { >> "OK [décision] " + nom } sinon { >> "ECHEC [décision] " + nom }
  ord_compter(cpt, $ok)
}

$reg = ord_registre()
$par = ord_trouver($reg, "Nat/≤/PartialOrder")
ord_lance("ordre/refl[Nat]", ord_lem_refl($par), ta_ctx(), $cpt)
ord_lance("ordre/trans[Nat]", ord_lem_trans($par), ta_ctx(), $cpt)

$o0 = ta_zero()
$o1 = ta_succ($o0)
$o2 = ta_succ($o1)
dem_test_dec("decider/1 ≤ 2 (vrai)", ord_decide_nat($o1, $o2), vrai, $cpt)
dem_test_dec("decider/3 ≤ 1 (faux)", ord_decide_nat(ta_succ($o2), $o1), faux, $cpt)

$faux_but = al_le($o0, $o1)
$faux_pr  = al_le_refl($o0)
$rv = verifier($faux_pr, ta_ctx())
$refuse = (non noy_est_ok($rv)) ou (non convertible(noy_type_res($rv), $faux_but))
si ($refuse) { >> "OK [refus] 0≤0 n'est pas 0≤1" } sinon { >> "ECHEC [refus]" }
ord_compter($cpt, $refuse)

>> "OK=" + chaine($cpt["r"]) + " ECHEC=" + chaine($cpt["e"])
EOF

# ── Catalogue : nom | extrait attendu | commande (eval) | rc attendu ──────────────
# Le format « | » impose de ne pas mettre de pipe dans les commandes : c'est volontaire.
# « ANY » = un code non nul est attendu (cas du refus délibéré d'une preuve fausse).
CATALOGUE=$(cat <<'EOF'
termes_et_Pi|ok      = vrai|$ROSE thorn/exemples/ex01_identite.rose|0
beta_alpha|alpha(Lam(x:X,x), Lam(y:X,y)) = vrai|$ROSE thorn/exemples/ex02_beta_alpha.rose|0
iota_reecriture|convertible(0+n, n)       = vrai|$ROSE thorn/exemples/ex03_iota.rose|0
cumulativite|cumulatif(Type0, Type1)   = vrai|$ROSE thorn/exemples/ex05_cumul.rose|0
syntaxe_francaise|[genre => pi|$ROSE thorn/exemples/ex06_syntaxe.rose|0
reflexivite_symetrie|symetrie : ok          = vrai|$ROSE thorn/exemples/ex07_refl_sym.rose|0
introduction_application|introduction : ok        = vrai|$ROSE thorn/exemples/ex08_intro_app.rose|0
induction_nat|convertible(type, but)  = vrai|$ROSE thorn/exemples/ex09_induction.rose|0
noyau_refuse_faux|code(Ω avec limite 0)    = GARDE_PAS|$ROSE thorn/exemples/ex04_refus.rose|0
pilote_identite|OK identite|$ROSE thorn/src/principal.rose thorn/preuves/identite.thn|0
pilote_associativite|OK associativite_plus|$ROSE thorn/src/principal.rose thorn/preuves/associativite.thn|0
pilote_refus|1 théorème(s), 0 réussi(s)|$ROSE thorn/src/principal.rose thorn/exemples/ex10_echec.thn|ANY
instances_M0|6 contrôle(s) réussi(s), 0 échec(s)|$ROSE thorn/exemples/ex11_mathlib.rose|0
ordres_decision_refus|OK=5 ECHEC=0|$ROSE $WORK/demo_ordres.rose|0
suite_noyau|REUSSIS=165|$ROSE thorn/tests_noyau.rose|0
EOF
)

TOTAL=0
PASS=0
FAIL=0

# Affiche au plus 10 lignes de sortie réelle, avec un marqueur de troncature.
afficher_sortie() {
  local nb
  nb=$(wc -l < "$1")
  if [ "$nb" -le 10 ]; then
    sed 's/^/    | /' "$1"
  else
    sed -n '1,6p' "$1" | sed 's/^/    | /'
    printf '    | … (%s lignes au total, milieu omis) …\n' "$nb"
    tail -2 "$1" | sed 's/^/    | /'
  fi
}

while IFS='|' read -r nom attendu cmd rc_attendu; do
  [ -z "${nom:-}" ] && continue
  # Développer les raccourcis avant affichage (le catalogue reste lisible dans le fichier).
  cmd="${cmd//\$ROSE/$ROSE}"
  cmd="${cmd//\$WORK/$WORK}"
  TOTAL=$((TOTAL + 1))
  sortie="$WORK/sortie_${nom}.txt"
  eval "$cmd" > "$sortie" 2>&1
  rc=$?
  rc_ok=0
  case "${rc_attendu:-0}" in
    ANY) rc_ok=0 ;;
    *)   [ "$rc" -eq "${rc_attendu:-0}" ] && rc_ok=0 || rc_ok=1 ;;
  esac
  if [ "$rc_ok" -eq 0 ] && grep -Fq -- "$attendu" "$sortie"; then
    statut="OK"
    PASS=$((PASS + 1))
  else
    statut="ÉCHEC"
    FAIL=$((FAIL + 1))
  fi

  case "$MODE" in
    --dump)
      printf '### %s (rc=%s, %s)\n' "$nom" "$rc" "$statut"
      cat "$sortie"
      ;;
    --check)
      printf '[%s] %s\n' "$statut" "$nom"
      if [ "$statut" = "ÉCHEC" ]; then
        printf '    attendu: %s\n' "$attendu"
        afficher_sortie "$sortie"
      fi
      ;;
    *)
      printf '\n[%s] %s\n' "$statut" "$nom"
      printf '    commande : %s\n' "$cmd"
      printf '    attendu  : %s\n' "$attendu"
      afficher_sortie "$sortie"
      ;;
  esac
done <<< "$CATALOGUE"

if [ "$MODE" != "--dump" ]; then
  printf '\n=== BILAN : %s cas, %s OK, %s ÉCHEC ===\n' "$TOTAL" "$PASS" "$FAIL"
fi
[ "$FAIL" -eq 0 ]
