# Bilan de la vague 28 — repousser les limites (19/11/2026)

## Ce qui a été livré

| Demande | Livraison | État |
|---|---|---|
| Continuer à battre Rust (général puis cas par cas) | Aucune régression de perf ; bench re-joué (voir §2) | ✅ |
| Délocaliser ~40 % de la stdlib vers les jardins | 40/112 primitives = **35,7 %** (30,0 % en familles dédupliquées), 3 jardins + tests | ✅ ( honnête : pas 40 %) |
| Convertisseur tous langages | C, C++, **Rust**, **WAT** convertis et appelés depuis Rose ; CLI préservée + `--allow-imports` | ✅ |
| Accepter un wasm **avec imports** | 10 hôtes (`env.log/ecrire`, WASI `fd_write`/`fd_read`/`proc_exit`/`random_get` + alias), mémoire importée, rejet propre des autres | ✅ |
| CPython n'était pas demandé | Non réintroduit ; limites `.py`/`.go`/`.js` documentées (`Python.h`, runtime/GC, Emscripten) | ✅ |
| Clic droit `.rose` → exécuter / shell Rose, « toujours ouvrir avec » | Module `assoc_windows.rs` + commande `rose assoc` réellement câblée, 9 entrées registre, patch Velopack | ✅ (à tester sur Windows réel) |
| Jardin GUI comme tkinter, sans Rust/Python | `jardins/gui.rose` : 23 fonctions, 15 publiques, rendu HTML autonome | ✅ (limite : navigateur hôte, pas de fenêtre OS) |
| Rust entièrement self-hosted ? | Verdict **PARTIEL** : 50 % des étapes du compilateur, 0 % de reconstruction du binaire sans rustc | ⚠ documenté |
| RVM ultra complète / JVM | `rosevm.rs` 1086 → 2981 lignes, classes/méthodes/exceptions, 25 tests, commandes `bc`/`rvm` câblées | ✅ |
| Transpilation | Rose → JS/Python/C, round-trip `diff -u` vide, commande `rose transpile` câblée | ✅ |
| Bench JIT / AOT / interpréteur / RVM | `bench/backends/**` en Rose, 2 horloges, écarts et backends cassés documentés | ✅ |

**12 commits poussés** de `2b57be7` à `b3bef02`, tous sur `Stef302/Rose`.

## 1 — Perf après tous les ajouts (salve 19/11, 9 échantillons, c19)

| Domaine | Rose | Rust | Ratio |
|---|---:|---:|---:|
| fib(35) | 11,625 ms | 12,741 ms | **0,912 ✅** |
| Boucle 1M | 0,261 ms | 0,259 ms | 1,006 ≈ |
| Premiers 50k | 0,854 ms | 1,002 ms | **0,852 ✅** |
| Factorielle 5000 | 1,408 ms | 6,535 ms | **0,215 ✅** |
| Chaîne 20k | 0,917 ms | 0,014 ms | 63,7 |
| Liste 100k | 0,359 ms | 0,184 ms | 1,95 |
| Table 3000 | 0,097 ms | 0,094 ms | 1,034 |
| E/S 10 Mo | 6,015 ms | 5,894 ms | 1,021 |

**3 victoires nettes + 3 domaines au niveau**, aucun ajout de la vague 28 n'a
dégradé le moteur. La délocalisation est même **plus lente** qu'une primitive
Rust (factorielle 13 → 18 ms, `compter_premiers` 5 → 46 ms) — elle vise la
légèreté/indépendance, pas la vitesse.

## 2 — Validation finale

- `cargo test --release` : **204 + 188 + 6 + 1 tests, 0 échec** (avant câblage
  RVM : 1 test échouait par concurrence sur le tampon de sortie global ; corrigé
  par un verrou local dans les tests `rosevm`).
- Jardin `20/20`, `verif-tous` **7 jardins valides**, fib(25) `75025`.
- Point fixe M4.5 `bdbef63681608a0b22a5637539e9d738` ×2.
- Oracles M4 : lexeur **162/0**, analyseur **162/0** (corrigé de 160/2),
  vérificateur **157/5**, émetteur **157/5** — voir `ORACLES_FIDELITE_RESTANTE.md`.
- `rug` absent, version `0.1.5`, HEAD = distant `b3bef02`.

## 3 — Limites et vrais bugs trouvés (non masqués)

1. **Oracles 157/5** : vérificateur/émetteur Rose ne savent pas traiter les
   paramètres implicites (`?`), `type_de`, `len` et le typage dynamique utilisés
   par 5 fichiers de jardins. Aucun n'échoue à l'exécution (`rose verif` Rust
   répond « vérifié »). Tentative de correction sur copie isolée documentée,
   non appliquée car elle n'améliorerait pas les compteurs et risquerait une
   divergence sémantique.
2. **Backends cassés** (découverts par `BENCH_BACKENDS.md`) : `ROSE_NO_RIR=1`
   renvoie des résultats faux ; `ROSE_NATIF=1 ROSE_SANS_JIT=1` produit un
   `temps()` aberrant (~1,8 × 10¹⁸ ns) ; l'émetteur ELF auto-hébergé refuse le
   RIR du banc ; la JVM Rose n'a pas de JIT depuis `.rosebc`.
3. **Wasm** : `call_indirect`, `start`, SIMD et threads restent hors support ;
   `fd_read` est un stub, `random_get` non cryptographique, `proc_exit` devient
   une erreur Rose. Rust std génère 14 `call_indirect` → refusé.
4. **Windows** : rien n'a été testé sur une machine Windows réelle (clic droit,
   icône, élévation `--system`, cycle Velopack). Le patch est vérifié statiquement
   et compilé sur une copie, pas sur le produit final.
5. **GUI** : aucune fenêtre OS ; le navigateur est l'hôte du rendu HTML.
6. **Self-hosting** : `rose/rose` nécessite toujours `cargo build` ; le mot-clé
   `var` a été ajouté au front-end Rose (correctif appliqué, AST 162/0), mais
   G2/G3/G4 restent à faire.

Constat daté, pas consigne. Re-mesurer après tout changement de moteur.
