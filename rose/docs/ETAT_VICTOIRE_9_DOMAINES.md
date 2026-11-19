# État — 9 domaines tous gagnés (constat daté, 20/11/2026)

## Résultat

**Rose bat Rust sur les 9 domaines du banc principal**, sur deux cœurs distincts,
avec le binaire SHA256 `a2f98fe93799e2faa69a21a486bcd4b7bc7def976d7b62181c0c6c95ca04ff3c`.

Banc `/tmp/opencode/benchmulti/bench_now.py`, 9 échantillons entrelacés,
pinage `taskset -c 19` puis `-c 6`, deux horloges (murale / calcul seul),
valeur de sortie vérifiée à chaque tirage. Ratios Rose/Rust (calcul seul,
médianes — **< 1 = Rose plus rapide**) :

| Domaine | c19 | c6 |
|---|---:|---:|
| `fib(35)` | 0,92 | 0,91 |
| Boucle 1..10⁶ | 0,39 | 0,39 |
| Premiers < 50 000 | 0,85 | 0,85 |
| Factorielle 5000 | 0,21 | 0,21 |
| PGCD 1000 chiffres | 0,66 | 0,69 |
| Chaîne 20 000 | 0,61 | 0,78 |
| Liste 100 000 | 0,64 | 0,72 |
| Table 3000 | 0,69 | 0,65 |
| E/S 10 Mo | 0,63 | 0,61 |

**9 victoires / 9**, contre 7/9 la veille. Les trois gains de cette dernière
vague : liste (×1,6), table (×1,5), suite complexe (÷109).

## Autres bancs (constats séparés, non généralisés)

- `bench_global` : Rose affiche `33` ms en interne (constat de fonctionnement,
  le jumeau Rust complet n'a pas été rejoué dans cette salve).
- Suite complexe : Rose rend `0.9999999999860738` et `333333833333500000`,
  sortie bit-à-bit identique au binaire de référence.
- Englobant : le fichier **commité** `bench/englobant/bench_englobant.rose`
  reste cassé (`variable de boucle 'i6' existe déjà`, ligne 60) — c'est un
  défaut préexistant documenté par CMP2 ; la copie réparée
  `/tmp/opencode/cmp2_agents/bench_englobant_fixed.rose` s'exécute et rend
  `2820 / 157158 / 11899223598484`. Correction à porter dans le dépôt.

## Limites

Ces 9 ratios ne couvrent **pas** les autres bancs (global, englobant, racines
spécialisées, langages) dans cette salve : ne pas en déduire « Rose bat Rust
partout ». Le self-hosting reste PARTIEL. Constat daté, pas consigne.
