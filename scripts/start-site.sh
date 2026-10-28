#!/usr/bin/env bash
# Sert le site ROSE en local pour la relecture (port 18080).
# Racine servie : <dépôt>/site (index.html, Rose-Setup-*.exe, rose-windows.zip).
# Le canal PUBLIC est GitHub Pages (https://stef302.github.io/Rose/) ; ce
# script ne sert que la prévisualisation sur la machine, jamais une IP privée.
set -u
BIND="${BIND:-127.0.0.1}"
PORT=18080
DIR="$(cd "$(dirname "$0")/../site" && pwd)"

# motif entre crochets : pkill ne se tue pas lui-même
pkill -f "[h]ttp.server $PORT" 2>/dev/null || true
sleep 0.4

cd "$DIR" && setsid nohup python3 -m http.server "$PORT" --bind "$BIND" \
  >/tmp/opencode/site-http.log 2>&1 &
sleep 0.6
curl -s -o /dev/null -w "site      : HTTP %{http_code}\n" "http://$BIND:$PORT/"
curl -s -o /dev/null -w "setup     : HTTP %{http_code}\n" "http://$BIND:$PORT/Rose-Setup-3.14.0.exe"
curl -s -o /dev/null -w "zip       : HTTP %{http_code}\n" "http://$BIND:$PORT/rose-windows.zip"
