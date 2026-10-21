#!/usr/bin/env bash
# Sert le site ROSE + les artefacts Windows sur le tailnet (port 18080).
# Racine servie : <dépôt>/site (index.html, Rose-Setup-*.exe, rose-windows.zip).
set -u
IP=100.67.23.83
PORT=18080
DIR="$(cd "$(dirname "$0")/../site" && pwd)"

# motif entre crochets : pkill ne se tue pas lui-même
pkill -f "[h]ttp.server $PORT" 2>/dev/null || true
sleep 0.4

cd "$DIR" && setsid nohup python3 -m http.server "$PORT" --bind "$IP" \
  >/tmp/opencode/site-http.log 2>&1 &
sleep 0.6
curl -s -o /dev/null -w "site      : HTTP %{http_code}\n" "http://$IP:$PORT/"
curl -s -o /dev/null -w "setup     : HTTP %{http_code}\n" "http://$IP:$PORT/Rose-Setup-3.14.0.exe"
curl -s -o /dev/null -w "zip       : HTTP %{http_code}\n" "http://$IP:$PORT/rose-windows.zip"
