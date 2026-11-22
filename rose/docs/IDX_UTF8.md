# Indexation G5 des chaînes par point de code — diagnostic du 22/11/2026

Périmètre : `compilateur/emetteur_elf.rose` uniquement (`g5_emit_idx`).
Aucun autre fichier modifié ; C0 non réémis, `rose_natif.rose` non régénéré,
`emetteur.rose` intact (point fixe `b3ed9262…`).

## 1. Cause racine (mesurée)

`g5_emit_idx` (ancien code, l.2921–2951) indexait les chaînes **PAR OCTET** :

```
g5_emit(g5_asr3(7, 1))       # i
g5_emit(addi(5, 7, 16))      # i + 16
g5_emit(ldrbr(4, 6, 5))      # octet à [descripteur + i + 16]
```

et fabriquait un descripteur d'**1 octet** via une table d'internement à
`0x60001300` (commentaire : « table @0x60001300 par valeur d'octet »).

Or le moteur Rust indexe les chaînes **PAR POINT DE CODE**. Depuis le
correctif UTF-8, les descripteurs G5 contiennent des octets UTF-8
(`[tag=1][len_octets][octets…]`). Le lexer du compilateur embarqué lit
`$src[$j]` (emetteur.rose l.3510, 3519, 3521–3523) : il tombait donc sur des
octets de continuation au lieu des caractères.

Conséquences mesurées avec C0 (`/tmp/opencode/C0_final.elf`) :
- `>> "≈"` → ELF émis mais `e0 a0 a9` au lieu de `e2 89 88` ;
- `>> longueur("≈")`, `>> ordre("≈")`, `>> sous_chaine("x≈y",1,2)` → SIGSEGV
  pendant la compilation par C0 ;
- `fn p(a,b){ si (a==63 et b==61) { retour "≈" } … }` → timeout > 90 s.

L'hypothèse `eval_expr` a été **invalidée** par sonde (7/7 identique au
natif) ; les primitives G5 `longueur`, `ordre`, `caractere`, `sous_chaine`
sont saines. Voir `EVAL_EXPR_UNICODE.md`.

## 2. Sondes avant / après (émetteur hébergé, ELF exécuté)

Source : `/tmp/opencode/probe_idx.rose` (12 cas : Unicode 1/3/4 octets,
ASCII, `ordre`/`longueur` sur un caractère indexé).

Référence native (`ROSE_SANS_JIT=1`) — hex :

```
e289 880a e289 880a e289 880a 3837 3736
0a31 0af0 9f9a 800a 610a 630a c3a9 0a74
0ae6 9687 0a31 0a
```

**AVANT** (ancien `g5_emit_idx`) — hex :

```
e20a e20a e20a 2d31 3238 0a31 0af0 0a61
0a63 0ac3 0aa9 0ab8 0a30 0a
```

`"≈"[0]` → `e2` seul ; `ordre("x≈y"[1])` → `-128` (octet de continuation
pris pour une tête de séquence) ; `"🚀"[0]` → `f0` seul ; `"été"[1]` → `a9` ;
`"中文"[1]` → `b8` ; `longueur("x≈y"[2])` → `0`. ASCII seul correct (`a`, `c`).

**APRÈS** : sortie ELF **identique octet pour octet** à la référence native
sur les 12 cas (`cmp` = 0). Hors bornes (`"abc"[10]`, `"abc"[-1]`, `""[0]`,
`"≈"[1]`) → chaîne vide, rc=0, aucun crash.

## 3. Correctif

`g5_emit_idx` balaie maintenant le descripteur depuis l'octet 0 et compte
les têtes de séquence UTF-8, en réutilisant exactement le motif de
`g5_emit_sous` (l.2849–2883) : `cmpi 128 / 224 / 240` → `movz k` (1/2/3/4).
À chaque itération, le point de code à `bo` est décodé **avant** le test
`ci == i`, ce qui garantit que `k` est toujours initialisé (y compris pour
`i == 0`). Une fois l'indice atteint, on alloue un descripteur
`[tag=1][len=k][k octets]` et on copie les k octets avec `ldrbr`/`strbr`
(comme `g5_emit_sous`). Les 10 labels du scan sont pris par `g5_next()` en
tête de fonction, donc sans toucher à la réservation `$g5_idxl` (+1 liste,
+3..+7 table). Le comportement ASCII (k=1) et les chemins liste/table sont
inchangés. La table d'internement `0x60001300` n'est plus utilisée ici.

## 4. Vérifications (constats datés du 22/11/2026)

| Contrôle | Résultat |
|---|---|
| `rose verif compilateur/emetteur_elf.rose` | `vérifié`, rc=0 |
| Sondes 12 cas natif vs ELF | identiques octet pour octet |
| Batterie AZ (`tests_selfhost/lancer_az.sh`) | **31/31 CONFORMES**, 0 divergent, 0 bloqué |
| AZ comparaisons natif/ELF, RIR, ELF, sortie | 31/31 sur les 4 |
| Sonde `u16le(183)` (octet_brut intact) | `b7 00` en tête d'ELF (offset 18) |
| Point fixe `rose rir compilateur/emetteur.rose` | `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o, ×2 |
| Fichiers interdits (`emetteur.rose`, `rose/src/*`, `generer_rose_natif.py`, `rose_natif.rose`) | intacts (`git status` muet) |

md5 final de l'émetteur modifié : `95535e514c8834144684e5ca62073851`.
