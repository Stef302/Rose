# Mémoire — protocole de mesure, chiffres réels et règles

Date des mesures : **2026-11-22** (session 14:48–14:55 UTC). Machine : 121 Gio RAM,
15 Gio swap. Toutes les mesures ci-dessous ont été faites avec
`scripts/prof_mem_rose.sh`, **toujours** via `scripts/garde_fou_mem.sh`.

## 1. Règle absolue

> **Jamais de compilation Rose longue sans plafond mémoire.**

`timeout N` ne protège que du temps. Il ne limite pas la RSS. Le 22/11/2026,
`C0_final2.elf compilateur/rose_natif.rose` lancé sans plafond a monté à
**84 Gio de RSS en 27 min**, épuisé le swap et dû être arrêté (rc=143, fichier
`/tmp/opencode/C1_final2.elf` resté à 0 octet — voir `final_chain.log`).

Forme obligatoire :

```bash
scripts/garde_fou_mem.sh 16777216 <commande Rose> [args...]
# ou, pour une mesure reproductible :
scripts/prof_mem_rose.sh -n 3 -- <commande Rose> [args...]
```

`16777216` Ko = 16 Gio. `rc=75` signifie « limite atteinte, arrêt propre par le
garde-fou » : c'est un **résultat attendu**, pas une panne à masquer.

## 2. Protocole de mesure

```bash
scripts/prof_mem_rose.sh [-m GIO] [-n N] [-o RAPPORT] -- COMMANDE [ARGS...]
```

- refuse de lancer si `MemAvailable < 20 Gio` (`/proc/meminfo`), code 74 ;
- passe **toujours** par `garde_fou_mem.sh <plafond>` (défaut 16 Gio) ;
- **≥ 3 exécutions**, médianes du temps réel et du pic RSS ;
- temps réel et pic RSS lus dans la sortie `/usr/bin/time -v` du garde-fou
  (le `wait4` du garde-fou englobe la RSS de l'enfant compilé — vérifié par une
  sonde `bash -> python3` allouant 200 Mio : `time` a bien rapporté 214 004 Ko) ;
- `SwapFree` échantillonné toutes les secondes pendant chaque exécution ;
- rapport sur stdout + fichier, et journal appendu à
  `/tmp/opencode/prof_mem_rose_journal.tsv`.

**Biais connu** : la boucle du garde-fou dort 5 s entre deux contrôles. Le temps
réel peut donc inclure jusqu'à 5 s de latence de détection de fin (visible sur
`fib28` : 5,01 s pour une compilation quasi instantanée). Le pic RSS peut, lui,
**dépasser le plafond** de plusieurs Gio : le garde-fou ne voit le dépassement
qu'au prochain réveil, puis tue. C'est pourquoi il faut garder une marge, et
surveiller `SwapFree` comme canary.

Commandes exactes exécutées :

```bash
PROF_REPORT=/tmp/opencode/prof_rapport_fib28.txt \
  scripts/prof_mem_rose.sh -n 3 -- /tmp/opencode/C0_final2.elf /tmp/opencode/fib28.rose
PROF_REPORT=/tmp/opencode/prof_rapport_noyau.txt \
  scripts/prof_mem_rose.sh -n 3 -- /tmp/opencode/C0_final2.elf thorn/src/noyau.rose
PROF_REPORT=/tmp/opencode/prof_rapport_emetteur.txt \
  scripts/prof_mem_rose.sh -n 3 -- /tmp/opencode/C0_final2.elf compilateur/emetteur.rose
PROF_REPORT=/tmp/opencode/prof_rapport_rose_natif.txt \
  scripts/prof_mem_rose.sh -n 3 -- /tmp/opencode/C0_final2.elf compilateur/rose_natif.rose
```

## 3. Montée en charge mesurée (C0_final2.elf, plafond 16 Gio)

| Source | Taille | rc | Temps médian | Pic RSS médian | Swap libre | Observations |
|---|---:|---:|---:|---:|---:|---|
| `/tmp/opencode/fib28.rose` | 100 o | 0 | 5,01 s | 14 172 Ko (13,8 Mio) | 13,49 Gio, constant | 3/3 rc=0 ; temps = latence de sonde du garde-fou |
| `thorn/src/noyau.rose` | 54 075 o | 1 | 15,04 s | 49 600 Ko (48,4 Mio) | 13,49 Gio, constant | 3/3 rc=1 : échec de compilation, RSS d'un échec précoce (pic 49 980 Ko au 3e run) |
| `compilateur/emetteur.rose` | 155 504 o | **75** | 72,24 s | **19 928 032 Ko (19,0 Gio)** | 13,49 Gio, constant | 3/3 rc=75 ; pics par run : 19,0 / 17,0 / 23,3 Gio ; arrêt propre |
| `compilateur/rose_natif.rose` | 440 046 o | **75** | 317,95 s (1 run) | **18 416 348 Ko (17,6 Gio)** | 13,49 Gio, constant | run 1 rc=75 en 5 min 18 ; runs 2–3 interrompus à la clôture de la session (timeout 900 s) — **non médian** |

Le swap libre est resté à ~13,49 Gio partout (variation < 500 Ko, bruit des
processus concurrents) : **le garde-fou a rempli son office, aucun swap n'a été
consommé**.

### Seuil identifié

La dérive apparaît **entre ~54 Ko et ~155 Ko de source** :

- ~50 Ko (`noyau.rose`) → ~50 Mio de RSS ;
- ~155 Ko (`emetteur.rose`) → > 16 Gio de RSS en ~72 s (facteur > 300 pour un
  facteur ~3 sur la taille du source) ;
- ~440 Ko (`rose_natif.rose`) → 16 Gio en ~5 min 18 ; sans plafond, le même
  travail a atteint 84 Gio en 27 min.

**Seuil pratique : au-delà d'environ 100 Ko de `.rose`, considérer la
compilation comme dangereuse et imposer le garde-fou.** La croissance n'est pas
linéaire : elle est superlinéaire (typiquement quadratique ou pire sur les
structures du compilateur), donc extrapoler depuis un petit fichier est faux.

## 4. Pic RSS vs MemAvailable — deux chiffres qui ne se valent pas

- **Pic RSS** (`VmHWM`, « Maximum resident set size » de `time -v`) : le maximum
  de mémoire **résidente physique** détenue par UN processus (et ses enfants
  attendus) à un instant donné. C'est une valeur **par processus**, qui ne
  descend jamais. Il peut dépasser le plafond du garde-fou à cause de la
  fenêtre de polling de 5 s.
- **MemAvailable** : estimation **système** du noyau (`/proc/meminfo`) de la
  mémoire qu'il pourrait donner à une nouvelle allocation **immédiatement**, en
  comptant le cache reclaimable. Une valeur de 90 Gio ne signifie PAS qu'un
  seul processus peut prendre 90 Gio : les pics de plusieurs compilateurs
  concurrents s'additionnent, et la fragmentation peut empêcher d'atteindre le
  chiffre théorique.
- **SwapFree** est le canary : s'il commence à baisser, le plafond est trop
  haut ou trop de compilateurs tournent en parallèle. Ici il n'a pas bougé.

En résumé : `MemAvailable` dit « combien la machine peut encore donner »,
`pic RSS` dit « combien UN compilateur a réellement pris au maximum ». Le
garde-fou arbitre entre les deux en plafonnant le second.

## 5. Audit des scripts existants

`tests_selfhost/limites_mem.sh` (lecture seule) liste les scripts longs et
vérifie la présence de `garde_fou_mem`/`prof_mem_rose`. Résultat du 22/11 :
**aucun** des `tests_selfhost/*.sh` ni de `scripts/validation_finale.sh` ne
contient le garde-fou — ils utilisent seulement `timeout`, ce qui est
insuffisant. Ils n'ont pas été modifiés (un autre agent peut les utiliser) ;
le rapport les marque `A PROTEGER`.

```bash
tests_selfhost/limites_mem.sh   # code 1 tant qu'il reste des scripts à protéger
```

## 6. Conduite à tenir pour la suite

1. Ne jamais lancer C0/C1/C2 sur `emetteur.rose` ou `rose_natif.rose` sans
   `garde_fou_mem.sh 16777216`.
2. Accepter `rc=75` comme issu normal d'une tentative hors budget, et le
   journaliser.
3. Ne pas augmenter le plafond pour « faire passer » : le problème est
   algorithmique (croissance superlinéaire), pas un manque de RAM.
4. Après tout changement de compilateur ou de quantification de la charge,
   re-mesurer avec `prof_mem_rose.sh -n 3` — les chiffres ci-dessus sont des
   **constats datés**, pas des consignes permanentes.
