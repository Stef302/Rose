# Self-hosting A–Z — jalon final (22/11/2026)

## État au moment de l'écriture

**Corrige et vérifié côté émetteur hébergé** ; chaîne C0→C1→C2 en cours
(script détaché `/tmp/opencode/final_chain.sh`, journal
`/tmp/opencode/final_chain.log`).

### 1. Message G5ERR (rc=2 silencieux)

Voir `G5_ERR_FINAL.md`. Message exact lu en mémoire avec GDB :

```text
G5 : sous_chaine invalide : 5..4 sur 4 octets
```

### 2. Cause racine

`g5_eval` (emetteur_elf.rose, bloc « G5-TAS : fusion
`sous_chaine(s,a,b) == "littéral"` ») lisait l'octet à
`descripteur + 16 + a`, c.-à-d. traitait l'indice `a` comme un décalage
**octet**, alors que les indices Rose/G5 sont des **points de code** et que
le descripteur contient des octets UTF-8. Conséquence : dès qu'un caractère
multi-octets précédait `a`, la comparaison décalait de `k-1` octets.

Dans le compilateur embarqué, `g4_lignes()` utilise
`sous_chaine($s,$i,$i+1) == "\n"` : le RIR d'un programme contenant
`retour "≈"` se retrouvait découpé en lignes désynchronisées (`ops\n`, 4
caractères), puis `g5_nu_op` appelait `g5_sub(ligne,5,4)` → `g5_fail` →
`quitter(2)` sans message (stub `afficher_erreur`).

### 3. Correctif (un seul fichier source modifié)

`compilateur/emetteur_elf.rose`, bloc de fusion G5-TAS uniquement :
- l'indice de point de code `a` est converti en décalage octet `bo` en
  balayant le descripteur UTF-8 (motif identique à `g5_emit_sous` /
  `g5_emit_idx` : `cmpi 128 / 224 / 240` → `k` = 1/2/3/4) ;
- les **octets UTF-8 du littéral** sont tous comparés (`g5_cps` + `g5_utf8`
  à l'émission), au lieu d'un octet par point de code ;
- 6 labels supplémentaires pris par `g5_next()` ; aucun changement d'ABI,
  aucune routine ajoutée, aucun autre fichier source touché.

Le fichier `emetteur.rose` reste intact (point fixe `b3ed9262` vérifié).

### 4. Preuves déjà mesurées (émetteur hébergé corrigé)

| Contrôle | Résultat |
|---|---|
| `rose verif compilateur/emetteur_elf.rose` | `vérifié` (rc=0) |
| Point fixe `rose rir emetteur.rose` | md5 `b3ed9262dd54776a6dbd07bc12368d0e` |
| Point fixe `rose emetteur.rose emetteur.rose` | md5 `b3ed9262dd54776a6dbd07bc12368d0e` |
| Batterie AZ 31 programmes (`lancer_az.sh`) | **31/31 conformes**, 0 divergent, 0 bloqué ; natif/ELF, RIR×2, ELF×2, sortie×2 tous 31/31 — journal `/tmp/opencode/az_after_fix.log` |
| Sondes idx UTF-8 12 cas (`probe_idx.rose`) | sortie ELF **identique octet pour octet** au natif (`cmp=0`) |
| `u16le(183)` | `02 00 b7 00` dans l'en-tête ELF émis (`e_machine`) |
| Sonde `>> sous_chaine("≈\nx\n",1,2) == "\n"` | `vrai` (avant : `faux`) |
| `f2` (boucle sur 4 caractères multi-octets) | `NYNY` = natif (avant : `NNNY`) |
| `p3.rir` (`retour "≈"`) émis par l'hébergé | ELF 9104 o, exécution `e2 89 88 0a` |

La batterie AZ a été relancée avec une copie figée de l'émetteur corrigé :
`md5=96e5835e7c2512cbcb374b84f545956f`, binaire Rose inchangé
`md5=7614a534b9411917ef9cd49a834014a8`.

### 5. Chaîne C0 → C1 → C2 (en cours)

Le diagnostic détaché du 11:13 UTC (PID 2889184) patchait encore
`compilateur/rose_natif.rose` ; le script final attend son message
`composite restaure` avant de régénérer le composite, afin d'éviter que la
restoration n'écrase le composite neuf.

 Étapes automatisées dans `/tmp/opencode/final_chain.log` :
1. `python3 scripts/generer_rose_natif.py` + `rose verif compilateur/rose_natif.rose` ;
2. `rose rir compilateur/rose_natif.rose > rnat_final.rir` ;
3. `rose compilateur/emetteur_elf.rose rnat_final.rir > C0_final2.elf` (~80 min) ;
4. sonde p3 + fib (contrôle magic `0200b700`, sortie `317811`) ;
5. `C0_final2.elf rose_natif.rose > C1.elf`, `C1.elf rose_natif.rose > C2.elf`, `cmp C1 C2`.

Les résultats réels de 1–5 seront ajoutés ici dès la fin du script ; aucune
affirmation de réussite n'est faite avant preuve de sortie.

## 6. Échec matériel de la chaîne C1 (22/11, 14:39 UTC) — cause mesurée

`C0_final2.elf compilateur/rose_natif.rose > C1.elf` a consommé jusqu'à
**84 Gio de RAM (65,8 % de la machine)** en 27 minutes, puis le noyau a
épuisé le swap (`SwapFree` = 48 ko sur 16 Gio) et le processus est passé en
état `D` (attente I/O sur disque), CPU tombé de 99,9 % à 34,2 %.
Le processus a été arrêté pour rendre la machine utilisable. `C1.elf` = 0 octet.

Ce n'est PAS un bug de compilation: c'est une limite d'implémentation du
runtime G5 — allocateur « tas montant » (`g5_alloc_inline`, `[0x60000000]`)
sans récupération mémoire. Compiler la source du compilateur (440 Ko) crée
des centaines de millions de descripteurs de chaînes/listes qui ne sont
jamais libérés. Le compilateur hébergé (Rust, `Rc`) n'a pas ce problème mais
met > 2 h (boucles O(n²) interprétées).

Conséquence: `C1 == C2` n'est PAS atteint. Prochain changement nécessaire
avant de rejouer la chaîne: récupération mémoire dans le runtime G5
(compactage ou réutilisation des descripteurs temporaires de l'émetteur),
ou réduction du pic en émettant par morceaux. Aucun résultat de self-hosting
complet n'est revendiqué.
