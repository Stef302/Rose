# OCTET_BRUT — deux régressions du chemin NATIF G5 (22/11/2026)

Constat daté, mesuré le 22/11/2026. Le composite **hébergé par Rust**
(`rose rose_natif.rose prog.rose > prog.elf`) fonctionnait ; les deux
régressions concernent le **binaire G5 auto-émis** (`C0`) et le chemin
`argument(1)` sans garde de la queue « vieux compilateur ».

## 1. Causes mesurées

### (1) En-tête ELF corrompu — `c2 b7` au lieu de `b7`

Le correctif Unicode a fait représenter les chaînes G5 en UTF-8. Or les
utilitaires `u8le/u16le/u32le/u64le` de `compilateur/emetteur_elf.rose`
fabriquaient des **octets bruts** avec `caractere($b)` :

| chemin | `caractere(183)` | `u16le(183)` |
|---|---|---|
| Rust (`ecrire_bytes` écrit `c as u8` par code point) | `b7` | `b7 00` |
| G5 (chaîne déjà UTF-8) | `c2 b7` | `c2 b7 00` |

Mesure avant correction (émetteur de `HEAD`, RIR identique) :

```
AVANT (G5, ancien emetteur) u16le(183) -> c2 b7 00
APRÈS (G5, emetteur corrigé) u16le(183) -> b7 00
```

L'octet surnuméraire `c2` décalait tout l'en-tête : `e_machine=0xb7c2`,
`file` disait `*unknown arch 0xffffb7c2*, corrupted program header size`
et l'exécution donnait `Exec format error` (preuve : `/tmp/opencode/c0n_fib.elf`,
12 944 o, octets `0200 c2b7 0001 0000 00c2 b000 4000 0000`).

### (2) `argument(1)` sans garde

La queue historique de `emetteur_elf.rose` contient `$sortie = argument(1)`
au niveau supérieur. Quand `C0` reçoit un seul argument (un `.rose`, pas un
`.rir`), le chemin G5 est court-circuité, la queue s'exécute et le moteur
lève `Rose : erreur — index hors des limites des arguments` (rc=1).

## 2. Correctif livré

### `compilateur/emetteur_elf.rose` (source possédée)

* `fn octet_brut($v: entier) -> chaine { retour caractere($v % 256) }`.
  Sous Rust, c'est exactement l'ancien comportement (1 code point → 1 octet).
* `u8le/u16le/u32le/u64le` appellent désormais `octet_brut` au lieu de
  `caractere` (`u8le` était inutilisé, corrigé pour cohérence).
* Globale `$g5_octetll = 0` + réservation d'**une seule** étiquette
  (`$g5_octetll = g5_next()`, aucun label interne) dans le dispatch.
* Interception **avant** la recherche de fonction utilisateur :
  `si ($nom == "octet_brut") { $lbl = $g5_octetll }`.
* `fn g5_emit_octetl($lbl)` : descripteur `[tag=1][len=1][1 octet]`,
  avec `strbio` (poids faible de l'entier désétiquetté), **sans encodage
  UTF-8**. Appelée juste après `g5_emit_car`.

### `scripts/generer_rose_natif.py` (source possédée)

* Injection de `octet_brut` → `"chaine|entier"` dans le **std_sig du
  front-end copié** (comme `eval_expr`, HOST-2). `emetteur.rose` intact.
* Injection de la garde dans la **copie du backend concaténée** :
  `$sortie = ""` puis `si (nb_arguments() > 1) { $sortie = argument(1) }`.
  `emetteur_elf.rose` lui-même n'a pas été modifié pour ce point.

### Composites régénérés (via le script)

```
compilateur/rose_natif.rose      433 565 o  md5 a44582157509863c838761c2e29c0328
compilateur/rose_natif_pe.rose   212 388 o  md5 ff3068947f224a82d704176985115b5c
```

## 3. Preuves rapides exécutées (22/11/2026)

| Contrôle | Résultat |
|---|---|
| `rose verif` sur les 2 composites + `emetteur_elf.rose` | `vérifié` (3/3) |
| Point fixe `rose rir compilateur/emetteur.rose` | `b3ed9262dd54776a6dbd07bc12368d0e` (inchangé) |
| `rose emetteur_elf.rose fib28.rir` (voie Rust) | ELF aarch64, header `0200 b700`, exécution `317811` |
| `rose rose_natif.rose fib28.rose` (composite hébergé) | ELF aarch64, header `0200 b700`, exécution `317811` |
| Sonde G5 `ecrire_bytes("/dev/stdout", octet_brut(183))` | `b7` (1 octet) ; `caractere(183)` → `c2 b7` (témoin) |
| Sonde G5 `u16le(183)` | `b7 00` (avant : `c2 b7 00`) |
| Batterie AZ (`tests_selfhost/lancer_az.sh`) | **31/31 CONFORMES**, 0 divergent, 0 bloqué (64 s) |
| Garde `argument(1)` | présente dans `rose_natif.rose` l.9490 ; `rose verif` OK |
| RIR du composite régénéré | `/tmp/opencode/octet_brut/rnat3.rir`, 677 952 o, md5 `a9ce7b57780f4a4cccd94050f59f9506` (`rose rir …` en 0,03 s) |

Piège de mesure sur la garde : `rose rose_natif.rose` (0 argument utilisateur)
échoue encore sur `argument(0)` — `nb_arguments()` ne compte PAS le nom du
programme. Ce n'est PAS le scénario de la régression (2) : `C0 rose_natif.rose`
a bien 1 argument utilisateur, `argument(0)` y est valide et c'est
`argument(1)` qui débordait. Avec 1 argument, le composite hébergé passe
(`rose rose_natif.rose fib28.rose` → rc=0, ELF valide).

Note : sous G5, `longueur(octet_brut(183))` vaut 0 (le décodeur de points
de code rejette l'octet brut `0xb7`, continuation UTF-8 invalide). Ce n'est
pas une régression : `ecrire_bytes` lit le champ **longueur en octets** du
descripteur (offset 8), qui vaut bien 1 — c'est ce champ qui pilote
l'émission de l'en-tête ELF.

## 4. Ce qui reste à mesurer par l'orchestrateur

1. Réémettre **C0** depuis le nouveau RIR :
   `rose rir rose_natif.rose > /tmp/rnat3.rir` puis
   `rose emetteur_elf.rose /tmp/rnat3.rir > /tmp/C0_test.elf` (~80 min,
   676 Ko de RIR — non lancé ici, conformément à la consigne).
2. Vérifier `file` = `ARM aarch64` et les 16 premiers octets :
   `7f45 4c46 0201 0100 … 0200 b700 0100 0000 b000 4000 0000`.
3. `C0_test.elf fib28.rose > fib.elf` → `317811`.
4. `C0_test.elf rose_natif.rose > C1` (rc=0 + magic) puis
   `C1 rose_natif.rose > C2` et `cmp C1 C2`.
5. Re-jouer la batterie AZ avec `C0_test.elf` comme émetteur figé
   (`AZ_EMETTEUR=…`) si l'orchestrateur veut la preuve complète du chemin G5.

Interdiction respectée : aucun écrit sur `/tmp/opencode/C0_new.elf` ni
`C1_nat.elf` (jobs de l'orchestrateur), aucun commit, aucune publication,
`compilateur/emetteur.rose` et `rose/src/*` intacts.
