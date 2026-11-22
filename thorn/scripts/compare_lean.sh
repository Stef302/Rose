#!/usr/bin/env bash
# thorn/scripts/compare_lean.sh — MESURES NOUVELLES Thorn 0.1.73 vs Lean 4.36 core.
#
# Usage : bash thorn/scripts/compare_lean.sh [REPETITIONS] [WORKDIR]
#   défaut : 3 répétitions, WORKDIR=/tmp/opencode/thorn_lean_compare
#
# Pour chaque énoncé le script ÉCRIT les deux fichiers (Thorn et Lean) dans WORKDIR,
# puis mesure chaque commande avec /usr/bin/time -v (temps mural + pic RSS), REP fois,
# en séquentiel (un coeur est déjà pris par C2 : aucun parallélisme).
# Médianes calculées sur les REP valeurs. Toutes les valeurs affichées sont MESURÉES
# ICI le jour de l'exécution : aucune valeur citée n'est mélangée au tableau.
# Lean est utilisé en MODE CORE, sans mathlib (non installée).
# Aucun fichier du dépôt n'est écrit, aucun commit, aucune publication.
set -u

RACINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$RACINE" || exit 1
ROSE="./rose/rose"
REPS="${1:-3}"
WORK="${2:-/tmp/opencode/thorn_lean_compare}"
LEAN="${LEAN:-/dev/shm/leanhome/bin/lean}"
if [ ! -x "$LEAN" ]; then
  LEAN="$(command -v lean 2>/dev/null || true)"
fi

mkdir -p "$WORK"
echo "date UTC       : $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
echo "rose           : $($ROSE version 2>&1)"
echo "lean           : $($LEAN --version 2>&1 || echo 'ABSENT')"
echo "répétitions    : $REPS (séquentielles, un seul process à la fois)"
echo "messages       : /usr/bin/time -v —Elapsed + Maximum resident set size (kbytes)"
echo "périmètre      : fichiers générés dans $WORK (hors dépôt)"
echo "nature         : MESURÉ ICI. Aucune valeur CITÉE dans ce tableau."
echo "réserve        : même énoncé ≠ même travail (voir COMPARISON_LEAN.md §limites)."
echo

# ── 1. Énoncés → fichiers Thorn ──────────────────────────────────────────────────

cat > "$WORK/thorn_identite.thn" <<'EOF'
théorème identite : pour_tout A : type, pour_tout x : A, A
preuve
  introduction A
  introduction x
  application x
fin
EOF

cat > "$WORK/thorn_reflexivite.thn" <<'EOF'
théorème reflexivite : pour_tout A : type, pour_tout x : A, x = x
preuve reflexivite
fin
EOF

cat > "$WORK/thorn_symetrie.thn" <<'EOF'
théorème symetrie : pour_tout A : type, pour_tout a : A, pour_tout b : A, a = b -> b = a
preuve symetrie
fin
EOF

cp thorn/preuves/associativite.thn "$WORK/thorn_associativite.thn"
cp thorn/preuves/commutativite.thn "$WORK/thorn_commutativite.thn"

for n in refl trans; do
  if [ "$n" = refl ]; then LEM=ord_lem_refl; else LEM=ord_lem_trans; fi
  cat > "$WORK/thorn_ordre_${n}.rose" <<EOF
# Généré par compare_lean.sh — lemme d'ordre sur Nat/≤/PartialOrder.
importer "thorn/src/ordres.rose"
\$cpt = table()
inserer(\$cpt, "r", 0)
inserer(\$cpt, "e", 0)
\$reg = ord_registre()
\$par = ord_trouver(\$reg, "Nat/≤/PartialOrder")
ord_lance("ordre/${n}[Nat]", ${LEM}(\$par), ta_ctx(), \$cpt)
>> "OK=" + chaine(\$cpt["r"]) + " ECHEC=" + chaine(\$cpt["e"])
EOF
done

# ── 2. Mêmes énoncés → fichiers Lean core ───────────────────────────────────────

cat > "$WORK/lean_identite.lean" <<'EOF'
def identite : ∀ (A : Type) (x : A), A := fun A x => x
#check identite
EOF

cat > "$WORK/lean_reflexivite.lean" <<'EOF'
theorem reflexivite : ∀ (A : Type) (x : A), x = x := by
  intro A x
  rfl
#check reflexivite
EOF

cat > "$WORK/lean_symetrie.lean" <<'EOF'
theorem symetrie : ∀ (A : Type) (a b : A), a = b → b = a := by
  intro A a b h
  exact Eq.symm h
#check symetrie
EOF

cat > "$WORK/lean_associativite.lean" <<'EOF'
theorem associativite_plus : ∀ m n p : Nat, (m + n) + p = m + (n + p) := by
  exact Nat.add_assoc
#check associativite_plus
EOF

cat > "$WORK/lean_commutativite.lean" <<'EOF'
theorem commutativite_plus : ∀ m n : Nat, m + n = n + m := by
  exact Nat.add_comm
#check commutativite_plus
EOF

cat > "$WORK/lean_ordre_refl.lean" <<'EOF'
theorem ordre_refl : ∀ n : Nat, n ≤ n := by
  intro n
  exact Nat.le_refl n
#check ordre_refl
EOF

cat > "$WORK/lean_ordre_trans.lean" <<'EOF'
theorem ordre_trans : ∀ a b c : Nat, a ≤ b → b ≤ c → a ≤ c := by
  intro a b c h1 h2
  exact Nat.le_trans h1 h2
#check ordre_trans
EOF

# ── 3. Mesure ────────────────────────────────────────────────────────────────────

: > "$WORK/brut.txt"

secs_de() {
  awk -F': ' '/Elapsed \(wall clock\)/ {
    t=$NF; n=split(t,a,":");
    if (n==3) printf "%.2f\n", a[1]*3600+a[2]*60+a[3];
    else if (n==2) printf "%.2f\n", a[1]*60+a[2];
    else printf "%.2f\n", t+0
  }' "$1"
}

rss_de() {
  awk '/Maximum resident set size/ {print $NF}' "$1"
}

medianne() {  # $1 = fichier, $2 = colonne
  awk -v col="$2" '{print $col}' "$1" | sort -n | \
    awk '{a[NR]=$1} END { if(!NR){print "NA"; exit} if(NR%2) printf "%.2f\n",a[(NR+1)/2]; else printf "%.2f\n",(a[NR/2]+a[NR/2+1])/2 }'
}

# mesurer <label> <commande> ; écrit REP lignes "i secs kB" et retourne "secs kB" (médianes)
mesurer() {
  local label="$1"
  local cmd="$2"
  local log="$WORK/time_${label}.txt"
  local rep="$WORK/reps_${label}.txt"
  local i rc s k
  : > "$rep"
  for i in $(seq 1 "$REPS"); do
    /usr/bin/time -v -o "$log" bash -c "$cmd" >/dev/null 2>/dev/null
    rc=$?
    s="$(secs_de "$log")"
    k="$(rss_de "$log")"
    printf '%s %s %s rc=%s\n' "$i" "${s:-NA}" "${k:-NA}" "$rc" >> "$rep"
    printf '%s REP %s : %s s / %s Ko (rc=%s)\n' "$label" "$i" "${s:-NA}" "${k:-NA}" "$rc" >> "$WORK/brut.txt"
    if [ "$rc" -ne 0 ] || [ -z "${s:-}" ] || [ -z "${k:-}" ]; then
      printf 'ATTENTION : %s rep %s a échoué (rc=%s) — voir %s\n' "$label" "$i" "$rc" "$rep" >&2
    fi
  done
  printf '%s %s\n' "$(medianne "$rep" 2)" "$(medianne "$rep" 3)"
}

NOMS=(identite reflexivite symetrie associativite commutativite ordre_refl ordre_trans)
printf '%-16s | %-22s | %-22s | %-9s | %-9s\n' \
  'énoncé' 'Thorn médiane (s / Ko)' 'Lean médiane (s / Ko)' 'ratio t' 'ratio RSS'
printf '%-16s-+-%-22s-+-%-22s-+-%-9s-+-%-9s\n' \
  '----------------' '----------------------' '----------------------' '---------' '---------'

{
  echo '| Énoncé | Thorn méd. s | Thorn méd. Ko | Lean méd. s | Lean méd. Ko | Ratio temps (Thorn/Lean) | Ratio RSS |'
  echo '|---|---:|---:|---:|---:|---:|---:|'
} > "$WORK/tableau.md"

for n in "${NOMS[@]}"; do
  if [ "$n" = ordre_refl ] || [ "$n" = ordre_trans ]; then
    cmd_t="$ROSE $WORK/thorn_${n}.rose"
  else
    cmd_t="$ROSE thorn/src/principal.rose $WORK/thorn_${n}.thn"
  fi
  cmd_l="$LEAN $WORK/lean_${n}.lean"

  read -r ts tr <<< "$(mesurer "thorn_${n}" "$cmd_t")"
  if [ -n "$LEAN" ]; then
    read -r ls lr <<< "$(mesurer "lean_${n}" "$cmd_l")"
  else
    ls="NA"; lr="NA"
  fi

  if [ "$ls" != "NA" ] && [ "$ls" != "0.00" ]; then
    rt=$(awk -v a="$ts" -v b="$ls" 'BEGIN{printf "%.2f", a/b}')
  else
    rt="NA"
  fi
  if [ "$lr" != "NA" ] && [ "$lr" != "0" ]; then
    rr=$(awk -v a="$tr" -v b="$lr" 'BEGIN{printf "%.2f", a/b}')
  else
    rr="NA"
  fi

  printf '%-16s | %8s / %-11s | %8s / %-11s | %9s | %9s\n' \
    "$n" "$ts" "$tr" "$ls" "$lr" "$rt" "$rr"
  echo "| $n | $ts | $tr | $ls | $lr | $rt | $rr |" >> "$WORK/tableau.md"
done

echo
echo "Brut      : $WORK/brut.txt"
echo "Tableau   : $WORK/tableau.md"
echo "Fichiers  : $WORK/thorn_* $WORK/lean_*"
echo "FIN $(date -u '+%Y-%m-%dT%H:%M:%SZ') — toutes les valeurs ci-dessus sont MESURÉES ICI."
