# CLI claire — bannière neutre, `comp` autonome, AOT local

État : **21/11/2026**. Dépôt `Stef302/Rose`. Sous-agent CLI. **Aucun commit,
aucune publication, aucune modification hors fichiers possédés.**

Fichiers possédés modifiés : `rose/src/main.rs`, `rose/src/interp.rs`,
`rose/src/auto_llvm.rs`, `rose/src/cmd_llvm.rs`. Nouveau fichier : ce document.

---

## 1. Bannière / version / info : avant → après

| Surface | Avant | Après |
|---|---|---|
| `rose` sans argument (1ʳᵉ ligne de l'aide) | `Rose 0.1.7 — compilateur Rust + JIT Cranelift + LLVM ORC AOT (maths exactes BigInt natif, zéro GMP)` | `Rose 0.1.7 — langage de programmation en français (maths exactes, JIT, AOT, compilation autonome)` |
| `rose --version` | `Rose 0.1.7 — compilateur Rust + JIT Cranelift (sémantique exacte)` | `Rose 0.1.7 — langage de programmation en français (maths exactes, JIT, AOT, compilation autonome)` |
| `rose info` | `moteur : Rust natif aarch64 — JIT Cranelift + interpréteur exact`, `BigInt natif (num-bigint, … zéro GMP)` | `langage : Rose — langage de programmation en français`, `architecture : ARM 64 bits`, `maths exactes : entiers illimités, …`, `exécution : JIT à chaud + interpréteur intégré`, `compilation : exécutable autonome (\`rose comp\`), JIT et AOT optionnels` |

Aucune mention de l'outil ayant construit Rose, ni de `Cranelift`, ni de
`num-bigint`, ni de `GMP` dans ces trois surfaces. `JIT` et `AOT` sont
conservés : ce sont des concepts du produit.

Les trois lignes d'aide demandées (`comp` / `aot`+`orc` / `llvm`) sont
maintenant :

```
  rose comp prog.rose [-o sortie]   exécutable autonome : moteur + source collés, démarre immédiatement, aucun outil externe (~8,8 Mo) ; .exe ajouté automatiquement sur Windows
  rose aot prog.rose [-o sortie] [-O1|-O2|-O3]   binaire natif optimisé par compilation AOT LLVM ORC, sans JIT au runtime (optionnel) ; nécessite la chaîne LLVM (helper à côté de rose.exe ou installé). Alias de orc.
  rose orc prog.rose [-o sortie] [-O1|-O2|-O3] [--pgo-generate|--pgo-use] [--link]   idem (compilation AOT LLVM ORC en interne, optionnelle) ; nécessite la chaîne LLVM
  rose llvm prog.rose [-o sortie]   transpilation C puis clang/gcc (optionnel)
Note : `rose comp` suffit pour 99 % des usages (exécutable autonome). `aot`/`orc` et `llvm` sont optionnels et nécessitent des outils externes.
```

`LLVM` reste volontairement dans **les descriptions des commandes `aot`/`orc`/
`link`** : la consigne demande d'y dire explicitement ce qu'elles exigent. Il a
été retiré de la bannière, de `version()`, de `info()` et de tous les messages
d'erreur. Le contrôle demandé (`rose --help` sans `Rust`/`Cargo`/`cargo`) passe.

---

## 2. `rose comp` — extension `.exe`

`nom_sortie_comp(sortie, windows)` (dans `main.rs`) ajoute `.exe` **uniquement
sur Windows, uniquement si le suffixe manque** ; le drapeau est passé en
argument pour être testable sous Linux. La règle s'applique après le calcul du
nom par défaut **et** après un `-o` explicite, qui reste honoré.

| Plateforme | Commande | Fichier | Message |
|---|---|---|---|
| Linux | `rose comp main.rose` | `main` (0755) | `rose: main produit (exécutable autonome)` |
| Windows | `rose comp main.rose` | `main.exe` | `rose: main.exe produit (exécutable autonome)` |
| Windows | `rose comp main.rose -o livrable.exe` | `livrable.exe` (suffixe non doublé) | `rose: livrable.exe produit (exécutable autonome)` |
| Windows | `rose comp main.rose -o livrable` | `livrable.exe` | `rose: livrable.exe produit (exécutable autonome)` |

Le message est imprimé avec la variable `sortie` **après** normalization : le
`.exe` apparaît donc dans le message, comme demandé.

---

## 3. Helper AOT local — état réel

Audit réel effectué le 21/11/2026 :

```
GET https://api.github.com/repos/Stef302/Rose/releases/tags/v0.1.7
→ tag v0.1.7, 8 assets : releases.win.json, Rose-0.1.7-full.nupkg,
  rose-aarch64-linux, rose-language-0.1.7.vsix, rose-setup.exe,
  rose-shell.exe, rose-wasm.wasm, rose.exe
→ AUCUN asset rose-aot / rose-aot-x86_64-windows.exe
```

L'ancienne URL (`v5.10.0/rose-aot-x86_64-windows.exe`) et le miroir
`raw.githubusercontent.com/.../docs/telechargements/...` pointaient vers des
fichiers inexistants. Ils ont été **retirés** du code, avec la vérification
SHA-256 associée et le téléchargement `curl`/`wget`. `VERSION_HELPER` vaut
maintenant `env!("CARGO_PKG_VERSION")` (`0.1.7`) et ne sert plus qu'à nommer le
chemin du cache.

Ordre de recherche, **sans aucun accès réseau** :
1. à côté de l'exécutable courant ;
2. `~/.cache/rose/rose-aot-0.1.7` (ou `ROSE_CACHE`) ;
3. dans le `PATH`.

Un helper qui annonce lui-même « la chaîne AOT … n'est pas embarquée » est
ignoré (sonde `helper orc` sans programme, sans effet de bord) : un ancien
helper ne doit pas réinjecter son ancien message dans l'interface.

Message unique, identique pour `aot`, `orc` et `link` :

```
rose: chaîne AOT LLVM non disponible — utilisez `rose comp` (autonome) ou installez LLVM localement
```

Aucun message d'erreur ne parle de compilation du moteur. Le helper n'a **pas**
été publié : la consigne du sous-agent interdit toute publication, et la
release v0.1.7 n'a pas d'asset AOT, où que ce soit.

---

## 4. Vérifications réellement exécutées

| Contrôle | Résultat |
|---|---|
| `cargo build --release --bin rose` | `Finished release [optimized]`, 0 erreur (warnings préexistants uniquement) |
| `cargo test --release` | **552 passés, 0 échec, 6 ignorés** (lib 275, binaire `rose` 262, setup 0, shell 6, `grands_nombres` 8, `mul_cmp` 1, doc 0) |
| Nouveaux tests `tests_cli_clair` | 3/3 : suffixe `.exe` Windows simulé, neutralité de l'interface, bannière/aide |
| `rose --version`, `rose info`, `rose --help`, `rose` sans argument | 1ʳᵉ ligne neutre identique, sans `Rust`/`Cargo`/`cargo` |
| `grep -iE 'rust\|cargo' <(rose --help)` | aucun résultat (rc=1) |
| `rose aot`, `rose orc`, `rose link` | message clair unique, sans `cargo`, rc=1 |
| `rose comp /tmp/…/main.rose -o /tmp/…/sortie` (Linux) | fichier `sortie` 0755, 5 129 289 o, message `produit (exécutable autonome)`, exécution → `5` |
| Point fixe `b3ed9262` ×2 | `rose rir compilateur/emetteur.rose` **et** `rose compilateur/emetteur.rose compilateur/emetteur.rose` → `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o, stderr 0 octet des deux côtés (source `390335d2…`) |

---

## 5. Limites / hors périmètre

- Le helper AOT n'existe pas dans la release v0.1.7 : `aot`/`orc`/`link`
  restent indisponibles dans le binaire léger. C'est le repli demandé, pas un
  oubli. Un helper réellement construit avec la chaîne AOT et posé à côté de
  `rose.exe` sera trouvé et utilisé.
- Un ancien `target/release/rose-aot` (18/11, 1,2 Mo) traîne dans l'arbre de
  développement. Sans la sonde décrite §3, il interceptait `rose aot` et
  réimprimait l'ancien message avec `cargo build`. Il est désormais ignoré ;
  fichier non supprimé (hors fichiers possédés).
- `rose/src/setup_gui.rs:2101` contient encore, dans un texte de l'écran
  « Bienvenue », « interprète exact, JIT Cranelift, AOT LLVM et module WASM ».
  Ce fichier appartient à un autre agent et était modifié pendant ce chantier :
  **non touché**. Si l'exigence « aucune mention nulle part dans l'interface »
  couvre aussi le launcher, il faut corriger cette ligne côté propriétaire.
- Les commentaires source de `main.rs` / `interp.rs` citent encore
  `Cranelift`/`Rust` : ce sont des notes techniques internes, pas de
  l'interface utilisateur ; ils n'ont pas été réécrits.
