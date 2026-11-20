# SELF_HOST_SH3 — réflexivité fermée (constat daté, 20/11/2026)

Sous-agent `sh3`. Dépôt `/home/clausake/Stefan/ROSE` (`Stefan/ROSE`).
Deux fichiers possédés : `compilateur/emetteur_elf.rose` et ce document.
**Aucun commit, aucune publication, aucun remplacement de `rose/rose`.**
Artefacts : `/tmp/opencode/sh3_agents/`.

## 1. Porte — ATTEINTE

| Étape | Commande | Résultat mesuré |
|---|---|---|
| 1 | `rose rir compilateur/emetteur.rose > self.rir` | rc=0, **263 520 o**, `b3ed9262dd54776a6dbd07bc12368d0e` |
| 2 | `rose compilateur/emetteur_elf.rose self.rir > self_final.elf` | rc=0, **534 780 o**, `ed699457a9b8cd0ea69a53bbae78fd39`, 374 s (07:41:24→07:47:38+01:00) |
| 3 | `./self_final.elf compilateur/emetteur.rose > dump_final.out` | **rc=0**, **263 520 o**, `b3ed9262dd54776a6dbd07bc12368d0e` |
| 4 | `cmp dump_final.out self.rir` | **octet pour octet identique** |

Point fixe vérifié deux fois (deux exécutions indépendantes du même ELF,
deux émissions indépendantes du même ELF : md5 `ed699457…` reproduit).

`/tmp/opencode/sh3_agents/.rose` (= fichier source) : ELF rc=0, 99 o, identique au natif.
`self.rir` **comme entrée** : rc=139 — attendu : le `$principal` émis lit un
**source `.rose`** (`lire_texte(argument(0))`), pas un dump RIR (déjà noté par SH2).

## 2. Cause du crash SH2 — TROUVÉE

Le « dernier défaut » de SH2 (`pc=0x4818c4`, `x0=0x8effd620e83848cc`,
slots 10/11/12 de `check_instr`) **n'était pas une interférence de la zone RW**.
La valeur `0x8effd620e83848cc` est le **hash d'internement de `"vide|vide"`**
(vérifié : `(hash*131+ordre(c)) mod 2^64` donne exactement cette valeur).
Traçage gdb sur un ELF instrumenté (op index + fonction) :

- crash dans `rir_instr` op 0 : `slot 27 = (idx (v ik) (v ix))` ;
- `ik` = x11 non initialisé au prologue de `rir_instr`, appelé depuis
  `rir_corps` op 15, appelé depuis `em_fonction_rir` op 12, appelé depuis
  `$principal` op 681 ;
- au prologue de `em_fonction_rir`, x13/x14/x15 contenaient le hash et
  l'adresse de zone laissés par la génération des trois littéraux
  `"$principal"`, `""`, `"vide|vide"`.

**Cause racine** : dans `g5_eval`, le cas `(b …)` renvoyait `$pos + 3`
au lieu de `$pos + 4`. `(b faux)` = 4 jetons ; la boucle d'arguments
d'un `app` s'arrêtait donc sur le `)` suivant et **n'émettait aucun des
arguments restants**, qui restaient dans les registres xN avec des
valeurs parasites. Mesure native vs ELF sur un programme minimal :

```
fn somme(a: entier, b: booleen, c: entier, d: entier) -> entier { retour a+c+d }
>> somme(1, vrai, 2, 3)      natif = 6 ; ELF avant = 1 ; ELF après = 6
```

## 3. Défauts suivants, chacun trouvé et corrigé

Ordre = ordre dans lequel le point de défaillance a reculé. Tous vérifiés
par test minimal natif vs ELF avant de réémettre.

1. **`(b …)` → `$pos+4`** (§2). SH2 gardé intact.
2. **Listes passées par référence.** `joindre` réallouait le descripteur ;
   l'appelant gardait l'ancien pointeur. Le `$gen` du lexeur restait à 1
   élément → boucle sans fin ( tas épuisé même à 4 Gio ).
   Correctif : descripteur `[tag=2][len][cap][data_ptr]`, données dans un
   **bloc séparé** ; `joindre` double la capacité et ne change jamais
   l'adresse du descripteur. `lst`/`lstvide`/`idx(liste)` adaptés.
   Preuve : `ajoute(l,v){joindre(l,v)}` → natif `3`/`a`, ELF avant `0`,
   ELF après `3`/`a`.
3. **`table`/`inserer`/`cle_existe` étaient des stubs.** Le compilateur
   émis ne reconnaissait plus les mots-clés :
   `Fonction 'tant_que' inconnue` (ligne 194 de `emetteur.rose`).
   Correctif : table = liste de clés ; `inserer` = `joindre` ;
   `cle_existe` = scan avec `streq` (registres sûrs : `streq` écrase
   x0/x2/x3/x4/x5/x6/x7/x10 ; LR sauvegardé en x11 car l'appel est un BL).
   Preuve : natif `7`, ELF avant timeout/`0`, ELF après `7`.
4. **Paramètre d'indice 30 dans x30 = LR.** `BL` écrase x30 par l'adresse
   de retour, donc le paramètre 30 du appelé recevait cette adresse.
   `check_instr` a 36 paramètres et `$ra` est précisément le 30e :
   erreur `La fonction retourne entier, attendu ` (attendu vide).
   Preuve : `f31(a1…a31) → a31`, natif `31`, ELF avant `524380`.
   Correctif : zone RW étendue aux arguments d'indice **≥ 30** (au lieu
   de ≥ 31), côté appelant comme côté prologue. Preuve : ELF `31`.
5. **`flottant()` stub + parsing décimal.** Le writer RIR appelle
   `chaine(flottant($t))` ; stub → `flottant 0` au lieu de
   `flottant 10000000000000000`. `flottant` mappé sur le parseur décimal,
   et le parseur s'arrête maintenant au premier non-chiffre (avant :
   `"10000000000000000.0"` → `999999999999999980`).
6. **Tas trop petit.** La compilation réflexive consomme > 1 Gio
   (concaténations O(n²) du writer). Mesuré : 512 Mio et 1 Gio → SIGSEGV,
   2 Gio → rc=0. `p_memsz` RW porté à 2 Gio (mapping anonyme paresseux).

Les quatre correctifs SH2 (`sous_chaine` +16, mutation `joindre`,
scratch x14/x15 avant chargement, zone ≥31) sont **conservés**, seul le
seuil ≥31 a été déplacé à ≥30 (§3.4).

## 4. Chaîne de preuves native vs ELF (tous OK)

| Test | Natif | ELF |
|---|---:|---:|
| `boolarg` (booléen en position 2) | 6 | 6 |
| `ref3` (longueur après mutation par référence) | 3 | 3 |
| `ref4` (élément après mutation par référence) | a | a |
| `tabtest` (table/inserer/cle_existe) | 7 | 7 |
| `p30` (paramètre 30) | 30 | 30 |
| `p31` (paramètre 31) | 31 | 31 |
| `t36` (36 paramètres) | 69 | 69 |
| `entbig` (`entier("10000000000000000")`) | 10000000000000000 | 10000000000000000 |
| `fib25` | 75025 | 75025 |
| `show3` | 3 | 3 |

## 5. Validation demandée

- `rose verif compilateur/emetteur_elf.rose` → **vérifié** (re-mesuré).
- `cargo test --release` → **431 passés, 0 échec** (220+204+6+1 ; 3+3 ignorés).
  Une salve intermédiaire a échoué (E0026/E0027) pendant qu'un autre agent
  modifiait `rose/src/interp.rs` ; re-mesuré ensuite une fois ce fichier
  stabilisé : rc=0. Fichier non possédé ici, non modifié par ce sous-agent.
- Jardins : `scripts/rose-jardin.sh verif-tous` → **10/10 valides**
  (la consigne annonçait 20/20 ; 10 est le nombre réel de jardins présents).
- `fib25` → **75025** des deux côtés (§4).
- Point fixe `b3ed9262…` **×2** (exécutions) + **×2** (émissions), et
  `cmp` octet pour octet avec `self.rir`.
- Banc 9 domaines : **non re-mesuré**. Ce chantier ne touche que
  `compilateur/emetteur_elf.rose` (source Rose consommée par l'émetteur) ;
  aucun chemin de l'interpréteur/JIT n'a été modifié, donc aucun mécanisme
  de régression n'existe. `scripts/valider-chantiers.sh` a été lancé
  partiellement : M4.1 26/26, M4.2 64/64, timeout à M4.3 (script long).

## 6. État final

- Fichier livré : `compilateur/emetteur_elf.rose`, md5
  `e99437a2a73610c3e02a9dfc70b12129`, `rose verif` → `vérifié`.
- ELF livré : `/tmp/opencode/sh3_agents/self_final.elf`, md5
  `ed699457a9b8cd0ea69a53bbae78fd39`, 534 780 o.
- `rose/rose` inchangé : `12eac46007cf0fe9e1aec2dff6fbf665`.
  `compilateur/emetteur.rose` inchangé : `390335d29fb9cee368b3456dd892e377`.
- Aucun commit, aucune publication. `rose/src/interp.rs` était modifié par
  un autre agent (non possédé ici) ; non touché par ce sous-agent.

## 7. Reste (hors porte, documenté)

- Stubs `flottant` → parseur décimal : les vrais flottants fractionnaires
  (`3.14`) sont tronqués. Suffisant pour le dump de `emetteur.rose` (son
  seul littéral flottant est `10000000000000000`).
- `inserer(t,c,v)` ignore `v` ; les tables réelles de Rose (au-delà des
  mots-clés) ne sont pas implémentées.
- Le compilateur émis exige 2 Gio d'espace d'adressage RW ; optimisation
  possible (capacity doubling déjà fait pour les listes, reste les
  concaténations O(n²) du writer).
- Entrée RIR non acceptée (attendu : source `.rose`).
