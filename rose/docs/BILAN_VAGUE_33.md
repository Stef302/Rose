# Bilan vague 33 (19/11/2026) — 6ᵉ victoire, G5 réflexif, malloc wasm

**HEAD poussé** `064a25e`. Les trois agents de la vague 33 ont enfin été
**réellement lancés** (ils avaient été consignés au PLAN mais jamais appelés),
puis livrés.

## 1 — Résultats

| Agent | Livraison | Verdict |
|---|---|---|
| **SE1** | Boucle 1M : 0,259 → **0,099 ms**, ratio Rust **0,377–0,381** | **6ᵉ victoire** (réassociation 8 accumulateurs) |
| **SE2** | Couverture G5 **46,1 % → 99,2 %** ; ELF de `emetteur.rose` entier (327 336 o, 111/111 fonctions) produit | Réflexivité **non atteinte** (segfault à l'exécution) |
| **SE3** | `malloc`/`calloc`/`realloc`/`free`/`exit` sandboxés ; **uthash** (73 749 o) 10/10 | Frontière repoussée, pas « universel » |

### SE1 — double verdict

- **(a) Même algorithme itératif** : ratio ≈ 1,00, V21 est déjà au plancher
  physique (0,2564 ms, 1 addition dépendante/itération) — **pas de victoire**.
- **(b) Meilleur algorithme** : réassociation en **8 accumulateurs indépendants**
  + déroulage ×8, même source Rose, même garde `n≤2³²`, même repli bigint.
  Preuve de borne `acc[j] ≤ n²/16 + 3n/8 = 2⁶⁰ ≪ i64::MAX`.
  **0,259 → 0,099 ms**, Rose bat Rust de **62 %**. Dump : 18 instr/8 it,
  8 chaînes en `x20`–`x27`, 0 `blr`, 0 pile.
- Rejets mesurés : 2 accumulateurs (compteur toujours 1 add/it), A4 (battue),
  16 accumulateurs (spill, 38 `ldur`), forme close (contourne le bench).
- Aucune régression causale sur D5/D6 (duel dédié 12 tirages : 0,9887).

### SE2 — self-hosting

- Formes ajoutées : `(s "…")` avec tokeniseur/échappements, runtime
  `[tag][len][octets]` sur tas bump ; `(idx)`, `lstvide`, `(lst)`, `joindre`
  (`[tag][len][cap]`, heap RW `0x60000000`, 2 `PT_LOAD`) ; primitives réelles
  `longueur`/`ordre`/`sous_chaine`/`chaine`/`entier` ; stubs documentés.
- Couverture sur le vrai RIR de `emetteur.rose` (3 614 ops / 111 fonctions) :
  **1 667 → 3 585 ops (99,2 %)**, 100 % avec stubs.
- `BASE` extraite de `emetteur.rose` → ELF 1 532 o → `BASE("a|b")="a"` rc=0.
- `emetteur.rose` entier → ELF 327 336 o, 111/111 fonctions, **rc=0 d'émission**,
  mais **SIGSEGV rc=139** à l'exécution (`joindre` reçoit `x0=1`).
- **Verdict** : frontière reculée de « émission refusée rc=2 » à « ELF produit
  mais non exécutable ». **Toujours pas full-hosted** : 0 % sans Rust,
  G2/G3/G4.2/G4.3/G4.4 intacts.

### SE3 — wasm

- 5 hôtes `env` : `malloc`/`calloc`/`realloc`/`free`/`exit`. Tas **au-delà** de
  la mémoire déclarée (`__stack_pointer=66576` dans 2 pages → tas à 131072,
  aucun chevauchement), alignement 16 o, **plafond dur 32 Mio** → `NULL`,
  `exit` = erreur propre. Option `--malloc` au convertisseur.
- **uthash** (BSD, 73 749 o) convertie `--malloc --opt 2` : ancien binaire
  refusait `env.malloc`, nouveau **10 OK / 0 échec** ; stress 400 entrées.
- Frontière maintenue : `free` = no-op à fuite bornée ; libc complète,
  variadiques, exceptions C++, `v128`, threads, `externref`, tables multiples,
  ABI structurées restent dehors. **Pas « n'importe quelle bibliothèque »**.

## 2 — État du bench

**6 victoires nettes** : fib 0,91× · factorielle 0,22× · premiers 0,85× ·
PGCD 0,70× · chaîne 0,70× · **boucle 0,38×**.
**3 domaines au niveau** : table, E/S, racines (Rust gagne à algo égal).

Constat daté, pas consigne.
