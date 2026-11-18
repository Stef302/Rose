# Bench Rose — constats mesurés

Horodatage : **2026-11-18T12:31:42+01:00** (re-mesuré 12:33). Machine : **aarch64**, binaire `rose/rose` SHA256 `72d8cc31089e80ef…`, Rose 0.1.5.

Règle AGENTS.md : mesurer, jamais citer une spec.Protocole : 9 échantillons par mesure, médiane, min/max, A/A/A entrelacé pour les comparaisons. Tous les.rc = 0 sauf mention contraire.

## Mesures principales

| Programme | Médiane | Min | Max | Sorties distinctes |
|---|---|---|---|---|
| `fib(35)` récursif (JIT) | **210,25 ms** | 209,25 | 225,22 | 1 (stable) |
| jardin `premiers` (20 tests + cross-check 0..499) | **2,61 ms** | 2,29 | 4,06 | 1 (stable) |
| `bench_global` (fib 317811 + factorielle 10⁶) | **680,33 ms** | 671,82 | 687,17 | variable (temps affiché) |
| `factorielles.rose` (N=5000) | **107,35 ms** | 99,64 | 121,64 | variable (temps affiché) |

Deuxième salve : fib(35) 222,53 ms → 210,25 ms ; jardin 3,82 → 2,61 ms (bruit). Les valeurs calculées sont stables (1 sortie distincte pour fib et le jardin) ; seules les lignes de temps varient dans bench_global/factorielles.

## Comparaison JIT / interpréteur (A/B entrelacé, 6 paires)

| Mode | Médiane fib(35) |
|---|---|
| JIT Cranelift (défaut) | **209,78 ms** |
| `ROSE_SANS_JIT=1` (interpréteur pur) | **11899,36 ms** |
| **Facteur** | **~56,7×** |

## Appel WebAssembly externe (6 paires)

| Voie | Médiane | Statut |
|---|---|---|
| officielle (wasmtime 50.0.1, runner publié) | **2,90 ms** | rc=0, sortie `196418` |
| maison (interpréteur de pile) | — | **échec rc=1** (« récursion wasm trop profonde (>256) ») — aucune mesure valide |

⚠ Piège évité : une première mesure affichait « maison 0,71 ms, 3× plus rapide que wasmtime ». C'était un **échec rapide** (rc=1, sortie vide), pas une performance. Le script filtre désormais les rc ≠ 0. La voie maison n'est pas comparablesur ce module récursif.

## Ce que ces chiffres disent

- Le jardin Rose (2,61 ms) reste très en deçà du JIT sur `fib(35)` (210 ms) : ce sont deux charges différentes — le jardin fait 500 tests de primalité sur de petits nombres, fib(35) est 35 millions d'appels récursifs.
- Le JIT apporte ~57× sur une boucle récursive chaude : c'est le gain architectural majeur.
- L'appel wasm externe (2,90 ms) est un **appel de bibliothèque**, pas une vitesse d'exécution Rose : il mesure le démarrage du runner + la compilation JIT de wasmtime + le calcul. À ne pas comparer avec le JIT Rose sur du code Rose.
- Les temps de `bench_global`/`factorielles` incluent des affichages de temps internes : la variation inter-échantillons est attendue, pas un bug.

## Contexte

Voir `CAPACITES.md` (réalisations et plafonds théoriques), `AUDIT_FINAL.md` (audit complet 18/11), `WASM_RUNTIME.md` (pont wasm), `PLAN.md` (jalons M1–M6). Toute mesure est un constat daté : re-mesurer après tout changement de modèle, de quantification ou de topologie.
