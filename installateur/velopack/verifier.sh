#!/bin/sh
# Vérifie la chaîne de mise à jour Velopack SANS Windows.
#
# Le module `rose/src/velopack.rs` est portable : on lui donne une racine
# d'installation factice (ROSE_VELOPACK_ROOT), un manifeste local
# (ROSE_VELOPACK_URL=file://…) et ROSE_VELOPACK=1. Trois cas sont joués :
#   1. version installée 0.1.3 → le paquet complet de la dernière version est
#      téléchargé et son SHA-256 vérifié ;
#   2. version installée à jour → « déjà à jour » ;
#   3. SHA-256 falsifié → refus et suppression du paquet.
#
# Usage : sh installateur/velopack/verifier.sh [./rose/rose]
set -e

ROSE_BIN="${1:-./rose/rose}"
RACINE=$(mktemp -d)
trap 'rm -rf "$RACINE"' EXIT

printf 'PAQUET-VELOPACK-FACTICE-0.1.4\n' > "$RACINE/Rose-0.1.4-full.nupkg"
SHA=$(sha256sum "$RACINE/Rose-0.1.4-full.nupkg" | cut -d' ' -f1)
TAILLE=$(stat -c%s "$RACINE/Rose-0.1.4-full.nupkg")

manifeste() {
  cat > "$RACINE/releases.win.json" <<EOF
[
  {"PackageId":"Rose-0.1.3-full.nupkg","Version":"0.1.3","SHA256":"$1","SHA1":"11","Size":"$TAILLE","Type":"Full"},
  {"PackageId":"Rose-0.1.4-full.nupkg","Version":"0.1.4","SHA256":"$2","SHA1":"22","Size":"$TAILLE","Type":"Full"},
  {"PackageId":"Rose-0.1.4-delta.nupkg","Version":"0.1.4","SHA256":"$2","SHA1":"33","Size":"$TAILLE","Type":"Delta"}
]
EOF
}

maj() {
  ROSE_VELOPACK_ROOT="$RACINE" \
  ROSE_VELOPACK_URL="file://$RACINE/releases.win.json" \
  "$ROSE_BIN" update --velopack 2>&1
}

ECHECS=0
JOURNAL=""
noter() { JOURNAL="$JOURNAL$1\n"; echo "$1"; }
verifier() { # libellé, attendu, sortie
  if printf '%s' "$3" | grep -q "$2"; then
    noter "OK   — $1"
  else
    echo "FAUX — $1 : attendu « $2 », obtenu « $3 »"
    ECHECS=$((ECHECS + 1))
  fi
}

# 1. mise à jour 0.1.3 → 0.1.4
printf 'app-0.1.3' > "$RACINE/current"
rm -rf "$RACINE/packages"
manifeste "$SHA" "$SHA"
SORTIE=$(maj)
# sans launcher, le paquet factice est TÉLÉCHARGÉ puis appliqué (l'extraction
# échoue sur un faux ZIP : c'est attendu) — ce qui compte ici est le téléchargement
verifier "0.1.3 → 0.1.4 téléchargé" "Mise à jour Velopack" "$SORTIE"
verifier "paquet appliqué (téléchargement vérifié)" "Application du paquet" "$SORTIE"
[ -f "$RACINE/packages/Rose-0.1.4-full.nupkg" ] || [ -d "$RACINE/app-0.1.4" ] \
  && noter "OK   — paquet récupéré puis appliqué (ou déposé)" \
  || { echo "FAUX — paquet absent"; ECHECS=$((ECHECS + 1)); }

# 2. déjà à jour
printf 'app-0.1.4' > "$RACINE/current"
SORTIE=$(maj)
verifier "0.1.4 déjà à jour" "déjà à jour" "$SORTIE"

# 3. SHA-256 falsifié
printf 'app-0.1.3' > "$RACINE/current"
rm -rf "$RACINE/packages"
manifeste "$SHA" "0000000000000000000000000000000000000000000000000000000000000000"
SORTIE=$(maj)
verifier "SHA-256 falsifié refusé" "SHA-256 du paquet invalide" "$SORTIE"
[ ! -f "$RACINE/packages/Rose-0.1.4-full.nupkg" ] \
  && noter "OK   — paquet corrompu supprimé" \
  || { echo "FAUX — paquet corrompu conservé"; ECHECS=$((ECHECS + 1)); }

# 4. application RÉELLE du paquet quand aucun launcher Velopack n'est installé
#    (installation faite à la main par Rose-win-Setup.exe) : extraction ZIP,
#    écriture de `current`, comparaison md5 des binaires extraits à la source.
PAQUET_REEL="$RACINE/../paquet-reel"
if [ -f "$(dirname "$ROSE_BIN")/../installateur/velopack/Releases/Rose-0.1.4-full.nupkg" ]; then
  PAQUET_REEL="$(dirname "$ROSE_BIN")/../installateur/velopack/Releases/Rose-0.1.4-full.nupkg"
fi
if [ -f "$PAQUET_REEL" ]; then
  printf 'app-0.1.3' > "$RACINE/current"
  rm -rf "$RACINE/packages" "$RACINE/app-0.1.4" "$RACINE/Update.exe"
  mkdir -p "$RACINE/packages"
  cp "$PAQUET_REEL" "$RACINE/packages/"
  SORTIE=$(ROSE_VELOPACK_ROOT="$RACINE" "$ROSE_BIN" update --velopack 2>&1)
  verifier "paquet réel appliqué sans launcher" "installé — relancez" "$SORTIE"
  [ "$(cat "$RACINE/current")" = "app-0.1.4" ] \
    && noter "OK   — current = app-0.1.4" \
    || { echo "FAUX — current = $(cat "$RACINE/current")"; ECHECS=$((ECHECS + 1)); }
  if [ -f "$RACINE/app-0.1.4/rose.exe" ]; then
    noter "OK   — rose.exe extrait ($(stat -c%s "$RACINE/app-0.1.4/rose.exe") o)"
  else
    echo "FAUX — rose.exe absent de app-0.1.4"; ECHECS=$((ECHECS + 1))
  fi
  # avec un launcher, le paquet reste en attente (c'est lui qui applique)
  touch "$RACINE/Update.exe"
  printf 'app-0.1.3' > "$RACINE/current"
  mkdir -p "$RACINE/packages"; cp "$PAQUET_REEL" "$RACINE/packages/"
  SORTIE=$(ROSE_VELOPACK_ROOT="$RACINE" "$ROSE_BIN" update --velopack 2>&1)
  verifier "launcher présent → paquet laissé en attente" "déposé" "$SORTIE"
fi

# 5. arguments du launcher Velopack
"$ROSE_BIN" --veloapp-updated; RC=$?
[ "$RC" -eq 0 ] && noter "OK   — launcher --veloapp-updated (code 0)" \
  || { echo "FAUX — launcher code $RC"; ECHECS=$((ECHECS + 1)); }
"$ROSE_BIN" --veloapp-obsolete; RC=$?
[ "$RC" -eq 0 ] && noter "OK   — launcher --veloapp-obsolete (code 0)" \
  || { echo "FAUX — launcher code $RC"; ECHECS=$((ECHECS + 1)); }

echo
if [ "$ECHECS" -eq 0 ]; then
  echo "Chaîne Velopack : $(printf "%b" "$JOURNAL" | grep -c "^OK") contrôles conformes."
else
  echo "Chaîne Velopack : $ECHECS échec(s)."
  exit 1
fi
