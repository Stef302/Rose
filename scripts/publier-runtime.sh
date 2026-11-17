#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# M6.2 — PRÉPARATION de la publication de l'artefact runner wasmtime
#         release v0.1.5 — dépôt GitHub Stef302/Rose
#
# Usage :  bash scripts/publier-runtime.sh [artefact] [--exec]
#
# PAR DÉFAUT (dry-run) : vérifie le compte, vérifie l'artefact, calcule son
# SHA-256 et AFFICHE la commande `gh release upload` SANS L'EXÉCUTER.
# `--exec` : demande confirmation explicite (taper OUI) puis exécute l'upload.
#
# Ce script NE modifie aucun fichier du dépôt, NE touche à aucun .rs,
# NE télécharge RIEN et NE publie RIEN en mode dry-run. Le token GitHub est
# lu dans ~/.config/opencode/.gh_token (jamais écrit dans le dépôt).
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

VERSION="v0.1.5"                 # = VERSION_RUNTIME dans wasm_runtime.rs
REPO="Stef302/Rose"              # jamais clausake
COMPTE_ATTENDU="Stef302"
TOKEN_FILE="${GH_TOKEN_FILE:-$HOME/.config/opencode/.gh_token}"

usage() {
  echo "Usage: bash scripts/publier-runtime.sh [artefact] [--exec]"
  echo "  artefact  défaut: rose-wasmtime-runner-${VERSION}-<cible()>"
  echo "            (cible détectée comme dans rose/src/wasm_runtime.rs::cible())"
  echo "  --exec    exécute l'upload après confirmation 'OUI' (défaut: dry-run)"
}

# ── 0. Arguments ──────────────────────────────────────────────────────
EXEC=0
ARTIFACT=""
for arg in "$@"; do
  case "$arg" in
    --exec) EXEC=1 ;;
    -h|--help) usage; exit 0 ;;
    *) if [ -n "$ARTIFACT" ]; then
         echo "ERREUR: un seul artefact accepté (reçu: $ARTIFACT et $arg)" >&2
         exit 2
       fi
       ARTIFACT="$arg" ;;
  esac
done

detect_cible() {
  local arch os
  arch="$(uname -m)"; os="$(uname -s)"
  case "$arch:$os" in
    aarch64:Linux)                        echo "aarch64-linux" ;;
    x86_64:Linux)                         echo "x86_64-linux" ;;
    x86_64:MINGW*|x86_64:MSYS*|x86_64:CYGWIN*) echo "x86_64-windows.exe" ;;
    aarch64:Darwin)                       echo "aarch64-macos" ;;
    x86_64:Darwin)                        echo "x86_64-macos" ;;
    *) echo "${arch}-${os}" | tr '[:upper:]' '[:lower:]' ;;
  esac
}

if [ -z "$ARTIFACT" ]; then
  ARTIFACT="rose-wasmtime-runner-${VERSION}-$(detect_cible)"
fi

# ── 1. Token GitHub — lu dans le fichier, jamais affiché ─────────────
if [ ! -r "$TOKEN_FILE" ]; then
  echo "REFUS: token GitHub illisible : $TOKEN_FILE" >&2
  echo "  (le token Stef302 ne doit jamais être copié dans le dépôt)" >&2
  exit 1
fi
TOKEN="$(tr -d '\r\n' < "$TOKEN_FILE")"
if [ -z "$TOKEN" ]; then
  echo "REFUS: token GitHub vide : $TOKEN_FILE" >&2
  exit 1
fi

verifier_compte() {
  local login
  if ! login="$(GH_TOKEN="$TOKEN" gh api user -q .login 2>/dev/null)"; then
    echo "REFUS: impossible d'interroger 'gh api user' (gh absent, token invalide ou réseau indisponible)." >&2
    return 1
  fi
  if [ "$login" != "$COMPTE_ATTENDU" ]; then
    echo "REFUS: compte GitHub '$login' != '$COMPTE_ATTENDU'. AUCUNE publication." >&2
    echo "  Ce dépôt appartient à Stef302 (git@github.com:Stef302/Rose.git)." >&2
    return 1
  fi
  printf '%s' "$login"
}

LOGIN="$(verifier_compte)"
printf 'Compte GitHub : %s (source: %s) — OK\n' "$LOGIN" "$TOKEN_FILE"
printf 'Release       : %s — %s\n' "$VERSION" "$REPO"

# ── 2. Artefact local : existence, exécutabilité, taille, SHA-256 ─────
if [ ! -f "$ARTIFACT" ]; then
  echo "REFUS: artefact introuvable : $ARTIFACT" >&2
  echo "  Produire le runner (voir rose/docs/RUNTIME_RUNNER.md) puis relancer." >&2
  exit 1
fi
if [ ! -x "$ARTIFACT" ]; then
  echo "REFUS: artefact non exécutable : $ARTIFACT" >&2
  echo "  Corriger avec : chmod +x '$ARTIFACT'" >&2
  exit 1
fi

TAILLE="$(stat -c %s "$ARTIFACT" 2>/dev/null || wc -c < "$ARTIFACT")"
SHA="$(sha256sum "$ARTIFACT" | awk '{print $1}')"
if ! printf '%s' "$SHA" | grep -Eq '^[0-9a-f]{64}$'; then
  echo "REFUS: SHA-256 inattendu pour '$ARTIFACT' : '$SHA'" >&2
  exit 1
fi

# Contrôle de cohérence avec une empreinte déjà renseignée (si fournie).
if [ -n "${ROSE_WASM_RUNTIME_SHA256:-}" ]; then
  ATTENDU="$(printf '%s' "$ROSE_WASM_RUNTIME_SHA256" | tr '[:upper:]' '[:lower:]')"
  if [ "$ATTENDU" != "$SHA" ]; then
    echo "REFUS: ROSE_WASM_RUNTIME_SHA256 ($ATTENDU) != SHA-256 de l'artefact ($SHA)" >&2
    exit 1
  fi
  echo "Cohérence SHA-256 avec ROSE_WASM_RUNTIME_SHA256 : OK"
fi

printf 'Artefact      : %s\n' "$ARTIFACT"
printf 'Taille        : %s octets\n' "$TAILLE"
printf 'SHA-256       : %s\n' "$SHA"

printf -v ART_Q '%q' "$ARTIFACT"
CMD="gh release upload $VERSION $ART_Q --repo $REPO"

# ── 3. Mode dry-run (par défaut) : afficher, ne RIEN publier ──────────
if [ "$EXEC" -eq 0 ]; then
  echo
  echo "MODE DRY-RUN — AUCUNE publication effectuée."
  echo "Commande à exécuter APRÈS accord explicite :"
  printf '  GH_TOKEN="$(cat %s)" %s\n' "$TOKEN_FILE" "$CMD"
  echo
  echo "Après publication, renseigner l'empreinte officielle ($SHA) :"
  echo "  - test : ROSE_WASM_RUNTIME_SHA256=$SHA rose wasm --install-runtime wasmtime"
  echo "  - définitif : table intégrée de sha256_attendu() (à faire par l'orchestrateur)"
  echo "Puis : bash scripts/mesurer-wasm.sh  (contrat : fib 27 -> 196418, seuil <= 20 ms)"
  exit 0
fi

# ── 4. Mode --exec : re-vérification du compte + accord + upload ─────
LOGIN2="$(verifier_compte)"
if [ "$LOGIN2" != "$COMPTE_ATTENDU" ]; then
  echo "REFUS final: compte inattendu. AUCUNE publication." >&2
  exit 1
fi
echo
echo "ATTENTION: publication RÉELLE sur $REPO (tag $VERSION)."
echo "  $CMD"
printf 'Tapez OUI pour confirmer : '
read -r CONFIRM || CONFIRM=""
if [ "$CONFIRM" != "OUI" ]; then
  echo "Annulé — aucune publication effectuée."
  exit 1
fi

if GH_TOKEN="$TOKEN" gh release upload "$VERSION" "$ARTIFACT" --repo "$REPO"; then
  echo "Publication terminée. Vérification à faire :"
  printf '  GH_TOKEN="$(cat %s)" gh release view %s --repo %s\n' "$TOKEN_FILE" "$VERSION" "$REPO"
  echo "  Puis re-télécharger l'asset et revérifier : sha256sum -> $SHA"
else
  echo "ERREUR: 'gh release upload' a échoué." >&2
  exit 1
fi
