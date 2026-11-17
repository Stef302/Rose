#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# Validation centralisée des chantiers M4/M2/M3/M5/M6.
#
# Usage:  bash scripts/valider-chantiers.sh
#
# Ce script NE modifie aucun fichier. Il imprime des mesures reproductibles
# pour chaque jalon. Les seuils sont des CONSTATS, pas des consignes : toute
# mesure doit être re-lue après un changement de modèle, de quantification
# ou de topologie (règle AGENTS.md).
# ═══════════════════════════════════════════════════════════════════════
set -u
cd "$(dirname "$0")/.."
ROSE=./rose/rose

hdr() { echo; echo "══ $1 ══"; }

hdr "M4.1 — lexeur Rose vs rose tokens"
ok=0; tot=0
for f in $(find examples jardins -name '*.rose' 2>/dev/null | sort); do
  tot=$((tot+1))
  $ROSE compilateur/lexeur.rose "$f" > /tmp/vlex.out 2>&1
  $ROSE tokens "$f" > /tmp/vlex.ref 2>&1
  diff -q /tmp/vlex.ref /tmp/vlex.out >/dev/null 2>&1 && ok=$((ok+1))
done
echo "lexeur : $ok/$tot"

hdr "M4.2 — analyseur Rose vs rose ast"
ok=0; tot=0
for f in $(find examples jardins bench/dores rose-cpp/tests/roses -name '*.rose' 2>/dev/null | sort); do
  tot=$((tot+1))
  $ROSE compilateur/analyseur.rose "$f" > /tmp/vast.out 2>&1
  $ROSE ast "$f" > /tmp/vast.ref 2>&1
  diff -q /tmp/vast.ref /tmp/vast.out >/dev/null 2>&1 && ok=$((ok+1))
done
echo "analyseur : $ok/$tot"

hdr "M4.3 — vérificateur Rose vs rose verif"
ok=0; tot=0
for f in $(find examples jardins rose-cpp/tests/verifs -name '*.rose' 2>/dev/null | sort); do
  tot=$((tot+1))
  a=$($ROSE verif "$f" 2>&1)
  b=$($ROSE compilateur/verifieur.rose "$f" 2>&1)
  [ "$a" = "$b" ] && ok=$((ok+1))
done
echo "vérificateur : $ok/$tot"

hdr "M4.4 — émetteur Rose vs rose rir"
ok=0; tot=0; errs=0
for f in $(find examples jardins bench/dores rose-cpp/tests/roses -name '*.rose' 2>/dev/null | sort); do
  tot=$((tot+1))
  o=$($ROSE rir "$f" 2>&1 | head -1)
  r=$($ROSE compilateur/emetteur.rose "$f" 2>&1 | head -1)
  if [ "$o" != "rir" ]; then continue; fi
  if [ "$r" != "rir" ]; then errs=$((errs+1)); continue; fi
  $ROSE rir "$f" > /tmp/vrir.ref 2>&1
  $ROSE compilateur/emetteur.rose "$f" > /tmp/vrir.out 2>&1
  diff -q /tmp/vrir.ref /tmp/vrir.out >/dev/null 2>&1 && ok=$((ok+1))
done
echo "émetteur : $ok/$tot (erreurs émission: $errs)"

hdr "Auto-vérification des composants Rose"
for c in lexeur analyseur verifieur emetteur compilateur; do
  f=compilateur/$c.rose
  [ -f "$f" ] && echo "$c : $($ROSE verif "$f" 2>&1 | head -1)"
done

hdr "M2 — non-régression exécution"
$ROSE jardins/tests/premiers.rose 2>&1 | tail -1
printf 'fn fib(n: entier) -> entier { si (n < 2) { retour n }; retour fib(n-1) + fib(n-2) }\n>> fib(25)\n' > /tmp/vfib.rose
echo "fib(25) : $($ROSE /tmp/vfib.rose 2>&1)"

hdr "M2.1 — mesure jardin (3 échantillons)"
for i in 1 2 3; do
  /usr/bin/time -f "  sample$i: %e s" $ROSE jardins/tests/premiers.rose >/dev/null
done

hdr "État git"
git status --short
echo "compte GitHub: $(GH_TOKEN="$(cat ~/.config/opencode/.gh_token 2>/dev/null)" gh api user -q .login 2>/dev/null || echo 'token absent/invalide')"
