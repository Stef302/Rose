# G5 — chaînes UTF-8 dans l'émetteur ELF

Constat et correctif mesurés le 21/11/2026 sur `compilateur/emetteur_elf.rose`
(seul fichier modifié ; `emetteur.rose`, `rose_natif.rose`, `rose/src/*` et les
scripts de build intacts).

## Représentation

Le descripteur d'une chaîne G5 reste `[tag=1][len][données…]`, mais `len`
est désormais le **nombre d'octets UTF-8** et les données sont les octets
UTF-8. Auparavant chaque point de code était stocké dans un seul octet
(tronqué à 8 bits) et `len` comptait les points de code.

Conséquence : `g5_emit_print` écrit directement le bon octet pour octet.

## Routines d'émission modifiées

| Routine | Comportement nouveau |
|---|---|
| `g5_make_str` | encode chaque point de code renvoyé par `g5_cps` en 1–4 octets UTF-8 (`g5_utf8`) ; `len` = nombre d'octets ; hash d'internement toujours calculé sur les points de code. |
| `g5_emit_len` | `longueur(s)` compte les têtes de séquence UTF-8 (octets hors `0b10xxxxxx`), donc les points de code. |
| `g5_emit_ordre` | décode le premier point de code depuis 1 à 4 octets. |
| `g5_emit_car` | `caractere(n)` encode `n` en 1–4 octets UTF-8 (allocation toujours 24 octets). |
| `g5_emit_sous` | `sous_chaine(s,a,b)` convertit les indices `a`,`b` (points de code) en décalages octets en parcourant le descripteur. |
| `g5_emit_strcmp` (nouvelle) | comparaison lexicographique octet par octet, appelée par `g5_emit_cmp_value` pour `<`, `>`, `<=`, `>=` sur deux chaînes. L'ordre UTF-8 = l'ordre des points de code. |

`==`/`!=` utilisent déjà `g5_emit_streq` (égalité d'octets), correcte telle
quelle. `g5_tokens`/`g5_sub`/`g5_noms`/`g5_unescape` manipulent des chaînes
Rose natives et n'ont pas changé.

## Mesures

Avant (émetteur md5 `a3e42bfc…`) / après (md5 `a52b74f8…`), sorties ELF :

| Test | Avant | Après (identique au natif) |
|---|---|---|
| `t250_unicode_chaines` | `e974e90a2d870a80150a330a310a` | `c3a974c3a90ae4b8ade696870af09f9a80f09f9a80a330a310a` |
| `t251_unicode_sous` | `31310ae974e90a72610ae90a` | `31310ac3a974c3a90a72610ac3a90a` |
| `t252_unicode_ordre_car` | `3233330a34350a3132380ae90a2d0a800a` | `3233330a32303031330a3132383634300ac3a90ae4b8ad0af09f9a800a` |
| `t253_unicode_comparaison` | `767261690a767261690ae974e9800a767261690a` | `767261690a666175780ac3a974c3a9f09f9a800a767261690a` |

Batteries (émetteur figé par copie, comparaisons octet pour octet) :

- AZ : avant 27/31 conformes, 4 divergents ; après **31/31 conformes, 0 divergent, 0 bloqué**.
- G5B : avant 24/30 conformes, 6 divergents ; après **29/30 conformes** (le seul restant,
  `g5b513_rat_calc_boucle`, est préexistant et hors Unicode).
- SH9 : avant 121/141 conformes, 13 divergents, 7 bloqués ; après
  **133/141 conformes, 1 divergent (préexistant `g5b513`), 7 bloqués (préexistants)**.
- Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` ×3 (rir, rir, front-end) inchangé.
- Magic des ELF émis : `7f454c4602010100`.
