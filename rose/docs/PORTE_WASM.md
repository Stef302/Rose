# Porte de performance M6.2 — runtime WebAssembly officiel

> **État au 17/11/2026 : PORTE NON MESURÉE.**
> Le seuil contractuel est `fib(27)` via le runtime officiel **≤ 20 ms**, avec
> une sortie **identique** à l'interpréteur maison (`196418`). Aucun runtime
> officiel n'est installé sur cette machine et l'artefact runner n'est pas
> publié : le seuil n'est **pas revendiqué comme atteint**.

---

## 1. Environnement vérifié (constat daté, à re-mesurer)

| Vérification | Résultat réel |
|---|---|
| `which wasmtime wasmer` | aucun |
| `find / -name 'wasmtime' -type f 2>/dev/null \| head` | aucun |
| `find / -name 'wasmer' -type f 2>/dev/null \| head` | aucun |
| `~/.cache/rose/runtimes` | dossier absent |
| `PATH` (`rose-wasmtime-runner`, `rose-wasmer-runner`) | absent |
| Artefact `rose-wasmtime-runner-…` sur `Stef302/Rose` | non publié |
| `sha256_attendu()` | renvoie `""` → `installer()` refuse **par design** |

Le harnais `scripts/mesurer-wasm.sh` imprime donc exactement :

```
PORTE NON MESURABLE: runtime absent
```

⚠ **Binaire daté** : `rose/rose` est construit à `21:59:56` alors que
`rose/src/main.rs` (câblage CLI M6.2 `--export` / `--runtime` /
`--install-runtime`) est modifié à `22:03:57` le 17/11/2026. Le binaire
présent **ne contient donc pas encore** ce câblage : `rose wasm … --export`
y est interprété comme un nom de fichier. Il faudra reconstruire le binaire
avant toute mesure passant par la CLI (à coordonner avec le chantier de A3 —
ce document et le harnais ne touchent à aucun `.rs`).

---

## 2. Valeurs de référence maison mesurées

Deux chemins de code distincts, **à ne pas confondre** :

| Chemin | Mesure | Détail |
|---|---|---|
| Interpréteur de pile **wasm** (crate de validation isolée, profil `release`) | **médiane 172,1 ms** | 6 échantillons `171,4 / 171,8 / 172,0 / 172,2 / 172,4 / 172,5 ms`, `fib(27)=196418`, 17/11/2026 |
| Binaire natif `./rose/rose` (commande du chantier, source Rose directe) | **~4,6 ms** en haute résolution | 12 mesures `perf_counter`, min `3,473`, max `6,004` ms ; `0.00` affiché par `/usr/bin/time -f '%e'` (résolution 10 ms) |

C'est la **première** ligne qui sert de comparaison à la porte M6.2 : le
runtime officiel doit battre l'interpréteur de pile wasm, pas l'exécution
native du moteur.

### 2.1 Commande exacte du chantier et sortie réelle

```sh
printf 'fn fib(n: entier)->entier { si (n<2) { retour n }; retour fib(n-1)+fib(n-2) }\n>> fib(27)\n' > /tmp/fib27.rose
for i in 1 2 3; do /usr/bin/time -f '%e' ./rose/rose /tmp/fib27.rose >/dev/null; done
```

Sortie observée :

```
0.00
0.00
0.00
```

Le `0.00` est un artefact de résolution de `%e` (2 décimales), pas un calcul
instantané. En chronométrage haute résolution (12 exécutions du même
`/tmp/fib27.rose`, `time.perf_counter`, sortie `196418` à chaque fois) :

```
sample1: 5.970 ms   sample7:  5.443 ms
sample2: 6.004 ms   sample8:  3.562 ms
sample3: 3.904 ms   sample9:  3.473 ms
sample4: 3.629 ms   sample10: 5.380 ms
sample5: 3.578 ms   sample11: 5.387 ms
sample6: 3.568 ms   sample12: 5.704 ms
médiane 4.642 ms — min 3.473 ms — max 6.004 ms
```

Vérification de mise à l'échelle (le calcul est bien réel, pas un cache) :

```
fib(25) =   75025   médiane  2,089 ms
fib(27) =  196418   médiane  5,721 ms
fib(30) =  832040   médiane 11,701 ms
fib(32) = 2178309   médiane 41,605 ms
```

---

## 3. Module de mesure (intégré au harnais, rien ajouté au dépôt)

Le script `scripts/mesurer-wasm.sh` écrit dans un `mktemp` (nettoyé par un
`trap`) un module **fib récursif** `i32 -> i32`, export `fib`, qui rejoue
exactement la charge de l'interpréteur maison (`fib(n-1)+fib(n-2)`) — et non
un fib itératif qui rendrait la comparaison biaisée.

```
taille  : 61 octets
md5     : 1ea8a09ca1186a7f9de48d2068f2dcba
sha256  : 00c490e30433166670217e7999f5bd5dd678e67712ba6850ec94ca12c15119d3
base64  : AGFzbQEAAAABBgFgAX8BfwMCAQAHBwEDZmliAAAKHgEcACAAQQJIBH8gAAUgAEEBaxAAIABBAmsQAGoLCw==
```

Validations indépendantes effectuées :

* **Node WebAssembly** : `fib(27) = 196418`, `fib(10) = 55` ;
* **chargeur M6.1 de Rose** : `rose wasm fib_rec_if.wasm` →
  `fn fib(a0: entier) -> entier` / `1 fonction(s) exportée(s)`.

Note : un `fib_rec.wasm` présent dans `/tmp/opencode/rose-wasm-runtime-test/`
avait une taille de section `code` trop grande d'un octet et était **rejeté**
par Node (`section was shorter than expected size`). Le module ci-dessus est
corrigé et validé ; ne pas le remplacer par une copie non vérifiée.

---

## 4. Procédure exacte de mesure après publication de l'artefact

1. **Vérifier le compte GitHub avant toute publication** :
   `GH_TOKEN="$(cat ~/.config/opencode/.gh_token)" gh api user -q .login` doit
   rendre `Stef302`. Le dépôt est `Stef302/Rose` (release `0.1.5`) ; jamais
   `clausake`.
2. **Confirmer l'artefact** :
   `gh release view 0.1.5 --repo Stef302/Rose` doit lister
   `rose-wasmtime-runner-v0.1.5-aarch64-linux` (ou `x86_64-linux` selon la
   cible, `rose/src/wasm_runtime.rs::cible()`).
3. **Récupérer l'empreinte SHA-256 officielle** depuis la release (checksum).
   Ne jamais télécharger ni exécuter un runner sans empreinte vérifiée.
4. **S'assurer que le binaire Rose inclut le câblage M6.2** (voir §1 : le
   binaire présent est antérieur à `main.rs`). Reconstruire le moteur avant
   d'employer la CLI, en coordination avec le propriétaire du chantier.
5. **Installer explicitement** (seul chemin de téléchargement, vérifié) :

   ```sh
   ROSE_WASM_RUNTIME_SHA256=<empreinte-officielle> \
     ./rose/rose wasm --install-runtime wasmtime
   # miroir de test interne seulement :
   # ROSE_WASM_RUNTIME_URL=<url-mirow> ROSE_WASM_RUNTIME_SHA256=<empreinte> ...
   ```

   Contrôle : `test -x ~/.cache/rose/runtimes/wasmtime-v0.1.5 && echo OK`.
6. **Tester le contrat du runner** (doit afficher `196418`) :

   ```sh
   ~/.cache/rose/runtimes/wasmtime-v0.1.5 /tmp/fib_rec_if.wasm fib 27
   ```
7. **Mesurer la porte** :

   ```sh
   bash scripts/mesurer-wasm.sh
   ```

   Le script détecte le runner, chronomètre 6 échantillons (médiane des
   rangs 3 et 4), vérifie `196418` à chaque échantillon et imprime un
   `VERDICT: PASS` uniquement si la médiane ≤ 20 ms **et** si la sortie est
   cohérente. Sinon : `VERDICT: ÉCHEC`.
8. **Mesure manuelle équivalente** (wasmtime nu, pour recouper) :

   ```sh
   # avec le module intégré extrait par le script, ou un fib.wasm vérifié :
   wasmtime run --invoke fib fib.wasm 27
   ```
9. **Critères d'acceptation** : A/A/A entrelacé, ≥ 6 échantillons, médianes,
   sortie identique au caractère près ; n'accepter le changement que s'il bat
   les deux contrôles voisins. **Re-mesurer après tout changement** de modèle,
   de quantification ou de topologie : toute valeur ici est un constat daté,
   pas une consigne.

---

## 5. Pourquoi la porte reste « non mesurée »

* aucun binaire `wasmtime` / `wasmer` n'est présent et aucun n'a été téléchargé
  (pas de téléchargement non authentifié) ;
* l'artefact runner n'est pas publié sur `Stef302/Rose` ;
* `sha256_attendu()` est vide tant que l'artefact n'est pas publié, donc
  `installer()` refuse — c'est la garde-fou voulue, pas un bug ;
* le harnais ne peut donc que constater l'absence. Le seuil ≤ 20 ms n'est
  **pas** présenté comme acquis.

Sortie exacte du harnais dans cet état (`bash scripts/mesurer-wasm.sh`) :

```
PORTE NON MESURABLE: runtime absent

  Détection effectuée (aucun téléchargement, aucune installation) :
    cache scruté      : /home/clausake/.cache/rose/runtimes
    noms attendus     : wasmtime-v0.1.5, wasmer-v0.1.5
    PATH (runners)    : rose-wasmtime-runner, rose-wasmer-runner
    PATH (binaires)   : wasmtime, wasmer
    which wasmtime    : absent
    which wasmer      : absent

  Référence maison (crate de validation isolée, profil release,
    interpréteur de pile wasm, 17/11/2026) : médiane 172,1 ms
    6 échantillons : 171,4 / 171,8 / 172,0 / 172,2 / 172,4 / 172,5 ms
    fib(27) = 196418
  Référence maison (binaire natif ./rose/rose, commande du chantier) :
    ~4,6 ms en haute résolution (autre chemin de code, JIT/natif).
    La commande /usr/bin/time -f '%e' affiche 0.00 (résolution 10 ms).

  Seuil M6.2 : fib(27) via runtime officiel ≤ 20 ms — NON MESURÉ, non revendiqué.
  Procédure après publication de l'artefact : rose/docs/PORTE_WASM.md
```
