# HOST-2 — `eval_expr` : lever le double blocage du self-hosting

Date : 2026-11-21. Périmètre : `compilateur/emetteur_elf.rose`,
`scripts/generer_rose_natif.py`, `rose_natif.rose` (régénéré).
`compilateur/emetteur.rose` (point fixe `b3ed9262…`) et `rose/src/*` INTACTS.

## 1. Diagnostic exact (reproduit avant correction)

| Commande | Résultat mesuré |
|---|---|
| `rose compilateur/rose_natif.rose compilateur/emetteur_elf.rose` | rc=1, `Rose : erreur — index hors des limites des arguments` (`$sortie = argument(1)` du vieux chemin, atteint parce que `em_source_rir()` rend une chaîne d'erreur au lieu d'un dump `rir`) |
| `rose compilateur/emetteur.rose compilateur/emetteur_elf.rose` | rc=0, stdout = `Rose : erreur — [Ligne 4441, Colonne 0] Fonction 'eval_expr' inconnue` |
| `python3 scripts/generer_rose_natif.py` | `AssertionError` : `be[3653]` n'est plus `}` (repères figés périmés par le correctif UTF-8) |

### Cause

`eval_expr` est une primitive **native** du moteur Rust
(`rose/src/front.rs:758` : `std_sig`, `rose/src/interp.rs:5717` : contrat),
appelée 25 fois dans `emetteur_elf.rose`. Elle est absente :

1. de la table `std_sig` du front-end écrit en Rose (`emetteur.rose:1752-1832`)
   → le front-end refuse d'analyser la source du backend ;
2. du runtime G5 émis → un binaire natif issu de cette source ne peut pas
   résoudre les appels (`g5_func_id` → `appel non résolu`).

Corollaire : `em_source_rir()` échoue, le dispatch G5 est sauté, et le vieux
compilateur (atteignable seulement pour une source `.rose`) tombe sur
`argument(1)` non gardé.

Troisième blocage, indépendant : le générateur utilisait des numéros de ligne
figés (`head_end=3654`, `dispatch=3658`, `quit=3811`) alors que le fichier a
grandi (correctif UTF-8). Les repères réels sont `3894 / 3898 / 4063`.

## 2. Correctif

### 2.1 `scripts/generer_rose_natif.py`

- Repères du backend trouvés **par contenu** (`si (nb_arguments() >= 1) {`,
  `quitter(0)`, dernière `}` de niveau 0) au lieu de numéros de ligne : la
  génération redevient déterministe et surviv aux ajouts dans le backend.
- Injection de `si ($nom == "eval_expr") { retour "chaine|chaine" }` dans la
  **copie du front-end** (`em_head`), avant concaténation. `emetteur.rose`
  n'est pas modifié : le point fixe `b3ed9262dd54776a6dbd07bc12368d0e` reste
  inchangé.

### 2.2 `compilateur/emetteur_elf.rose`

`fn eval_expr($expr: chaine) -> chaine` — évaluateur en Rose pur, exclusivement
dans le sous-ensemble émettable G5, avec des helpers `ee_*` :

- `ee_trim`, `ee_trouve` : analyse de la chaîne ;
- séparation du suffixe `// vars:` (et variantes `//vars:`, `; vars:`,
  `;vars:`, `vars:`), coupure **avant** le `//` ;
- `ee_tokens` : lexeur (entiers/rationnels/flottants, chaînes guillemetées,
  mots, opérateurs dont `== != <= >=`, parenthèses) ;
- `ee_var` : contexte `nom=valeur`, **dernière définition fait foi** ; les
  paires malformées (`a==`) sont ignorées, comme le fait le repli natif ;
- `ee_num` / `ee_norm_num` : conversions exactes (`entier`, `fraction`,
  `flottant`) et formatage décimal des flottants ;
- `ee_prec` / `ee_apply` / `ee_atom` / `ee_parse` : analyseur à précédence,
  `+ - * / % ^`, comparaisons, `et/ou/non`, `vrai/faux`, unaires,
  parenthèses, concaténation de chaînes.

Toutes les variables sont **locales** : aucune globale G5 ajoutée (table
toujours sous la borne 512).

Contrat reproduit : 31/31 sur la sonde `/tmp/ee_probe2.rose` comparée à la
primitive native (entiers exacts > 2^64, rationnels `-3/7`, `2^64`,
`(2^64)^2`, substitutions avec contexte, Unicode, littéraux guillemetés).
Sur les 227 expressions réellement collectées par instrumentation du vieux
chemin (batterie AZ + fib), 224/227 identiques ; les 3 divergences restantes
sont des expressions **malformées** produites par le préprocesseur
(`$a + 1 }`, `= 100…){ >> … }`), sans effet sur le chemin G5.

### 2.3 Régénération

`python3 scripts/generer_rose_natif.py` → `rose_natif.rose` (431 321 o).
`rose verif compilateur/rose_natif.rose` → **`vérifié`**.

## 3. Preuves

- Point fixe : `rose rir emetteur.rose` et `rose emetteur.rose emetteur.rose`
  donnent tous deux md5 `b3ed9262dd54776a6dbd07bc12368d0e`.
- Batterie AZ (émetteur snapshots : `emetteur_elf.rose` modifié) :
  **31/31 CONFORMES**, 0 divergent, 0 bloqué ; comparaisons natif/ELF,
  RIR×2, ELF×2, sortie×2 toutes 31/31 (63 s).
- Émission courante : `rose rose_natif.rose fib28.rose` → ELF 9 284 o, magic
  `7f454c46…`, sortie `317811`.
- Chaîne C1/C2 : voir §4 (résultat mesuré en fin de chantier).

## 4. Chaîne C1/C2 (HOST-2) — état mesuré le 22/11/2026

**Tentative 1 (interpréteur Rust) — timeout.** `rose rose_natif.rose rose_natif.rose`
a tourné 7 200 s sous `timeout` : `rc=124`, sortie 0 octet, aucun magic. La
compilation de la source du composite (431 Ko) par le front-end interprété est
trop lente (boucles de chaînes O(n²) déjà documentées par ailleurs). Ce n'est pas
un échec fonctionnel de `eval_expr`, dont la sonde est à 224/227 sur les
expressions réelles et la batterie AZ à 31/31.

**Tentative 2 (compilateur natif C0) — en cours.** On émet d'abord le compilateur
en code machine à partir du RIR du composite :

```
rose rir rose_natif.rose > rnat2.rir                 # 676 543 o
rose emetteur_elf.rose rnat2.rir > C0_new.elf        # ~70 min mesurées
C0_new.elf rose_natif.rose > C1_nat.elf              # natif, rapide
C1_nat.elf rose_natif.rose > C2_nat.elf
cmp C1_nat.elf C2_nat.elf                            # point fixe étendu
```

Journal : `/tmp/opencode/point_fixe_C2.log`. Le résultat n'est PAS revendiqué
avant lecture du `cmp`.

### Résultat mesuré de la tentative 2 (22/11, 02:00 UTC) — ÉCHEC

- `C0_new.elf` a bien été émis : 1 573 232 o, magic `7f454c4602010100`.
- MAIS `C0_new.elf fib.rose` produit un ELF **corrompu** (12 944 o) :
  `file` → `*unknown arch 0xffffb7c2*, corrupted program header size`,
  exécution → `Exec format error`. Les 16 premiers octets montrent un `c2`
  inséré avant `b7` : `0200 c2b7 0001 0000 00c2 b000…` au lieu de
  `0200 b700 0100 0000 b000 4000…`. Cause : `caractere(183)` encode désormais
  en UTF-8 (`c2 b7`) alors que `u16le/u32le/u64le` l'utilisent pour écrire des
  octets bruts.
- `C0_new.elf rose_natif.rose > C1` → `rc=1`, `index hors des limites des
  arguments` : le `$sortie = argument(1)` non gardé du composite.

Ces deux régressions sont propres au **chemin natif G5** ; le composite hébergé
par Rust reste valide (fib → ELF 9 284 o → 317811, AZ 31/31). Correctif en cours
dans `emetteur_elf.rose` + générateur (primitive `octet_brut`, garde `argument(1)`).
Aucun point fixe C1/C2 n'est revendiqué à ce stade.
