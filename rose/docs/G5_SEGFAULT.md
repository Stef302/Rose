# G5 — diagnostic du segfault de l'ELF auto-écrit (19/11/2026)

## État initial (SE2)

`emetteur.rose` entier → RIR (3 614 ops) → `emetteur_elf.rose` → **ELF 327 336 o,
111/111 fonctions émises**, `rc=0` d'émission. Exécution : **SIGSEGV rc=139**,
pc `0x44fc1c`, `joindre` recevant `x0=1` au lieu d'un descripteur.

## Avancées faites ici (orchestrateur, après 2 échecs d'infrastructure du sous-agent)

1. **Régions de gratte-papier par profondeur** (déjà présentes dans l'arbre) :
   `g5_tmpoff/argoff/listoff` indexées par profondeur d'imbrication `$d`, frame
   9 232 o découpée en `sub sp` de 4 080 (immédiat 12 bits). Le `x0=1` est devenu
   `x0=2` puis une valeur packée — la cause n'était donc pas seulement l'écrasement
   d'arguments imbriqués.
2. **Label manquant identifié** : instrumentation de `g5_patch` — 140 appels vers
   le label **3859** (`$g5_chl`, helper `chaine`) qui n'était **jamais émis** :
   `g5_emit_ch` n'existait pas et n'était pas appelé. Conséquence : `g5_labelidx`
   renvoyait `-1` → `bl 0x4000ac` (4 octets avant le début du code) → **SIGILL**.
   Correctif : ajout de `g5_emit_ch` (identité si tag=1, chaîne vide sinon, avec
   garde de plage sur le tas) et appel dans la séquence des runtime helpers.
3. Après correctif, l'ELF de `emetteur.rose` (334 092 o) **démarre** mais segfault
   encore à `pc=0x4514ec`, soit `g5_emit_len` : `ldr x1,[x0]` avec
   **`x0 = 0x120002b40`** — une valeur packée, pas un pointeur `[tag][len]`.

## Cause restante (non réparée)

**Incohérence de l'ABI des valeurs dans G5** : les entiers sont passés bruts ou
packés (`0x120002b40`) tandis que les helpers chaîne/liste (`len`, `ordre`, `sous`,
`idx`, `join`) supposent un pointeur vers un descripteur `[tag][len][données…]`
dans le tas à `0x60000000`. Tant que les slots ne portent pas un tag explicite
cohérent, l'ELF auto-écrit ne peut pas exécuter le compilateur qui l'a produit.

## Verdict

- Émission : **111/111 fonctions**, rc=0 — progrès réel et vérifié.
- Exécution : **toujours SIGSEGV** — réflexivité fonctionnelle **non atteinte**.
- Self-hosting : **PARTIEL** ; `rose/rose` exige toujours `cargo build`.
- Nouveau point fixe M4.5 (indépendant) : `8ac53eed3d4967a0d7d9ab333256801c`.

Constat daté, pas consigne. Pour aller plus loin il faut uniformiser le tag des
valeurs dans G5 (representative unique des entiers/chaînes/listes), pas ajouter
des gardes de plage.
