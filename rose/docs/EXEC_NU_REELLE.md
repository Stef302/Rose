# Exécution machine RÉELLE d'un vrai programme Rose compilé en binaire nu

*Constat daté : 27/11/2026. Worktree exclusif `/tmp/opencode/v13run`, HEAD
`326ac49`. Aucun commit, aucun push. Garde-fou `scripts/garde_fou_mem.sh
12000000 <cmd>` sur toutes les commandes longues.*

## 1. Ce qui est PROUVÉ

Trois programmes écrits en **Rose** (avec `fn`, `tant_que`, `si`, variables et
slots) sont :

1. compilés en RIR par le front-end Rose auto-hébergé (`lire_programme_reg`,
   `compilateur/lecteur_source.rose`) — **depuis les chaînes sources contenues
   dans le test** ;
2. abaissés en opérations machine abstraites par `compilateur/comp_nu_demo.rose`
   qui appelle **`bu_operation`** du backend universel, puis traduits par
   **`bu_traduire(seq, bu_cible("x86_32"))`** ;
3. écrits sous forme de **binaire nu** (ni ELF, ni PE, ni en-tête) par
   **`compilateur/link_nu.rose`** (`ln_config` / `ln_ajouter_section` /
   `ln_ecrire`), à l'adresse de charge fixe `0x08048100` ;
4. **exécutés réellement** par `qemu-i386-static` (enveloppe ELF32 minimale de
   256 octets, usage uniquement comme contrat de chargement), puis comparés à
   l'exécution interprétée des mêmes sources par le moteur Rose.

Résultat : **3/3**. Les valeurs natives sont identiques aux valeurs
interprétées.

| Programme Rose | Valeur interprétée | Binaire nu | Exécution QEMU (`exit`) | Octets lus en mémoire |
|---|---:|---:|---:|---|
| `fn carre($x: entier) -> entier { retour $x * $x }` ; `afficher(carre(5))` | 25 | 121 o | **25** | `19 00 00 00` |
| `$s=0` ; `$i=1` ; `tant_que ($i <= 5) { $s = $s+$i ; $i = $i+1 }` ; `afficher($s)` | 15 | 172 o | **15** | `0f 00 00 00` |
| `si (3 > 2) { afficher(1) } sinon { afficher(0) }` | 1 | 141 o | **1** | `01 00 00 00` |

Empreintes des binaires nus livrés : `fae112ff3182ca0185055b3ecd045407`
(`p1_carre.bin`), `78bb4b62e11ef04ea89b9f4fc3d5eb44` (`p2_somme.bin`),
`226be65108368995827313e843ff709a` (`p3_si.bin`).

`19` = 25, `0f` = 15, `01` = 1 en little-endian 32 bits. Les quatre octets
sont **écrits puis relus en mémoire** par le code machine lui-même : le
`write` (syscall 4) envoie sur stdout le contenu de la case mémoire, et le
code de retour est rechargé depuis cette même case (`mov ebx,[edi]`), donc
l'égalité des deux est une preuve d'exécution, pas une coïncidence de
désassemblage.

Adresses convenues : charge `0x08048100` (`ln_charge`), entrée =
`0x08048132` pour `carre` et `0x08048100` pour les deux autres
(`ln_entree`, fonction `$principal`), case résultat `0x08049000`.

## 2. Ce qui est VRAIMENT compilé depuis Rose

Le RIR produit par le front-end est, par exemple pour `carre` :

```
fonction carre
  param x entier
  retour entier
  n_slots 1
  noms [0]x
  ops
    0000 retour_abstrait (bin * slot 0 slot 0)
fonction $principal
  retour vide
  n_slots 0
  noms
  ops
    0000 expr (app afficher (app carre (n 5)))
```

L'abaisseur le parcourt et émet, **via `bu_operation`**, les opérations
suivantes (toutes traduites par `bu_traduire`, aucun octet écrit à la main
pour elles) :

| Op RIR reconnue | Appels `bu_operation` | Sens machine |
|---|---|---|
| `affect slot k = …` | `imm`, `store` | `mov [ebp-4(k+1)], eax` |
| `bin +` / `bin -` / `bin *` | `mov`, `add` / `sub` / `mul` | `eax = a op b` (ordre respecté) |
| `(v nom)` / `(slot k)` | `load` | `mov eax,[ebp-4(k+1)]` |
| `(app f …)` | `imm`, `call` | `mov edi, @f` ; `call edi` |
| `retour_abstrait …` | `mov`, `ret` (via `nu_jmp`) | épilogue + `ret` |
| `afficher_abstrait` / `expr (app afficher …)` | `imm`, `store`, `mov`, `load`, `syscall` | écriture mémoire + `write` + `exit` |
| prologue | `mov`, `sub`, `store` | `push ebp` (octet brut), `mov ebp,esp`, réservation des slots, copie des arguments EAX/ECX/EDX |
| épilogue | `mov`, `ret` | `mov esp,ebp`, `pop ebp` (octet brut), `ret` |

**Exception honnête.** Le backend universel n'expose pas encore d'opération
abstraite de **branchement conditionnel** (son jeu est `nop/imm/mov/add/sub/
mul/and/or/xor/load/store/jmp/call/ret/syscall/hlt/jmp_rel8`). Les deux ops
`saut` et `saut_si_faux_abstrait` sont donc abaissées par `nu_saut`, un
encodeur local qui émet `E9 rel32` et `0F 8x rel32` (`jg`, `jle`, …) avec une
table de labels et un patch final. C'est **la seule partie du binaire qui ne
passe pas par `bu_operation`** ; elle est documentée ici et dans le source.
Les conditions `<=` et `>` du programme 2 et 3 sont de vrais `cmp` + sauts
conditionnels x86, pas du repli à la compilation.

Extrait du désassemblage réel (capstone 5.0.7, mode 32 bits) du binaire de la
somme :

```
0x08048133: 39c8              cmp    eax, ecx
0x08048135: 0f8f3c000000      jg     0x8048177     ; si i > 5 -> afficher
0x08048172: e9abffffff        jmp    0x8048122     ; retour au test
```

Et du `si` :

```
0x0804811c: 39c8              cmp    eax, ecx
0x0804811e: 0f8e35000000      jle    0x8048159     ; condition fausse -> sinon
0x08048154: e930000000        jmp    0x8048189     ; saut 4 -> épilogue
```

Le fichier `nu_exec/desasm.txt` contient les trois désassemblages complets.

## 3. Reproductible

```bash
cd /tmp/opencode/v13run
bash scripts/verifier_exec_nu.sh
```

La script concatène le backend, le linker, le lecteur de source, l'abaisseur
et le test, fait `rose verif`, compile chaque programme dans un processus
Rose distinct (les journaux globaux sont ainsi isolés), construit l'enveloppe
ELF avec `nu_exec/mkelf.py`, exécute sous `qemu-i386-static`, puis compare
avec `rose nu_exec/pN_*.rose`. Sortie attendue :

```
vérifié
OK p1_carre : nu=121 o entree=134512946 QEMU rc=25 stdout=[19000000] Rose interprete=25
OK p2_somme : nu=172 o entree=134512896 QEMU rc=15 stdout=[0f000000] Rose interprete=15
OK p3_si    : nu=141 o entree=134512896 QEMU rc=1  stdout=[01000000] Rose interprete=1
TALLY : 3/3 programmes Rose compiles en binaire nu et executes
```

L'enveloppe ELF est générée ainsi (le binaire nu reste inchangé) :

```bash
python3 -c "import sys; sys.path.insert(0,'nu_exec'); from mkelf import build; \
  build(open('nu_exec/p1_carre.bin','rb').read(),'nu_exec/p1_carre.elf',entree=134512946)"
qemu-i386-static ./nu_exec/p1_carre.elf ; echo $?     # 25
```

## 4. Validation globale (chantier)

| Contrôle | Résultat mesuré |
|---|---|
| `rose verif /tmp/opencode/nu_exec.rose` (5 fichiers concaténés) | `vérifié` |
| Compilation C0 du même programme concaténé (version finale) | ELF produit `956128` o, `rc=0` (`/tmp/opencode/nu_exec_c0_v3.elf`) ; sortie identique octet pour octet à l'oracle Rust (md5 `85d64d30138d213a2b4f7e0e5547b7ee` des deux côtés) |
| Parité self-host (141 sources, oracle Rust vs moteur C0) | **141 CONFORMES / 0 DIVERGENT / 0 BLOQUÉ**, RC=0 |
| Point fixe `rose rir compilateur/emetteur.rose` puis `rose compilateur/emetteur.rose compilateur/emetteur.rose` | md5 identique `b3ed9262dd54776a6dbd07bc12368d0e` |
| Exécution native | 3/3 (voir tableau §1) |

Empreintes : moteur self-hosté `c0f90c6ebfb6835b1725f0a6dbec2977`, oracle
Rust `7614a534b9411917ef9cd49a834014a8` (les deux de la batterie de parité).
Le TSV horodaté est dans `/tmp/opencode/v13run_parite/`.

Commandes correspondantes :

```bash
R=/home/clausake/Stefan/ROSE/rose/rose
bash scripts/garde_fou_mem.sh 12000000 $R verif /tmp/opencode/nu_exec.rose
printf 3 > nu_exec/choix.txt
bash scripts/garde_fou_mem.sh 12000000 /tmp/opencode/C0_h.elf \
  /tmp/opencode/nu_exec.rose > /tmp/opencode/nu_exec_c0_v3.elf
chmod +x /tmp/opencode/nu_exec_c0_v3.elf
/tmp/opencode/nu_exec_c0_v3.elf > /tmp/opencode/nu_v3_c0.out
diff <($R interprete /tmp/opencode/nu_exec.rose) /tmp/opencode/nu_v3_c0.out
bash scripts/garde_fou_mem.sh 12000000 bash scripts/verifier_parite_selfhost.sh \
  --moteur /tmp/opencode/moteur_141.elf --rose-bin $R \
  --workdir /tmp/opencode/v13run_parite
# -> tests=141  CONFORMES=141  DIVERGENTS=0  BLOQUES=0 ; VERDICT RC=0
$R rir compilateur/emetteur.rose > /tmp/opencode/pf1_nu.rose
$R compilateur/emetteur.rose compilateur/emetteur.rose > /tmp/opencode/pf2_nu.rir
md5sum /tmp/opencode/pf1_nu.rose /tmp/opencode/pf2_nu.rir
# -> b3ed9262dd54776a6dbd07bc12368d0e x2
```

## 5. Limites (à ne pas présenter comme un compilateur complet)

- Entiers **32 bits signés** uniquement (pas de BigInt, pas de flottants,
  pas de rationnels, pas de chaînes).
- Pas de surcharge, pas de fonctions d'ordre supérieur, pas de récursivité :
  l'adresse d'une fonction appelée doit être déjà connue (les fonctions sont
  émises dans l'ordre de définition).
- Appel conventionnel maison : arguments dans EAX/ECX/EDX, retour dans EAX,
  EBP = cadre, slots à `[EBP-4(k+1)]`. `push ebp`/`pop ebp` sont deux octets
  bruts parce que le backend n'a ni `push`/`pop` et n'émet pas l'octet SIB
  nécessaire à `[ESP+disp32]`.
- Le binaire livré est **nu** : l'adresse de chargement est un paramètre du
  contrat (`ln_charge`), pas un champ du fichier. L'enveloppe ELF et le
  syscall Linux `int 0x80` servent **uniquement** à rendre l'exécution
  observable sur cette machine ; ils ne font pas partie de la brique et ne
  prouvent pas l'existence d'un système d'exploitation Rose.
- `qemu-i386-static` est un banc d'exécution, pas un banc de performance.
- Aucun fichier de `compilateur/abaisser_nu.rose` (agent v13lower) n'a été
  lu ni modifié : l'abaisseur ci-dessus est indépendant et écrit dans ce
  seul worktree.

## 6. Fichiers créés dans ce worktree (aucun commit/push)

- `compilateur/comp_nu_demo.rose` — abaisseur RIR → ops backend → octets nus.
- `compilateur/tests_exec_nu.rose` — les 3 sources Rose, le pilotage.
- `scripts/verifier_exec_nu.sh` — rejeu complet de la preuve.
- `rose/docs/EXEC_NU_REELLE.md` — ce document.
- `nu_exec/` (artefacts non suivis) : `p1_carre.bin`, `p2_somme.bin`,
  `p3_si.bin`, leurs enveloppes `.elf`, `desasm.txt`, `mkelf.py`, `comp*.out`.
