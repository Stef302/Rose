# BRIQUE_PROFIL — B5 : entiers bornés et latence prévisible, en Rose pur

**État livré :** une brique Rose pure, un manifeste `.bud`, un pilote, ce
document. Worktree exclusif `/tmp/opencode/v3borne`, HEAD de départ
`1950fe70fdd1bac24aa4fd77a7cd654cd5da1fb6`. **Aucun commit, aucun push,
aucun fichier préexistant modifié.**

---

## 1. Périmètre et doctrine

`briques/README.md` et `rose/docs/BRIQUES_UNIVERSELLES.md` fixent la règle :
**une brique décrit un mécanisme, jamais un appareil**, et le profil noyau
(B5) est le préalable à B2 → B6 → B1 → B4 → B3 car `in/out` ou MMIO sur un
entier de précision arbitraire n'a pas de largeur de registre.

B5 **offre le choix d'une borne, elle ne l'impose pas**. Il n'y a ni
`sous-langage noyau` figé dans le compilateur, ni type `u8`/`i32` ajouté au
front-end, ni nom d'OS, de CPU, de registre, de bus ou d'appel système dans
le code. La largeur, la signedness, la politique de débordement et le budget
sont des **données** déclarées dans un `.bud`, puis construites par
`borne_declarer_avec`. Une autre cible = un autre `.bud`, zéro ligne de Rose
modifiée (preuve §5.1).

| Fichier créé | Rôle |
|---|---|
| `briques/profil/noyau.rose` | B5 — profil borné, 4 opérations + conversion, politique explicite, budget symbolique |
| `briques/profil/noyau.bud` | manifeste de données : bits, débordement, latence_max |
| `briques/profil/tests_noyau.rose` | pilote : 75 lignes de sortie, identiques interpréteur et ELF |
| `rose/docs/BRIQUE_PROFIL.md` | ce document |

Rose n'a pas de `importer` : la brique se **concatène** avec le programme,
comme `compilateur/brique_runtime.rose` et les autres briques du dossier :

```bash
cat briques/profil/noyau.rose mon_noyau.rose > /tmp/prog.rose
/tmp/opencode/C0_h.elf /tmp/prog.rose > /tmp/prog.elf   # ELF64 aarch64
```

---

## 2. Le profil est une donnée `liste<entier>`

La convention des autres briques (B3/B4) est reprise : une lecture de `table`
est typée `?` en G5 et `inserer` crée des doublons, donc tout l'état est une
`liste<entier>`. Chaque fonction rend un état neuf que l'appelant reprend
explicitement ; aucune mutation cachée, aucun GC, aucun `gc_allouer`.

| Index | Champ | Sens |
|---|---|---|
| 0 | `bits` | largeur logique choisie (1..64 ; 8/16/32/64 courants) |
| 1 | `signe` | 0 = non signé `[0, 2^bits-1]`, 1 = signé `[-2^(bits-1), 2^(bits-1)-1]` |
| 2 | `politique` | 0 = `refuser`, 1 = `saturer`, 2 = `enrouler` |
| 3 | `min` | borne basse |
| 4 | `max` | borne haute |
| 5 | `module` | `max - min + 1`, jamais nul (même profil invalide) |
| 6 | `statut` | 0 = valide, -1 = largeur hors 1..64 |
| 7 | `latence_max` | plafond symbolique, 0 = non bornée |
| 8 | coût 0 | déclaration |
| 9 | coût 1 | addition |
| 10 | coût 2 | soustraction |
| 11 | coût 3 | multiplication |
| 12 | coût 4 | division |
| 13 | coût 5 | conversion |
| 14 | coût 6 | lecture |

Politique de débordement, **toujours signalée** par `deborde = 1` :

| Code | Nom | `valeur` rendue quand `deborde = 1` |
|---|---|---|
| 0 | `refuser` | `0` — valeur non utilisable, l'appelant doit tester `deborde` |
| 1 | `saturer` | `min` si dépassement par le bas, `max` sinon |
| 2 | `enrouler` | retour modulo dans `[min, max]` (complément à deux si signé) |

---

## 3. API

### 3.1 Déclaration (le choix vient des données)

```rose
fn borne_declarer(bits: entier) -> liste<entier>                       # non signé, refuser
fn borne_declarer_signe(bits: entier) -> liste<entier>                 # signé, refuser
fn borne_declarer_avec(bits: entier, signe: entier, politique: entier) -> liste<entier>
fn borne_fixer_couts(p: liste<entier>, couts: liste<entier>) -> liste<entier>
fn borne_latence_fixer(p: liste<entier>, max_cycles: entier) -> liste<entier>
```

`borne_declarer_avec` est le point d'entrée du `.bud`. Une largeur hors
1..64 produit un profil **dégradé mais sûr** (`bits` replié à 8, `statut=-1`,
module 256) : toutes les opérations refusent (`deborde=1`), jamais de calcul
silencieux ni de division par zéro.

### 3.2 Opérations — toutes rendent `[valeur, deborde]`

```rose
fn borne_add(a: entier, b: entier, p: liste<entier>) -> liste<entier>
fn borne_sub(a: entier, b: entier, p: liste<entier>) -> liste<entier>
fn borne_mul(a: entier, b: entier, p: liste<entier>) -> liste<entier>
fn borne_div(a: entier, b: entier, p: liste<entier>) -> liste<entier>
fn borne_convertir(v: entier, p: liste<entier>) -> liste<entier>
```

- `borne_add/sub/mul` calculent le résultat exact, comparent à `[min, max]`,
  puis appliquent la politique. Aucun appel à `bigint_rose.rose`.
- `borne_div` est une **division tronquée vers zéro** calculée sur des
  grandeurs positives puis signée explicitement. `b = 0` rend `[0, 1]`.
  Le cas signé `min / -1` déborde et est traité comme toute autre sortie
  de borne. Cette écriture est volontaire : l'interpréteur Rose arrondit
  `÷` vers le bas sur un dividende négatif alors que le backend C0 tronque
  vers zéro ; la décomposition garantit la parité des deux backends.
- `borne_convertir` est le **seul** point d'entrée d'un entier Rose
  arbitraire. Il vérifie l'appartenance à la borne avant de l'y ramener.

### 3.3 Lecteurs

```rose
borne_bits(p)  borne_signe(p)  borne_politique(p)  borne_min(p)  borne_max(p)
borne_module(p)  borne_statut(p)  borne_latence_max(p)  borne_cout(p, code)
borne_dans(v, p)                       # 1 si v est dans [min, max], 0 sinon
resultat_valeur(r)  resultat_deborde(r) # lecture de [valeur, deborde]
```

### 3.4 Latence — comptage symbolique, pas mesure

```rose
fn latence_budget(p: liste<entier>, ops: liste<entier>) -> entier
fn latence_verifier(p: liste<entier>, ops: liste<entier>) -> liste<entier>
  # rend [cycles, nombre_operations, respecte_le_budget]
```

Codes d'opération : `0` déclaration, `1` add, `2` sous, `3` mul, `4` div,
`5` conversion, `6` lecture. Un code inconnu coûte 1 unité.

Modèle par défaut, avec `largeur = ceil(bits / 8)` :

| Code | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---|---|---|---|---|---|---|
| Coût | `2+l` | `1+l` | `1+l` | `2+2l` | `3+3l` | `2+l` | `1` |

Exemples : `l=1` (8 bits) → `[3,2,2,4,6,3,1]` ; `l=4` (32 bits) →
`[6,5,5,10,15,6,1]`. Ces nombres sont des **unités comptables** : aucune
mesure de cycle, aucun chronomètre, aucune hypothèse de fréquence. Le `.bud`
peut les remplacer par `borne_fixer_couts`.

`latence_verifier` compare le total à `p[7]` ; `latence_max = 0` signifie
« non bornée » et rend toujours `respecte = 1`.

---

## 4. Comment utiliser la brique depuis un `.bud`

Le `.bud` ne contient aucune logique : il **déclare** les valeurs. Le noyau
les traduit en appels Rose (aujourd'hui à la main, demain par le compilateur
quand il lira le manifeste avant l'analyse lexicale).

Extrait de `briques/profil/noyau.bud` :

```
profil_entiers: {
  bits: 32
  signe: non_signe
  debordement: refuser
  largeurs_autorisees: [8, 16, 32, 64]
  codes_debordement: { refuser: 0, saturer: 1, enrouler: 2 }
  conversion_bigint: expliciteUniquement
}
latence: {
  modele: "symbolique_octet_v0"
  mesuree: faux
  couts: [6, 5, 5, 10, 15, 6, 1]
  latence_max: 0
}
```

Assemblage correspondant, sans modifier `noyau.rose` :

```rose
$p = borne_declarer_avec(32, 0, 0)        # bits, signe, refuser
$p = borne_fixer_couts($p, [6,5,5,10,15,6,1])
$p = borne_latence_fixer($p, 100)
$r = borne_add(4294967295, 1, $p)
si (resultat_deborde($r) == 1) { ... }   # le noyau décide de sa réaction
```

Changer `bits: 32` en `bits: 8`, `signe: signe` ou `debordement: enrouler`
dans le manifeste suffit : la brique n'a aucun `si architecture == …` et
aucune constante de largeur en dur. Une démonstration est donnée en §5.1
avec un profil 16 bits signé « saturer ».

---

## 5. Validation — preuves de sortie réelles

Commandes exécutées depuis `/tmp/opencode/v3borne` le 25/11/2026,
interpréteur = `/home/clausake/Stefan/ROSE/rose/rose` (0.1.73),
compilateur C0 = `/tmp/opencode/C0_h.elf` :

```bash
rose verif briques/profil/noyau.rose                      # vérifié, rc=0
cat briques/profil/noyau.rose briques/profil/tests_noyau.rose > /tmp/pilote.rose
rose verif /tmp/pilote.rose                               # vérifié, rc=0
/tmp/opencode/C0_h.elf briques/profil/noyau.rose > /tmp/seul.elf
/tmp/opencode/C0_h.elf /tmp/pilote.rose          > /tmp/pilote.elf
rose /tmp/pilote.rose > /tmp/interp.txt && /tmp/pilote.elf > /tmp/elf.txt
cmp /tmp/interp.txt /tmp/elf.txt
```

| Contrôle | Résultat mesuré |
|---|---|
| `rose verif` brique seule | `vérifié`, rc=0 |
| `rose verif` pilote concaténé | `vérifié`, rc=0 |
| C0 brique seule | rc=0, ELF 26 852 o, `7f 45 4c 46`, `e_machine = 183` (aarch64) |
| C0 pilote | rc=0, ELF 32 120 o, `7f 45 4c 46`, `e_machine = 183` |
| Exécution ELF | rc=0, 75 lignes, stderr vide |
| Interpréteur | rc=0, 75 lignes, stderr vide |
| Parité | `cmp` muet, md5 commun `1035b8a03b9ea01dd5907fd28bde6504` |

Points vérifiés dans les 75 lignes : `255 + 1` déborde en borne 8 (politique
`refuser` → `[0,1]`, `saturer` → `[255,1]`, `enrouler` → `[0,1]`), borne 32
(`2^32 - 1 + 1` déborde), conversion depuis un entier arbitraire (valide,
trop grand, négatif), borne signée (enroulement `-128..127`), division par
zéro, division négative stable, profil invalide, et budget symbolique
(`16`/`37` unités, dépassement détecté, coûts remplacés par le `.bud`).

### 5.1 Réversibilité

Un second pilote, sans aucune modification de `noyau.rose`, a construit
`borne_declarer_avec(16, 1, 1)` (16 bits, signé, `saturer`) puis fixé un
budget à 100 : sorties interpréteur = ELF (12 lignes, `cmp` muet), avec
`min=-32768`, `max=32767`, `32767+1 → 32767/1`, `-32768-1 → -32768/1`,
`convertir(99999) → 32767/1`, budget `21` respecté. Le code source de la
brique est identique (md5 `f94bde06b666f593c74dcda7dff94133`) dans les deux
assemblages.

---

## 6. Limites

1. **Le compilateur ne lit pas encore les `.bud`.** La convention est en
   attente de câblage (`briques/README.md` §4). Aujourd'hui le noyau passe
   lui-même `bits/signe/politique/couts` aux fonctions de déclaration ; le
   `.bud` est la spécification de ce passage, pas encore une entrée de
   `rose verif`.
2. **Le bigint de l'hôte reste utilisé pour le calcul intermédiaire.**
   Rose n'a pas de types natifs de largeur fixe : `a + b` est évalué en
   précision arbitraire avant comparaison à la borne. Ce qu'exige B5 est
   respecté — aucune valeur rendue ne dépasse la borne, aucun appel à
   `bigint_rose.rose`, aucune dilatation silencieuse de précision — mais
   l'élimination physique du bigint appartient à B1/B2 (types machine
   déclarés par le `.bud`) et à l'émetteur, pas à cette brique.
3. **Le budget est symbolique, jamais garanti.** `latence_budget` compte
   des unités ; il ne mesure ni cycles, ni temps, ni cache, ni variations de
   microarchitecture. Une latence réelle doit être mesurée sur la cible par
   B1/B6 et déclarée dans le `.bud` sous forme de chiffres mesurés, comme
   l'impose la règle 5 de `briques/README.md`.
4. **Division et modulo** : troncature vers zéro et modulo normalisé ont été
   choisis pour la parité interpréteur/C0, pas pour une sémantique
   arithmétique unique. D'autres backends (AOT LLVM, WASM) n'ont pas été
   validés ici.
5. **Plafond du backend C0 (mesuré).** L'émetteur C0 calcule et affiche des
   entiers machine `i64` : `2 ^ 63` et `2 ^ 64` y débordent. Un profil 64 bits
   est correct à l'interpréteur (min `-2^63`, max `2^64-1`) mais ses bornes
   deviennent `0`/`-1` en ELF C0, et l'affichage de `2^64-1` y rend `-1`.
   Sur ce backend, utiliser une largeur **≤ 62** (8/16/32 validées) ou un
   backend à entiers plus larges. Ce n'est pas un défaut de la brique : c'est
   la largeur du backend, que le `.bud` doit déclarer.
6. **Largeurs 1..64 uniquement**, entiers seuls : ni rationnels, ni
   flottants, ni virgule fixe, ni SIMD, ni types vectoriels. Un `bits`
   invalide est refusé (`statut=-1`) et replié sur 8 bits de sécurité.
7. **Listes = valeurs explicites.** `joindre` reconstruit une liste neuve
   dans le moteur hôte : ce n'est pas un allocateur noyau, et la brique
   n'appelle ni GC ni allocation implicite dans sa sémantique. Le pilote
   n'imprime que des entiers : le backend C0 ne sait pas afficher une liste
   complète (`>> $p` y rend un entier) et diffère sur `÷` négatif — d'où
   `resultat_valeur`/`resultat_deborde` et la division décomposée.
8. **Hors périmètre** : registres, MMIO, interruptions, pagination,
   ordonnancement, pilotes, formats d'image. Ce sont B2, B3, B4, B7, B9 —
   aucune de ces briques ne doit être introduite ici.

---

## 7. État des lieux factuel

Fait : 4 fichiers créés, `rose verif` vert, compilation C0 verte, ELF64
aarch64 produit, 75 lignes de sortie identiques entre interpréteur et ELF,
réversibilité démontrée sur un second profil, aucun fichier existant modifié,
aucun commit ni push. Non fait : câblage de la lecture des `.bud` dans le
compilateur, validation AOT/WASM, mesure de latence réelle sur matériel.
