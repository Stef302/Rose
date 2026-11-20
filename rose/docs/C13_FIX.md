# C13_FIX — F1 ≠ F2 : encodage des littéraux non ASCII (constat mesuré, 21/11/2026)

Sous-agent `sh11`. Dépôt `/home/clausake/Stefan/ROSE`, HEAD de départ
`10c51f19bf331029e99ddb27b25761d32399fb41`. Deux fichiers possédés, et
**aucun autre modifié** : `compilateur/emetteur_elf.rose` et ce document.
Aucun commit, aucune publication. Artefacts : `/tmp/opencode/sh11_agents/`.

---

## 1. Reproduction de l'écart (artefacts de l'orchestrateur)

Mesures reprises telles quelles dans `/tmp/opencode/c13_verify/` :

| Artefact | Taille | md5 | Rôle |
|---|---|---|---|
| `e_final.rir` | 301 903 | `75c36be2…` | RIR natif de `emetteur_elf.rose` (source avant correctif C1.3) |
| `E1.elf` | 428 808 | `22873d82…` | émis par `rose emetteur_elf.rose e_final.rir` |
| `E1_32g.elf` | 428 808 | `0898c74d…` | `E1.elf` avec `p_memsz` patché 8 → 32 Gio (seuls 5 octets d'en-tête diffèrent) |
| `E2.elf` | 0 | — | `./E1.elf e_final.rir` → SIGSEGV, `E2.err` = « timeout: the monitored command dumped core » |
| `E2_32g.elf` | 428 832 | `3e248c8e…` | `./E1_32g.elf e_final.rir` → rc=0 ; **+24 octets**, différent de E1 |

Le crash était bien le tas à `0x260000008` (~10 Gio) hors du mapping RW de
8 Gio ; le commit `75f21a6` porte le mapping à 32 Gio (`0x800000000`) et
supprime le SIGSEGV. **Mais l'écart F1/F2 subsiste après ce correctif** : il
ne vient donc pas du mapping.

## 2. Première divergence localisée (preuve)

Comparaison octet par octet de `E1_32g.elf` et `E2_32g.elf` :

- offset 1-based 97 / 105 / 109 / 165 : champs d'en-tête (`p_filesz`,
  `p_memsz`) — légitimes (E2 réémet l'ancienne constante 8 Gio, puisque le
  code de E1 n'a pas été repatché, seul son en-tête l'a été) ;
- **premier octet de code divergent : offset fichier `0x608`** (1-based 1545),
  `a9` (octal 251) dans E1, `af` (octal 257) dans E2.

Désassemblage (AArch64, `--adjust-vma=0x4000b0`, texte à partir de `0xb0`) :

```
E1  400608: 94019ea9  bl  0x4680ac
E2  400608: 94019eaf  bl  0x4680c4      ; cible +0x18 = +24 octets
```

Les cibles de saut sont décalées de 24 octets : 6 mots (24 o) sont insérés
**avant** `0x4680ac`. Alignement mot à mot (ancres exactes) :

| Insertion | Adresse E1 | Drift | Contenu inséré dans E2 |
|---|---|---|---|
| 1 | `0x4509c0` | +4 mots (+16 o) | 2 paires `mov x1,#imm ; strb w1,[x0,#…]` |
| 2 | `0x451538` | +2 mots (+8 o) | 1 paire `mov x1,#imm ; strb w1,[x0,#…]` |

Les paires `mov`/`strb` sont exactement le patron d'émission d'un caractère
de chaîne dans `g5_make_str`. Lecture des deux littéraux :

| Littéral | E1 (natif) | E2 (auto-émis) |
|---|---|---|
| `(s "forme parenthésée non couverte : ")` | `longueur` 33 (0x21), `é` = **1 octet `e9`** (deux fois) | `longueur` 35 (0x23), `é` = **2 octets `c3 a9`** (deux fois) |
| `(s "appel non résolu : ")` | `longueur` 19 (0x13), `é` = **1 octet `e9`** | `longueur` 20 (0x14), `é` = **2 octets `c3 a9`** |

Preuve complémentaire : les deux clés de table d'internement changent aussi
(`0x00000095ec0000cf09` → `0x000000fdd700007871` en `0x45086c`, et
`0x000000012f00006b6b` → `0x000000b7590000100c` en `0x451444`),
conséquence directe du hash calculé sur des octets différents —
pas une table non réinitialisée.

Les 87 634 octets qui diffèrent après désalignement sont des décalages
d'adresses de saut, des clés de hash et les octets de ces deux chaînes.
**Aucun compteur d'émission, aucun label `g5_next` et aucune globale non
restaurée n'est en cause.**

## 3. Cause racine (mesurée)

`rose rir` (natif) lit le source/RIR comme des **points de code Unicode** :

```
>> longueur("é≠→—")                        -> 4
>> ordre(sous_chaine("é≠→—",0,1))           -> 233     (0xE9)
>> ordre(sous_chaine("é≠→—",1,2))           -> 8800    (0x2260)
```

L'ELF G5 lit les fichiers comme des **octets** (`lire_texte` = openat/read).
Dans `g5_make_str`, `longueur($s)` et `ordre(sous_chaine($s,$i,$i+1))`
produisent donc des valeurs différentes pour tout littéral non ASCII :
2 octets `c3 a9` sous G5 contre 1 point de code `233` en natif. L'émetteur
auto-exécuté n'est donc **pas** un point fixe de son propre code pour les
chaînes accentuées. C'est une vraie non-idempotence déterministe, pas un
succès simulé ni un état interne sale.

Le RIR `e_final.rir` contient exactement 4 points de code non ASCII : les
deux `é` de chaque chaîne ci-dessus (sous forme UTF-8 `c3 a9` sur disque).

## 4. Correctif appliqué

Ajout de `g5_cps($s)` dans `compilateur/emetteur_elf.rose` (avant
`g5_make_str`) et utilisation de son résultat pour le hash, la taille et
l'émission des octets. `g5_cps` recombine les suites UTF-8 valides
(`c2..df + 80..bf`, `e0..ef + 2×80..bf`, `f0..f4 + 3×80..bf`) en un **point
de code entier** (ex. `e2 89 a0` → `8800`).

- Sous G5 : `c3 a9` → `233`, longueur 1, hash sur 233 — identique au natif.
- En natif : les chaînes sont déjà des points de code ; `233` ou `8800`
  ne sont jamais suivis du nombre requis de continuations `80..bf` dans
  nos littéraux, donc `g5_cps` est **identité** (vérifié : 0 suite ambiguë
  sur les deux sources Rose et les deux RIR ; point fixe inchangé, §6).
- `strbio` ne stocke que l'octet de poids faible, donc l'émission reste
  conforme à l'ABI tandis que le hash utilise le point de code entier,
  exactement comme en natif.

`rose verif compilateur/emetteur_elf.rose` (binaire `./rose/rose`) →
**`vérifié`**, rc=0. Le mapping 32 Gio du commit `75f21a6` est conservé.

## 5. Rejeu F1 / F2

### 5.1 Référence avant correctif (rejeu de l'orchestrateur, source HEAD)

Chaîne exacte :

```
./rose/rose rir compilateur/emetteur_elf.rose > /tmp/opencode/c13_verify/final.rir
./rose/rose compilateur/emetteur_elf.rose final.rir > F1.elf
./F1.elf final.rir > F2.elf
```

| Artefact | Taille | md5 |
|---|---|---|
| `final.rir` | 301 904 | `946d0524…` |
| `F1.elf` | 428 808 | `8e887d4c…` (`p_memsz` = 32 Gio : le crash est corrigé) |
| `F2.elf` | **428 832** | `d39f6fc0…` |

`cmp -l F1 F2` : première divergence à l'offset 1-based 1545 = `0x608`,
octets `251` (E1/F1) vs `257` (F2) — **exactement la même divergence que
E1/E2**, 87 629 octets différents, +24 octets. Donc le commit `75f21a6`
(mapping 32 Gio) corrige le SIGSEGV mais **pas** la non-idempotence.

### 5.2 Après correctif — SUCCÈS

Md5 final de l'émetteur : **`8df588b0a77f663a61e5eef39a53509f`**
(`rose verif` → `vérifié`, rc=0).

Chaîne exacte (artefacts `/tmp/opencode/sh11_agents/`) :

```
./rose/rose rir compilateur/emetteur_elf.rose > e_fix.rir
# 304 755 o, md5 71d1580641955166580aab29391c6028
./rose/rose compilateur/emetteur_elf.rose e_fix.rir > F1.elf
./F1.elf e_fix.rir > F2.elf
cmp F1.elf F2.elf
```

| Artefact | Taille | md5 | rc |
|---|---|---|---|
| `F1.elf` | 432 544 | `6f2715e5c5fe26c3e26f6ac404051bd2` | 0 |
| `F2.elf` | 432 544 | `6f2715e5c5fe26c3e26f6ac404051bd2` | 0 |

**`cmp` : IDENTIQUE octet pour octet.** Les deux ELF ont `p_memsz =
0x800000000` (32 Gio) et s'exécutent sans patch manuel.

Contrôle fonctionnel complémentaire : `./F1.elf tiny2.rir` produit un ELF
de 4 328 o, md5 `c7f823e1…`, **identique** à l'ELF produit par l'émetteur
natif sur le même `tiny2.rir` ; exécuté, il affiche `3` et rend rc=0.

## 6. Point fixe b3ed9262 — NON CASSÉ (×2)

Chaîne rejouée deux fois avec l'émetteur corrigé :

```
./rose/rose rir compilateur/emetteur.rose > fix.rir      # 263 520 o, b3ed9262…
./rose/rose compilateur/emetteur_elf.rose fix.rir > fix_host.elf
./fix_host.elf compilateur/emetteur.rose > fix_dump.out
cmp fix_dump.out fix.rir
```

| Passage | ELF émis | Dump | Résultat |
|---|---|---|---|
| 1 | 542 212 o, `05a15ab3…`, rc=0 | 263 520 o, `b3ed9262…`, rc=0 | **IDENTIQUE** |
| 2 | 542 212 o, `05a15ab3…`, rc=0 | 263 520 o, `b3ed9262…`, rc=0 | **IDENTIQUE** |

Le dump RIR natif a aussi été refait deux fois : `b3ed9262dd54776a6dbd07bc12368d0e`
les deux fois, `cmp` identique.

## 7. Validation (toutes mesurées)

| Contrôle | Résultat |
|---|---|
| `./rose/rose verif compilateur/emetteur_elf.rose` | **`vérifié`**, rc=0 |
| Point fixe `b3ed9262` | identique, ×2 (dump ELF) + ×2 (dump natif) |
| `F1` vs `F2` | **md5 identiques**, `cmp` identique |
| Test critique `t999` | **CONFORME**, 20/20 lignes, natif = ELF octet pour octet |
| Batterie SH9 (80 programmes) | **57 CONFORMES / 16 DIVERGENTS / 7 BLOQUÉS** — identique à la référence SH10, aucune régression |
| `cargo test --release -- --test-threads=1` | **rc=0, 515 passés, 0 échec**, 6 ignorés (258+242+6+8+1) |
| fib25 (`t74_fib_rec`) | **75025 / 75025** (natif / ELF) |

`git status --short` : seul `compilateur/emetteur_elf.rose` est modifié, plus
ce nouveau document. Aucun commit, aucune publication, aucun fichier
interdit touché.

## 8. Reste

1. Le correctif repose sur l'absence de suite UTF-8 ambiguë dans les
   littéraux des deux sources et des deux RIR (vérifié : 0 occurrence). Un
   littéral natif contenant un point de code `C2..F4` suivi du nombre requis
   de points de code `80..BF` serait décodé à tort ; ce n'est pas le cas ici
   et le point fixe ×2 le confirme.
2. Les divergences préexistantes documentées par SH7/SH9/SH10 restent
   inchangées (rationnels négatifs, division/modulo négatifs, Unicode par
   octets, grands entiers ABI, `type`/`abs`/`min`/`max`, graine/PRNG,
   blocages de langage). La batterie donne exactement le même compte
   57/16/7 qu'avant le correctif.
3. `rose verif` doit être appelé via le binaire du dépôt `./rose/rose` ; le
   `rose` installé dans `~/.local/bin` est une version 2.0 sans sous-commande
   `verif` (mesuré : « fichier introuvable : verif »).
