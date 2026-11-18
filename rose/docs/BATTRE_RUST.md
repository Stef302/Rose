# Battre Rust AOT sur fib(35) par la méthode LLVM (transformation récursion → boucle)

**Horodatage** : 2026-11-18T12:46Z → 13:25Z. Tous les chiffres sont des
**constats datés** (règle AGENTS.md) : re-mesurer après tout changement de
binaire, de topologie ou d'état machine.

**Binaire final** : `rose/rose` SHA256
`ab01f8fd43f4fda304706697d1fe9e91981b50746d86ab282f8412770d665ee1`
(la baseline était `8a4ac955…`).

**Machine** : aarch64 GB10, 20 cœurs hétérogènes (X925 cœur 19,
`cpu_capacity=1024`, 3,9 GHz ; governor `performance`). Toutes les salves sont
**pinées** sur le cœur 19 via `taskset -c 19`, 9 échantillons par
implémentation, ordre entrelacé par rotation, résultat vérifié à chaque
échantillon (`9227465`). Deux horloges distinctes : temps **interne** mesuré par
`temps()` dans le programme et temps **wall** du processus ; le classement
porte sur l'interne.

---

## 1. Baseline re-mesurée (12:46:48Z, cœur 19, 9 échantillons)

`PIN_CPU=19 python3 /tmp/opencode/rosebench/battre_fib.py baseline_avant`
(sortie `battre_baseline_out.txt`), binaire `rose/rose` SHA256
`8a4ac955dced44dab318ca6eebbe309af4f3e8f0480b5bb3c2ae76fae48d6eab` :

| Implémentation | interne médian | min | max | wall médian |
|---|---:|---:|---:|---:|
| Rose JIT (source) | **51,329 ms** | 51,308 | 51,392 | 52,857 ms |
| Rust `rustc -O` | **12,636 ms** | 12,581 | 12,931 | 14,238 ms |

Rapport Rose/Rust = **4,0620** (Rust 4,06× plus rapide). Baseline conforme à
`VITESSE_VS_RUST.md` (51,310 ms / 12,663 ms).

---

## 2. Ce que fait LLVM — étudié, pas cité

Désassemblage réel de `pure_rs` (`nm -C` puis `objdump -d`), fonction
`pure::fib` à `0x90bc` :

```
90bc stp x29,x30,[sp,#-32]!       prologue
90c8 mov x19,x0                   n
90cc cmp x0,#2
90d0 b.cs 90e4                    n >= 2 -> boucle
90d4 add x0,x19,xzr               base : retour n
...
90e4 mov x20,xzr                  acc = 0
90e8 sub x0,x19,#1                argument = n-1
90ec bl  90bc                     appel récursif DIRECT
90f0 sub x19,x19,#2               n -= 2
90f4 add x20,x0,x20               acc += résultat
90f8 cmp x19,#1
90fc b.hi 90e8                    continuation
9100 add x0,x19,x20               retour base(n_final) + acc
```

**Identité utilisée** : `f(n) = f(n-c1) + f(n-c2)` se réécrit
`acc = Σ f(n_i - c1)`, `n_{i+1} = n_i - c2`, avec retour `base(n_final) + acc`.
Le nombre d'appels passe de `2·F(36)−1 = 29 860 703` (récursion naïve Rose) à
`F(36) = 14 930 351` (Rust), soit exactement **÷2**.

---

## 3. Ce qui a été modifié (`rose/src/jit.rs` uniquement)

**Aucun nom de fonction n'est codé en dur.** Le motif reconnu est purement
structurel :

```
fn f(n: entier) -> entier {
    si (n < k) { retour n | LITTÉRAL >= 0 }     // k > 0
    retour f(n - c1) + f(n - c2)               // 0 < c1 <= c2 <= k
}
```

1. `detecter_recurrence_binaire(sig)` : détecte ce motif sur l'AST. Accepte
   `<`, `<=` et les formes renversées, `base = n` ou littéral non négatif,
   deux appels avec pas positifs (ou un seul appel répété `f(n-c)+f(n-c)`).
   Tout le reste retombe sur l'émetteur historique inchangé.
2. `emettre_boucle_recursive(...)` : émet la boucle à accumulateur avec
   sous-tractions `isub` nues (sans test de débordement — prouvé par `n >= k >=
   c2 >= c1`), appel récursif direct `bl` au `FuncId` colocated, `iadd` nu,
   paramètres de bloc Cranelift explicites, aucun `rose_set_ret_tag` sur le
   chemin rapide.
3. `borne_securisee(d)` : simule la récurrence en i128 **à la compilation**
   pour calculer le plus grand `n` dont tous les résultats tiennent en i64.
   Au-delà (cas bigint), un `brif` délègue à `rose_appel_interpreteur` avec le
   corps d'origine — exactité conservée, et le prologue reste maigre (2 `stp`
   seulement, mesuré : l'émission du corps générique *dans* la fonction
   gonflait le prologue à 5 sauvegardes et coûtait ~13 %).
4. `appeler_trampoline` : remet le tag global de retour à 0 à la frontière
   Rust→JIT, ce qui rend l'omission de `rose_set_ret_tag` sûre.

`rose/src/rir.rs` n'a pas eu besoin d'être modifié : fib est compilé par la
voie AST, pas par la voie RIR.

### Étapes et mesures (toutes pinées sur le cœur 19)

⚠ Les étapes intermédiaires sont des **observations unitaires** (une exécution
chacune, le signal étant alors ≫ bruit) ; les trois dernières lignes sont des
**médianes de 9 échantillons entrelacés**, seules décisionnelles.

| Étape | interne | vs baseline | Rapport à Rust |
|---|---:|---:|---:|
| Baseline (avant) | 51,329 ms | 1,000 | 4,06× |
| `b3c0b72` chemin rapide `-` (déjà en place) | — | — | — |
| Boucle LLVM, avec fastpath `-` seul | 50,712 ms | 0,988 | 4,01× |
| + sous-traction nue + appel direct | 40,857 ms | 0,796 | 3,23× |
| + addition non négative (sans tag) | 34,678 ms | 0,676 | 2,74× |
| + garde de plage, tag conservé | 37,724 ms | 0,735 | 2,98× |
| + garde, tag omis | 37,023 ms | 0,721 | 2,93× |
| + délégation interpréteur (au lieu du corps généré) | **19,900 ms** | **0,388** | 1,57× |
| + paramètres de bloc explicites | 19,911 ms | 0,388 | 1,57× |
| + réordonnancement des blocs | 19,922 ms | 0,388 | 1,57× |

Le gain décisif vient de la **délégation du repli hors fonction** : le corps
générique émet des valeurs vivantes qui forcent des registres callee-saved
supplémentaires dans le prologue de *chaque* appel récursif
(7 registres sauvés au lieu de 4, `objdump` à l'appui).

---

## 4. Résultat final (13:22:07Z → 13:24:27Z, cœur 19, 9 échantillons)

`PIN_CPU=19 python3 /tmp/opencode/rosebench/battre_fib.py apres_delegue`
(sortie `battre_delegue_out.txt`), puis `apres_params_explicites` et
`apres_ordre_blocs` (confirmation, écart < 0,1 %) :

Dernière salve sur le binaire final (13:30:59Z, `battre_final_out.txt`) :

| Implémentation | interne médian | min | max | wall médian |
|---|---:|---:|---:|---:|
| **Rose JIT (boucle LLVM générique)** | **19,905 ms** | 19,887 | 19,924 | 21,160 ms |
| Rust `rustc -O` | **12,634 ms** | 12,601 | 12,649 | 13,624 ms |

- Rose/Rust = **1,5754** → **Rust reste 1,58× plus rapide**.
- Amélioration réelle : **×2,58** par rapport à la baseline du jour.
- Appels : 29 860 703 → **14 930 351** (`I(35)` calculé par récurrence et
  confirmé par le dump IR : un `call` par itération de boucle).
- Coût par appel : 1,72 ns → **1,33 ns** ; Rust 0,85 ns.
- Chiffre final : **51,329 → 19,905 ms = ×2,58**.

### Verdict honnête

**Non, Rose ne bat pas Rust sur fib(35).** Le meilleur chiffre mesuré est
**19,905 ms** contre **12,634 ms** pour `rustc -O` : il manque **~7,3 ms**,
soit un facteur **1,58×**.

La transformation de LLVM a été reproduite **exactement dans son principe**
(même nombre d'appels, même structure d'accumulateur) et elle est **générique**
: elle s'applique à toute fonction du motif ci-dessus, pas seulement à `fib`.
L'écart résiduel n'est plus dans l'algorithme ni dans le nombre d'appels, mais
dans la **qualité du code machine** :

```
Rose (objdump du JIT, boucle)              Rust (objdump -d pure_rs)
mov x1,x20   ; n -> x1                     sub x0,x19,#1
sub x0,x1,#1 ; arg                         bl   fib
mov x20,x1   ; sauver n                    sub x19,x19,#2
bl  0x1000   ; appel                       add x20,x0,x20
mov x1,x20                                 cmp x19,#1
sub x1,x1,#2 ; n -= 2                      b.hi 90e8
mov x20,x1
add x19,x19,x0 ; acc
mov x0,x20
cmp x0,#2
b.lt sortie
mov x1,x20
b   debut
```

~13 instructions + `bl` chez Rose contre ~6 + `bl` chez Rust : Cranelift ne
maintient pas `n` dans un registre callee-saved stable à l'entrée du bloc de
boucle (6 `mov` de permutation par itération), alors que LLVM le garde dans
`x19` du prologue à la fin. **Il manque une allocation de registres / un
émetteur machine dédié au motif** (comme le suggérait déjà
`PLAFOND_VITESSE.md` §5), pas une optimisation de compilation de plus.

---

## 5. Validation

Toutes les commandes ont été exécutées sur le binaire final.

```bash
cd /home/clausake/Stefan/ROSE/rose
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release --bin rose                     # Finished, 0 erreur
cp -f target/release/rose rose

# 1. fib(25)
taskset -c 19 ./rose /tmp/opencode/rosebench/fib25.rose
# -> 75025

# 2. fib(35) — 9 échantillons pinés (cf. §4)
PIN_CPU=19 python3 /tmp/opencode/rosebench/battre_fib.py apres_delegue

# 3. jardin des premiers, depuis la racine du dépôt
cd /home/clausake/Stefan/ROSE
taskset -c 19 ./rose/rose jardins/tests/premiers.rose | tail -1
# -> ── jardins/premiers : 20 tests, 0 échec ──

# 4. débordements i64 exacts (repli bigint)
taskset -c 19 ./rose/rose /tmp/opencode/rosebench/overflow_test.rose
# -> 9223372036854775808
#    9223372036854775806
#    -9223372036854775809
#    18446744073709551614
#    1000000000000000000000000000000000000

# 5. GÉNÉRICITÉ : trois autres fonctions du même motif (aucun nom codé en dur)
taskset -c 19 ./rose/rose /tmp/opencode/rosebench/generique.rose
# -> 178 (lucas(10), base=2), 1024 (double(10), f(n-1)+f(n-1)), 89 (decale(10), base n<=2)

# 6. point fixe M4.5 — oracle RIR
./rose/rose rir compilateur/emetteur.rose | md5sum
# -> bdbef63681608a0b22a5637539e9d738  -

# 7. auto-émission complète (peut prendre plusieurs minutes sur cette machine)
timeout 280 ./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose \
  > /tmp/opencode/rosebench/emit_final.txt
md5sum /tmp/opencode/rosebench/emit_final.txt
# -> bdbef63681608a0b22a5637539e9d738  (260 692 octets, rc=0)

# 8. quatre oracles M4.1–M4.4 sur un échantillon de 12 fichiers du corpus
#    (tokens / ast / verif / rir comparés octet pour octet aux composants Rose)
# -> 0 écart

# 9. dump machine (constat de l'écart)
ROSE_DUMP_CODE=/tmp/opencode/rosebench/loop6_code.bin \
  taskset -c 19 ./rose/rose /tmp/opencode/rosebench/rose_fib.rose
objdump -D -b binary -m aarch64 /tmp/opencode/rosebench/loop6_code.bin
```

**Point fixe M4.5 — CONSTAT** : les deux voies rendent le même md5
`bdbef63681608a0b22a5637539e9d738` sur le binaire final : oracle RIR
(`rose rir …`, ~27 ms) et auto-émission complète (260 692 octets, rc=0).
⚠ L'auto-émission a pris plusieurs minutes pendant la fenêtre 13:55–14:29Z
(alors qu'elle était quasi instantanée à 12:44Z) **avec le binaire initial
`rose_v0` comme avec le binaire modifié** : ralentissement de l'état machine
(concurrents qui compilent/committent dans le dépôt), pas une régression de
cette étape. Chaque mesure est un constat daté.

Aucun commit, aucune publication. Seul `rose/src/jit.rs` a été modifié.

---

## 6. Reproduction

```bash
cd /tmp/opencode/rosebench
PIN_CPU=19 python3 battre_fib.py <label>
```

Sorties brutes : `battre_baseline_out.txt`, `battre_boucle_garde_out.txt`,
`battre_delegue_out.txt`, `battre_params_out.txt`, `battre_ordre_out.txt`.
Dumps IR/machine : `loop6_code.bin` (binaire final), `current_ir.txt`
(baseline). Script de mesure : `battre_fib.py`.
