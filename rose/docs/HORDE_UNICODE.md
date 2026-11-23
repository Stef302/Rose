# Unicode complet dans le moteur auto-hébergé (J6 §11)

Worktree `/tmp/opencode/horde_unicode`, HEAD `101b48d`. Trois fichiers
possédés, tous nouveaux : `compilateur/unicode_rose.rose` (bibliothèque),
`compilateur/tests_unicode_rose.rose` (pilote 80 cas), le présent document.
Aucun autre fichier du dépôt n'a été modifié ; aucun commit, aucun push.
Garde-fou `scripts/garde_fou_mem.sh 12000000 …` sur toutes les commandes longues.

## 1. Ce qui est livré

`unicode_rose.rose` implémente en Rose pur, sans appui Rust pour l'algèbre
des octets :

| Fonction | Rôle |
|---|---|
| `u_cont`, `u_tete` | classification des octets (continuation `10xxxxxx`, tête 1–4 octets avec rejet de `C0/C1` et `F5..FF`) |
| `u_decode(bytes,k)` | décodage 1–4 octets d'un point de code ; `-1` sur sur-longue, continuation manquante/fausse, surrogate `D800..DFFF`, `> U+10FFFF`, troncature |
| `u_encode(cp)` | encodage 1–4 octets par arithmetic (mod/div) ; liste vide si `cp<0` ou `cp>0x10FFFF` |
| `u_cps`, `u_bytes`, `u_points`, `u_texte`, `u_brut` | conversions chaîne ↔ points de code ↔ octets UTF-8 |
| `u_longueur`, `u_idx`, `u_sous`, `u_compare` | longueur en points de code (prouvée par décodage), indexation, sous-chaîne `[a,b)`, comparaison lexicographique des OCTETS UTF-8 (`-1/0/1`) |
| `u_ordre`, `u_caractere` | premier point de code ; code point → chaîne UTF-8 |
| `u_tab`, `u_dec_base`, `u_dec_mark`, `u_compose`, `u_normalise` | NFC élémentaire : table de 42 paires Latin-1 précomposé ↔ (base, diacritique), décomposition puis recomposition des paires adjacentes |

Représentation : listes `liste<entier>` **à sentinelle** (indice 0 = 0,
éléments logiques 1..`longueur-1`), même convention que `moteur_ops.rose`.
Raison : `[]` littéral est refusé par le front-end natif (« type d'élément
introuvable »). Aucune globale, aucun paramètre nommé `i` (l'imaginaire),
aucune affectation par index.

`octet_brut` est redéfini localement (`caractere($v % 256)`) pour que
l'appel soit accepté par le front-end ; le dispatch G5 l'intercepte et écrit
un descripteur d'**un seul octet brut**, exactement comme dans
`emetteur_elf.rose`.

## 2. Couverture des tests — 80 cas (≥ 50 demandés)

Le pilote s'ajoute au module par concaténation (Rose n'a pas d'include) :

```sh
cat compilateur/unicode_rose.rose compilateur/tests_unicode_rose.rose \
    > /tmp/opencode/unicode/prog.rose
rose verif /tmp/opencode/unicode/prog.rose           # vérifié, rc=0
ROSE_SANS_JIT=1 rose /tmp/opencode/unicode/prog.rose > nat.out
C0 /tmp/opencode/unicode/prog.rose > prog.elf && ./prog.elf > elf.out
cmp nat.out elf.out                                   # 0
```

| Section | Cas | Détail |
|---|---|---|
| 1. Encodage | 16 | U+0000, U+0041, U+007F, U+0080, U+00E9, U+00FF, U+0100, U+07FF, U+0800, U+20AC, U+2C9D, U+4E2D, U+FFFF, U+10000, U+1F680 (🚀), U+10FFFE — chaque hex attendu (`00`, `c280`, `f09f9a80`…) **et** égalité `u_caractere(cp) == caractere(cp)` au moteur Rust |
| 2. Décodage | 16 + 8 | les 16 mêmes points, comparés à `ordre(caractere(cp))` du Rust ; 8 invalides : continuation seule `80`, sur-longs `C0 80` / `E0 80 80`, continuation fausse `E2 28 A1`, surrogate `ED A0 80`, `> U+10FFFF` (`F4 90…`), tronqué `F0 9F`, `FF` → tous `-1` |
| 3. Longueur | 6 | vide, ASCII, latin1 (`été`), grec (`π≈λ`), CJK (`中文`), astral (`🚀🌕`) — comparé à `longueur()` natif |
| 4. idx | 6 | `été[1]`, `x≈y[1]`, `中文[1]`, `🚀🌕[1]`, hors bornes positif/négatif — comparé à `ordre(sous_chaine(…))` |
| 5. sous_chaine | 5 | accentué, ≈, CJK, deux astraux consécutifs, intervalle vide — comparé à `sous_chaine()` natif |
| 6. Comparaison | 7 | `a<b`, égalité, préfixe, latin1, grec (`≈ > λ`), astral (`🚀 > 中`), chaînage — comparé aux opérateurs natifs |
| 7. ordre / caractère | 4 | `π`, `🚀x`, chaîne vide (`-1`), aller-retour `ordre(u_caractere(128640))` |
| 8. Normalisation | 4 | `e + U+0301 → é`, déjà composé, `cafe + U+0301 → café`, non-latin inchangé |
| 9. Allers-retours | 4 | `u_points(u_bytes(s))` sur `été`, `π≈`, `中文`, `🚀` |
| 10. Bornes / erreurs | 4 | `u_encode(-1)`, `u_encode(0x110000)`, `u_decode` hors bornes, hex des octets de `Aé中🚀` = `41c3a9e4b8adf09f9a80` |

Chaque ligne imprime `label=valeur` et, quand un équivalent natif existe,
`,rust=<référence>` : la parité avec le moteur Rust est donc **visible dans
la sortie**, pas seulement affichée dans ce document. Les 80 lignes native
et ELF ont le même md5.

## 3. Parité octet pour octet (constats mesurés le 23/11/2026)

Source concaténée : md5 `facd1dedca954083fd8e23280813fbcd`, 20 132 octets.

| Exécution | Sortie | md5 |
|---|---|---|
| `ROSE_SANS_JIT=1 rose natif prog.rose` | 80 lignes | `75fd783aed198849d11ce536d417e40f` |
| ELF émis par `C0_final2.elf` (1 598 944 o) | 80 lignes | `75fd783aed198849d11ce536d417e40f` |
| ELF émis par `C0_opt.elf` | 80 lignes | `75fd783aed198849d11ce536d417e40f` |
| ELF émis par `C0_v4.elf` | 80 lignes | `75fd783aed198849d11ce536d417e40f` |
| ELF émis par `C0_ext.elf` | 80 lignes | `75fd783aed198849d11ce536d417e40f` |
| ELF émis par `rose emetteur_elf.rose prog.rir` | 80 lignes | `75fd783aed198849d11ce536d417e40f` |

`cmp` = 0 partout. Les quatre ELF « C0_final2/opt/v4/ext » sont
**bit-identiques** entre eux (md5 `49ce31a29daf1c264480025c7c5b965e`,
111 536 octets, magic `7f454c4602010100`) ; l'ELF émis par la source
`emetteur_elf.rose` diffère dans ses octets (111 812) mais rend la même
sortie — la parité exigée porte sur la sortie, elle est totale.

## 4. Cas particuliers documentés

1. **`C0_final.elf` (binaire du 22/11 07:25) est ANTÉRIEUR au correctif
   d'indexation UTF-8** décrit dans `IDX_UTF8.md`. Sur ce programme il rend
   rc=2 silencieux ; le message lu en mémoire par GDB au point d'erreur
   `0x403dec` est `G5 : sous_chaine invalide : 5..4 sur 4 octets`. Ce n'est
   PAS un défaut de `unicode_rose.rose` : les quatre C0 postérieurs au
   correctif (`C0_final2`, `C0_opt`, `C0_v4`, `C0_ext`) émettent un ELF
   correct, tout comme la source courante `emetteur_elf.rose`. Le verdict
   « tourne dans un ELF C0 » est donc établi sur les C0 corrigés.
2. **`caractere(1114111)` est refusé par le moteur Rust natif**
   (« code hors plage (0 à 1114111) » — test de borne exclusif). Le banc
   utilise donc U+10FFFE (`1114110`) comme borne supérieure, pour que la
   référence Rust et l'ELF C0 s'exécutent tous les deux. `u_encode` accepte
   bien U+10FFFF (valeur Unicode correcte) dans la bibliothèque.
3. **`u_caractere` ne confond pas octets et points de code** : elle
   encode puis *décode* avec `u_points` avant `caractere`. Une version
   naïve `u_texte(u_encode(cp))` produirait `Ã©` au lieu de `é` sous le
   moteur Rust (deux code points au lieu d'un) ; c'était mesurable comme
   `rust=NO` sur les 14 premiers cas avant correction.
4. **`u_brut` (octets bruts via `octet_brut`) est réservé à `ecrire_bytes`** :
   sous Rust il donne des code points latins-1 que `ecrire_bytes` tronque
   à 1 octet, sous G5 un descripteur d'octets nus. Les deux voies écrivent
   les mêmes octets dans un fichier, mais ce n'est PAS une chaîne à
   afficher ni à comparer avec `==` ; pour l'affichage on repasse toujours
   par `u_texte`/`caractere`.
5. **Ordre UTF-8 = ordre des points de code** pour des séquences valides ;
   `u_compare` compare les octets, ce qui reste vrai y compris pour 🚀
   (`f09f…`) après 中 (`e4b8…`).

## 5. Validation globale (constats datés du 23/11/2026)

Commande : `bash scripts/garde_fou_mem.sh 12000000 bash <copie privée de validation_finale.sh>`,
avec `RUSTFLAGS="" CARGO_PROFILE_RELEASE_STRIP=none` (contournement
statique-pie ; la copie privée ne modifie aucun fichier du dépôt et utilise
des journaux `/tmp/opencode/unicode/vf_*` non partagés — le journal
(original) `/tmp/opencode/vf_tests.log` est mutually écrit par plusieurs
agents, d'où des résultats instables si on le relit).

| Contrôle | Résultat |
|---|---|
| `rose verif` (module seul et concaténation) | `vérifié`, rc=0 |
| C0 → ELF (4 variantes corrigées + source courante) | magic `7f454c46…`, rc=0 |
| Banc Unicode natif / ELF | 80 lignes, md5 identique, `cmp`=0 |
| Build + tests Rust | 314 + 327 + … 0 failed, rc=0 |
| Point fixe `rose rir compilateur/emetteur.rose` | `b3ed9262dd54776a6dbd07bc12368d0e` ×2 |
| AOT | 75 OK, 0 ÉCHEC |
| JIT Cranelift | 25/25 |
| WASM + RVM | 92 o ; RVM = 317811 |
| Batterie AZ | 31 CONFORME, 0 non-conforme |
| Self-host HOST-1 | fib = 317811 |
| **`validation_finale`** | **0 échec** |
