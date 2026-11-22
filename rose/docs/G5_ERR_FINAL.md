# G5ERR — message exact du rc=2 silencieux (22/11/2026)

## Sonde

`/tmp/opencode/p3.rose` :

```rose
fn p(a: entier) -> chaine { retour "≈" }
>> p(1)
```

`/tmp/opencode/C0_final.elf p3.rose` → **rc=2**, stdout vide, stderr vide.
L'émetteur hébergé (`rose emetteur_elf.rose p3.rir`) réussit ; le RIR est correct.

## Méthode

Au lieu de réémettre un C0 instrumenté (~80 min), le message a été lu **directement
en mémoire** dans le binaire C0 existant avec GDB. Le chemin d'erreur G5 se trouve
à `0x403dec` dans `C0_final.elf` (juste avant l'appel au stub `afficher_erreur`,
puis `quitter(2)`) ; `x0` y contient le descripteur de chaîne
`[tag=1][len][octets…]` construit par `afficher_erreur("G5 : " + $g5_err[0])`.

Commande exacte :

```sh
cd /tmp/opencode
gdb -q -batch -ex 'set pagination off' -ex 'b *0x403dec' \
  -ex 'run /tmp/opencode/p3.rose' \
  -ex 'set $p=$x0 & ~7' \
  -ex 'printf "LEN=%llu\n", *(unsigned long long*)($p+8)' \
  -ex 'x/s $p+16' ./C0_final.elf
```

Sortie brute :

```text
Breakpoint 1 at 0x403dec

Breakpoint 1, 0x0000000000403dec in ?? ()
LEN=45
0x6004a9c8:	"G5 : sous_chaine invalide : 5..4 sur 4 octets"
```

**Message G5ERR exact : `G5 : sous_chaine invalide : 5..4 sur 4 octets`**
(soit, dans `$g5_err[0]` : `sous_chaine invalide : 5..4 sur 4 octets`).

Corroboration indépendante : le diagnostic détaché (PID 2889184, journal
`/tmp/opencode/diag_emettre.log`) a réémis un `C0_diag.elf` avec
`ecrire_bytes("/dev/stdout", "G5ERR:" + $g5_err[0] + "\n")` à la place du stub.
Exécuté sur la même sonde :

```text
$ ./C0_diag.elf p3.rose ; echo rc=$?
G5ERR:sous_chaine invalide : 5..4 sur 4 octets
rc=2
```

(47 octets sur stdout, stderr vide.)

## Cause immédiate

`g5_sub()` (emetteur_elf.rose:651) est appelée avec `a=5`, `b=4` sur la chaîne
`"ops\n"` (4 caractères) : `g5_nu_op($ligne)` fait `g5_sub($ligne, 5, longueur($ligne))`
(emetteur_elf.rose:601). Un appel avec `a > b` déclenche `g5_fail`.

GDB, point d'arrêt conditionnel à l'entrée de `g5_sub` (`0x5288b0`, condition
`$x1 > $x2`) :

```text
A=5 B=4 LR=0x51a914
LEN=4
0x60015338:	"ops\n"
```

`0x51a910` est bien l'appel `bl 0x5288b0` de `g5_nu_op`.

## Cause profonde (démontrée)

`g5_parse()` découpe le RIR avec `g4_lignes()`, dont le test de fin de ligne est
`sous_chaine($s, $i, $i + 1) == "\n"`. Le backend G5 **fusionne** ce motif
(`g5_eval`, emetteur_elf.rose:1721-1771) en un accès OCTET direct :

```rose
g5_emit(addi(1, 1, 16))   # a + 16 : indice OCTET
g5_emit(addr(7, 6, 1))
g5_emit(ldrbio(8, 7, $li))
```

Or sous G5, les indices de `sous_chaine` sont des **points de code** (routine
`g5_sousl`, UTF-8-aware), et les descripteurs stockent des **octets UTF-8**.
Dès qu'un caractère multi-octets précède la position `i`, l'octet lu est décalé
de `k-1`. La coupure en lignes du RIR se désynchronise, la ligne `  ops`
devient `ops\n` (4 caractères), puis `g5_nu_op(5,4)` échoue.

Reproduction minimale (sans le compilateur), compilée par C0 puis exécutée :

```rose
fn f(s: chaine) -> chaine {
  $r = ""
  $i = 0
  $n = longueur(s)
  tant_que ($i < $n) {
    si (sous_chaine(s, $i, $i + 1) == "\n") { $r = $r + "Y" } sinon { $r = $r + "N" }
    $i = $i + 1
  }
  retour $r
}
>> f("≈\nx\n")
```

| littéral | C0 | natif |
|---|---|---|
| `"a\nb\n"` | `NYNY` | `NYNY` |
| `"é\nx\n"` | `NNYN` | `NYNY` |
| `"≈\nx\n"` | `NNNY` | `NYNY` |
| `"𝄞\nx\n"` | `NNNN` | `NYNY` |

Le décalage vaut exactement `k-1` octets pour un caractère de `k` octets.
Le correctif est dans `compilateur/emetteur_elf.rose` : la fusion compare désormais
les octets UTF-8 du littéral à partir du décalage OCTET obtenu en balayant le
descripteur jusqu'au point de code `a` (voir SELFHOST_FINAL.md).
