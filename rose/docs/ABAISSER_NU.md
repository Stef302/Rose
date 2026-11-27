# ABAISSER_NU — le compilateur RIR → IR machine NU

_STAT DATÉ : 27/11/2026, worktree `v13lower`, HEAD `326ac49`, moteur Rose
`0.1.73` (binaire `~/Stefan/ROSE/rose/rose`). Toute mesure ci-dessous est à
re-faire après changement du backend, du RIR ou de la cible._

## Doctrine — c'est un compilateur, pas une recette

`compilateur/abaisser_nu.rose` transforme le **RIR textuel** produit par
`rose rir F.rose` en une **suite d'opérations abstraites** du backend
universel (`compilateur/backend_universel.rose`). Il ne contient aucune op
machine écrite à la main : chaque op est fabriquée par `bu_operation`, et
tous les octets viennent de `bu_traduire`. Le nom de la cible est une donnée
passée à `bu_cible` ; la logique d'abaissement ne nomme aucun CPU.

Le module est une **brique universelle** : il ne sait rien du noyau, du
programme compilé, ni du format de fichier. Il fournit une suite d'ids
d'opérations ; le linker NU (`compilateur/link_nu.rose`) ou tout autre
émetteur décide ensuite quoi en faire.

L'abaissement se fait en **trois passes**, pas en une substitution:

1. **Parse** : tokenisation du dump RIR, relevé des `fonction`, `param`,
   `retour`, `n_slots`, `noms [...]`, `globaux ...`, puis de chaque ligne
   d'op (index RIR 0-based). Les noms de fonctions et de slots sont **relus**
   dans le dump (`an_find_fn`, `an_find_slot`) : aucun nom n'est codé en dur.
2. **Lower** : chaque op RIR devient 1..n descripteurs logiques
   `[code,a,b,c]`. Les sauts portent une cible **symbolique** : index d'op
   RIR (`kind=0`) ou étiquette interne (`kind=1`). Les étiquettes sont
   créées à la demande (`an_label_new` / `an_label_set`) : les branchements
   avant peuvent référencer une étiquette définie après.
3. **Expand / resolve / emit** : un `call` devient `imm reg, adresse` + `call
   reg` ; un saut est résolu en **index d'op**, puis en **décalage octet** en
   mesurant chaque op avec `bu_traduire`. Un point fixe (≤ 12 tours) promut
   les sauts hors de portée `rel8` en `imm + jmp reg` (ou `jz +7 ; imm +
   jmp reg`). Enfin les `bu_operation` définitives sont ajoutées à une
   `bu_sequence`.

L'état du module est un **journal à générations** (sentinel `-777777777` /
`"@@AN@@"`), parce que Rose interdit de rebinder une globale depuis une
fonction. Chaque appel à `an_abaisse_rir` ouvre une génération : trois
sources peuvent être abaissées dans le même processus sans interférence.

## API

```
an_abaisse_rir(rir: chaine, cible: chaine) -> liste<entier>
    Suite d'ids BU, porteuse en index 0 (convention du backend).
    `x86_32` (famille 5) est la cible PAR DÉFAUT (chaîne vide) ; toute
    cible connue de `bu_cible` est acceptée (contrôlé : `x86_16`, famille 4).

an_base() -> entier                  adresse de chargement courante (défaut 0x100000)
an_fixe_base(adresse: entier) -> entier   pose cette adresse (DONNÉE)
an_diagnostics() -> chaine           avertissements des formes non couvertes
```

## Modèle de machine (32 bits, pile explicite)

| Rôle | Registre | Usage |
|---|---|---|
| EAX (0) | accumulateur | valeur d'expression, résultat de retour |
| ECX (1), EDX (2) | temporaires | opérandes, taille d'aire d'arguments |
| EBX (3) | sauvegarde | EBP du appelé, préservé par prologue/épilogue |
| ESP (4), EBP (5) | pile, cadre | `[EBP+4+4i]` = arg i à l'entrée |

* Prologue : `store EBX,[ESP-4]`, `store EBP,[ESP-8]`, `EBX = ESP`,
  `ESP -= cadre`, `EBP = EBX`, copie des `param` dans les slots
  `[EBP-12-4*i]`, remise à zéro du gratte-papier `[EBP-12-4*n_slots … +8]`.
* Épilogue : `ESP = EBP`, restauration `EBP`/`EBX`, `ret`. Une fonction qui
  a déjà un `retour_abstrait` ne reçoit pas de seconde épilogue.
* Appel : réservation de `4*n` octets sous ESP, chaque argument évalué puis
  `store` à `[ESP+4*i]`, `imm EDX, adresse`, `call EDX`, désallocation.
  L'adresse de la fonction cible = `an_base() +` décalage de sa première op.
* `afficher_abstrait` : expression en EAX puis `syscall(128)` — hook
  d'affichage NU (`int 0x80` sur la famille 5). Le contenu du hook est la
  responsabilité du runtime d'exécution, pas de l'abaiseur.

## Formes RIR couvertes

| Forme RIR | Traitement |
|---|---|
| `fonction NAME`, `param x entier`, `retour entier`, `n_slots N`, `noms [i]x`, `globaux i` | métadonnées relevées ; noms relus |
| `affect slot i = EXPR` | `EXPR` en EAX puis `store` dans le slot |
| `afficher` / `afficher_abstrait EXPR…` | `EXPR` en EAX puis `syscall` |
| `expr EXPR` | évaluée, résultat ignoré |
| `saut N` | `jmp_rel8`, ou saut long indirect après point fixe ; `N` = index d'op RIR |
| `saut_si_faux_abstrait N COND` | comparaison + `jz_rel8` vers l'index `N` |
| `retour_abstrait EXPR?` | `EXPR` en EAX puis épilogue + `ret` |
| `slot i`, `entier N`, `(n N)`, `(b vraie|fause)` | `load` de slot / `imm` |
| `(v nom)` | résolu en slot par son **nom relu** ; sinon `imm 0` + diagnostic |
| `(bin + - *)` | arithmétique 32 bits modulo 2³² (les 3 opérandes sont gérés selon la sémantique réelle de l'encodeur BU) |
| `(bin < <= > >= == !=)` | booléen 0/1 matérialisé par branchements ; test du signe par **load décalé** de l'octet de poids fort + `test_al`, égalité par OU des 4 octets + `test_al` |
| `(bin and/or/xor)` | opérations logiques sur registres |
| `(app f args…)` | appel avec passage d'arguments sur la pile ; `f` relu dans le RIR |
| `(neg …)`, `(non …)`, `(uni - …)` | négation arithmétique / booléenne |

Opérations BU produites (compteurs mesurés, voir la batterie) :

| Exemple | ops / octets | Détail |
|---|---|---|
| (a) `carre(5)` | 45 / 206 | `imm 9, mov 7, add 1, sub 3, mul 1, load 7, store 13, call 1, ret 2, syscall 1` |
| (b) `somme(5)` | 82 / 376 | `nop 1, imm 15, mov 9, add 3, sub 4, and 1, or 3, load 17, store 20, jmp 1, call 1, ret 2, jz_rel8 2, test_al 2, syscall 1` |
| (c) `absolu(-7)` | 62 / 279 | `imm 12, mov 9, add 1, sub 5, and 1, load 12, store 15, call 1, ret 3, jz_rel8 1, test_al 1, syscall 1` |

Le `jmp` (indirect) de (b) est un saut arrière devenu trop loin pour
`jmp_rel8` : le point fixe l'a promu en `imm + jmp reg` (adresse absolue).

## Limites — exactement, pas approximativement

1. **Comparaisons et débordement signé.** Le jeu d'ops BU n'a ni `cmp`, ni
   `setcc`, ni décalage, ni chargement d'octet. Un `load` décalé donne
   l'octet de poids fort ; `test_al 128` teste son bit de signe. Si
   `lhs - rhs` déborde le signed 32 bits, le bit de signe est faux. Les
   comparaisons sont donc exactes quand la différence reste dans
   `[-2³¹, 2³¹-1]` (cas des trois exemples). Ce n'est pas une approximation
   silencieuse : la contrainte est documentée ici et la correction
   nécessiterait une op `cmp`/`setcc` dans le backend, c'est-à-dire un
   changement de contrat de l'IR abstrait.
2. **Portée des sauts.** `jmp_rel8`/`jz_rel8` codent un déplacement sur 8 bits
   signé. Le point fixe promut automatiquement tout saut hors `[-128,127]` en
   forme indirecte absolue. Le `jz` long coûte 9 octets (`jz +7 ; imm + jmp`).
3. **Adresse absolue.** Les `call` et sauts longs supposent un binaire NU
   chargé à `an_base()` (défaut `0x100000`). Pour un autre contrat de charge,
   appeler `an_fixe_base` avant l'abaissement. Ce n'est pas un défaut : c'est
   la même donnée que `ln_config` du linker.
4. **Types non entiers.** `(f …)`, `(s …)`, les globales non résolues et les
   `bin` non couverts (`/`, `%`, `^`, …) sont abaissés en `imm 0` et
   signalés dans `an_diagnostics()`. Le contrat machine est entier 32 bits.
5. **Primitives RIR.** `call_prim` de l'interpréteur (`longueur`, `table`,
   E/S de haut niveau…) n'a pas d'équivalent dans l'IR NU ; seul le hook
   `syscall` est émis. Les programmes RIR qui en dépendent ne sont pas
   compilables par cette brique en l'état.
6. **Pas de compilation de quais.** `surcharge` est traité comme un `app`
   sur le nom de base ; la résolution de surcharge par types n'est pas
   reproduite (elle exigerait un mécanisme de table de dispatch dans l'IR).

## Preuves (commandes réellement exécutées)

```
cat compilateur/backend_universel.rose compilateur/abaisser_nu.rose \
    briques/tests_abaisser.rose > /tmp/opencode/abaisser/tests.rose
rose verif /tmp/opencode/abaisser/tests.rose          # vérifié
ROSE_SANS_JIT=1 rose interprete …                     # 15/15, TOUT VERT
C0 /tmp/opencode/abaisser/tests.rose > tests.elf
./tests.elf                                           # 15/15, TOUT VERT
diff <(ELF) <(interpréteur)                           # SORTIES IDENTIQUES
```

Global (non régressé par la brique) : parité self-host
`tests=141 CONFORMES=141 DIVERGENTS=0 BLOQUES=0`, ERREURS=0 ; point fixe
`rose rir compilateur/emetteur.rose` **et**
`rose compilateur/emetteur.rose compilateur/emetteur.rose` → md5
`b3ed9262dd54776a6dbd07bc12368d0e` ×2, `cmp = 0`, stderr vide.

## Fichiers

* `compilateur/abaisser_nu.rose` — créé (module compilateur).
* `briques/tests_abaisser.rose` — créé (batterie 15 assertions).
* `rose/docs/ABAISSER_NU.md` — créé (ce document).
* `compilateur/backend_universel.rose` — aligné sur la version du chantier
  parent pour disposer des ops `in_port(17)`, `out_port(18)`, `jz_rel8(19)`,
  `test_al(20)` dans `bu_noms_ops` et `bu_encoder_table`. Sans cet alignement,
  `jz_rel8`/`test_al` seraient inconnus de `bu_code_op` et l'abaissement des
  conditions serait impossible. Aucune autre modification du backend.
