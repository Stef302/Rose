# Banderole de mise à jour — source de vérité sur le site Rose

État : **21/11/2026**. Dépôt `Stef302/Rose`, HEAD `7c750f5` au début du chantier.
Agent L5. **Aucun commit, aucune publication.** Fichiers possédés modifiés :
`rose/src/velopack.rs`, `rose/src/setup_core.rs`, `rose/src/setup_gui.rs`.
Nouveau fichier : ce document. `docs/index.html`, `docs/telechargements/**`,
`interp.rs`, `lib.rs`, `Cargo.toml`, `PLAN.md` et les docs des autres agents
n'ont **pas** été touchés.

**Constat de traçabilité :** ce sous-agent n'a exécuté aucun `git commit` ni
aucune publication. Un commit `e8a4bfd` intitulé « Launcher : banderole update
+ manifeste site, zéro polling (L5) », signé `Stef302` le 21/11/2026 à 21:42:39,
est apparu dans l'historique pendant le chantier ; il contient l'essentiel de
ces modifications. Les deux corrections postérieures à ce commit
(`est_legacy` et le message hors ligne dans `construire_catalogue`) restent
**non commitées** dans l'arbre de travail.

---

## 1. Ce qui est livré

1. **Banderole dès le premier écran** (`Vue::MisesAJour` **et** `Vue::Bienvenue`) :
   `Nouvelle version disponible : 0.1.7 → 0.1.72 — [VOIR LES VERSIONS]`.
   Le bouton ouvre le sélecteur. Les deux versions viennent de données réelles.
2. **Source de vérité = le site Rose**, pas l'API GitHub. Repli sur l'API GitHub
   existante (`lister_versions()`) si le manifeste du site est absent ou invalide.
3. **Aucun polling** : une seule lecture au moment où l'utilisateur **ouvre le
   launcher**. Un seul thread, qui meurt après sa réponse. Pas de timer, pas
   d'icône cachée, pas de service, pas de boucle.

---

## 2. Priorité des sources (ordre exact)

| Ordre | Source | URL / fichier | Quand |
|---|---|---|---|
| 1 | **Site Rose** (manifeste) | `https://stef302.github.io/Rose/telechargements/releases.json` | cache frais d'abord, sinon **UNE** requête HTTP |
| 2 | **API GitHub** (repli existant) | `https://api.github.com/repos/Stef302/Rose/releases?per_page=100` | uniquement si le site échoue |
| 3 | **État local** | `versions-cache.json` / `site-releases-cache.json` / dossiers `app-<v>` | si les deux sources sont injoignables |

Le repli n'est jamais silencieux : le message de l'UI porte la cause des deux
échecs, et le sélecteur affiche la dernière liste réellement reçue avec sa date.

Format attendu du manifeste (convenu avec SITE1 ; le parseur tolère aussi
`{"releases":[…]}` et les alias `title`, `published_at`, `name`) :

```json
[{"version":"0.1.72","tag":"v0.1.72","date":"2026-11-21T09:00:00Z",
  "titre":"Correctifs","assets":[{"nom":"Rose-0.1.72-full.nupkg",
  "url":"https://…","sha256":"…"}],"changelog":"…"}]
```

**Constat daté :** au moment de l'écriture, `curl -m 5` sur l'URL du site a
rendu **HTTP 404** (manifeste pas encore publié par SITE1, qui travaille en
parallèle). Le code est prêt ; la banderole s'activera dès publication, sans
recompiler le launcher. Aujourd'hui, le repli GitHub répond et ne montre rien
d'alarmant car la version installée (0.1.7) est plus récente que les releases
GitHub connues.

### Sécurité des données affichées

- Une version n'est acceptée que si elle commence par un chiffre et ne contient
  que des caractères de version ; un titre marketing est rejeté.
- Un paquet n'est `installable` que si son **SHA-256 est annoncé** dans `assets`.
- Un `changelog` contenant le texte `"assets"` n'est pas confondu avec le
  tableau d'assets (analyseur qui respecte les chaînes JSON).
- La banderole n'apparaît **que** si `comparer_versions(disponible, installée)
  == Greater`. `0.1.72 > 0.1.7` est couvert par le comparateur existant.
- La version installée est lue depuis `current` / `app-<v>` (`version_installee`).
  **Aucune version n'est codée en dur en production** ; `CARGO_PKG_VERSION`
  n'est jamais utilisé pour cette décision.

---

## 3. Une seule requête, aucun polling — preuve

- `CreateWindowExW` appelle `lancer_verification_maj(hwnd, false)` **une fois**,
  uniquement si le mode « Auto » est actif (mode « Manuel » : cache seul ;
  mode « Désactivé » : aucun appel).
- Cette fonction ne contient **qu'un seul** `std::thread::spawn`, aucune boucle,
  aucun `sleep`, aucun `SetTimer`. Le thread appelle `verifier_bandeau()` puis
  poste `WM_APP_MAJ` et se termine. Le fil de messages n'est jamais bloqué.
- `requeter_json` borne curl à `--connect-timeout 2 --max-time 3` **plus** un
  garde-fou `try_wait`/`kill` à 3 000 ms (`DELAI_RESEAU_MS`).
- Cache local : `site-releases-cache.json` (réponse brute + horodatage), TTL
  **300 s** — exactement le mécanisme de `versions-cache.json` (fonctions
  `ecrire_cache_brut_dans` / `lire_cache_brut_dans` factorisées).
- Test structurel `verification_au_demarrage_sans_polling` relit le source de
  `setup_gui.rs` et échoue si un `loop`, un `sleep`, un `SetTimer` ou un second
  `spawn` apparaît dans cette fonction.

Le bouton **« Rafraîchir »** du sélecteur appelle
`launcher_catalogue_force()` → `lister_versions_source(true)` : le TTL est
ignoré, le manifeste site est relu et le cache réécrit. La banderole est alors
recalculée depuis cette même liste (aucun second appel réseau).

Variables de diagnostic (aucune n'est posée en production) :
`ROSE_LAUNCHER_SITE_MANIFESTE` (force la voie site, accepte `file://`) ;
`ROSE_LAUNCHER_API` / `ROSE_VELOPACK_URL` (demandent explicitement GitHub /
le manifeste Velopack, ce qui désactive la voie site pour les tests).

---

## 4. Mesures — temps jusqu'à la banderole (5 tirs)

Harnais : test `mesure_bandeau_cache_froid_et_chaud` (`cargo test --release
--lib mesure_bandeau_cache_froid_et_chaud -- --nocapture`), 5 tirs par état,
manifeste lu depuis un `file://` local dans un dossier temporaire (aucune
variable d'environnement touchée, aucun appel Internet). La partie réseau
réelle est bornée à 3 s ; ces mesures couvrent donc le chemin de lecture,
d'analyse, de cache et de comparaison — **pas** la latence du serveur GitHub
Pages, non publié au moment de la mesure.

Salves réellement exécutées (microsecondes, médiane au 3ᵉ tir) :

| Salve | Froid — 5 tirs | Médiane | Chaud — 5 tirs | Médiane |
|---|---|---:|---|---:|
| 1 | 40, 42, 45, 55, 203 | 45 | 13, 13, 13, 14, 19 | 13 |
| 2 | 40, 43, 44, 58, 104 | 44 | 13, 13, 14, 15, 19 | 14 |
| 3 | 36, 38, 40, 47, 87 | 40 | 13, 13, 13, 14, 18 | 13 |
| 4 | 39, 39, 43, 51, 87 | 43 | 13, 13, 13, 14, 18 | 13 |

Constat : cache chaud ~3,3× plus rapide que cache froid sur ce harnais local.
Le premier tir froid porte le coût de création du fichier de cache (203 µs puis
~90 µs) ; les tirs suivants se stabilisent à ~40 µs. Ces valeurs sont des
**constats datés**, pas des promesses : re-mesurer après tout changement de
format de manifeste ou de mécanisme de cache.

---

## 5. Validation réellement exécutée

| Contrôle | Résultat |
|---|---|
| `cargo build --release` | `Finished release [optimized]` — OK |
| `cargo test --release` | **529 passés, 0 échec** (265 lib + 249 bin + 6 + 8 + 1 ; 6 ignorés, tous réseau/installation réelle) |
| Tests `velopack` existants | verts, y compris `launcher_selection_et_cache` et `config_persistance_et_effet_au_demarrage` |
| Nouveaux tests L5 | **6** : parseur site, rejet JSON invalide, bandeau sans mensonge, cache/TTL, absence de polling, mesure |
| Jardin `jardins/tests/premiers.rose` | `20 tests, 0 échec` |
| `fib(25)` | `75025` |
| Point fixe `b3ed9262` ×2 | `rose rir compilateur/emetteur.rose` **et** `rose compilateur/emetteur.rose compilateur/emetteur.rose` → même md5 `b3ed9262dd54776a6dbd07bc12368d0e` |
| Type-check Windows | `cargo check --release --target x86_64-pc-windows-gnu --bin rose-setup` dans un arbre miroir sans `build.rs` (le `build.rs` du dépôt exige `windows.h`, indisponible ici) : **exit 0**, `setup_gui.rs` bien compilé |
| Bandeau hors ligne | aucun bandeau si les deux sources échouent ; message explicite « sources injoignables » + liste locale datée si un cache existe ; test `bandeau_ne_ment_jamais` couvre les cas « à jour » / « version installée inconnue » |
| Version codée en dur en production | aucune dans les trois fichiers hors fixtures de tests (vérifié par recherche des littéraux `0.1.x`) |
| 9 domaines 81/81 | **non re-exécuté ici** : le harnais externe (`/tmp/opencode/cmp1_agents/bench_cmp1.py`) n'existe plus sur cette machine ; aucun fichier de moteur/JIT modifié par ce chantier |

---

## 6. Comportement hors ligne (résumé)

- Site 404 + GitHub inaccessible : `etat.bandeau = None`, aucune banderole,
  sélecteur alimenté par le dernier cache réel ou par les dossiers `app-*`,
  avec un message qui nomme les deux sources en échec.
- Mode « Manuel » : aucun appel réseau au démarrage ; la banderole peut venir
  du cache frais uniquement.
- Mode « Désactivé » : aucun appel, message explicite dans la ligne de statut.
