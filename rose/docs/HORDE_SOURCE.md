# HORDE_SOURCE — lecteur de source auto-hébergé (J6.5 / J6 §11)

**Constat daté : 23/11/2026.** Worktree `/tmp/opencode/horde_src`, HEAD
`101b48d25a85a3a4a5be80232af9dda7bff037f1`. Binaire oracle
`/home/clausake/Stefan/ROSE/rose/rose` (Rose 0.1.73, md5
`7614a534b9411917ef9cd49a834014a8`). Aucun commit, aucun push.

Fichiers possédés (les seuls écrits) :

- `compilateur/lecteur_source.rose` (nouveau, 162 965 o, 4 305 lignes) ;
- `compilateur/tests_lecteur_source.rose` (nouveau) ;
- `rose/docs/HORDE_SOURCE.md` (ce document).

## 1. Ce qui manquait au composite

Lecture de `compilateur/lexeur.rose` (451 l.), `compilateur/analyseur.rose`
(1 281 l.), `rose/src/front.rs` (3 604 l., lecture seule) et
`compilateur/emetteur.rose` (4 070 l.).

Le composite `compilateur/rose_natif.rose` embarque **déjà** le front-end :
lexage, descente récursive, repli, vérification, émission RIR ; ses 79
fonctions viennent de `emetteur.rose`, qui est un miroir Rose de `front.rs`.
`analyseur.rose` produit un AST textuel (`rose ast`) et `lexeur.rose` un flux
`ligne:colonne type texte` : ce sont des outils de *dump*, pas une API de
lecteur. Il manquait donc quatre choses, et seulement elles :

| Manque | État dans `emetteur.rose` | Ajout dans `lecteur_source.rose` |
|---|---|---|
| imports | `lire_texte($chemin)` brut, dépôt du texte importé dans le flux (l. 3521-3525) : **relatif au CWD**, pas au fichier importateur | `ls_charger()` récursif, chemins résolus tels quels puis à côté du fichier importateur (`front.rs:2996-3012`), cycles coupés, garde de 32 niveaux |
| erreurs | `signaler()` = première erreur gagnante ; aucune erreur d'import | `ERR|` + diagnostic exact ; registre de sources pour diagnostiquer un import/racine absent |
| chemins relatifs | aucun | `ls_norm`, `ls_dossier`, `ls_jointure`, `ls_est_absolu`, `ls_cherche` |
| encodage | chaînes indexées par points de code côté Rust | identique côté G5 (mesuré, §4) ; aucun transcodage nécessaire |

## 2. API et contrat

```
lire_programme(chemin)                            -> chaine
lire_programme_reg(chemin, noms, sources)         -> chaine
```

Réponse : `"OK|" + dump RIR` (octet pour octet identique à `rose rir`) ou
`"ERR|" + diagnostic` (une ligne, première erreur). Le registre est une paire
de listes de chaînes **rembourrées à l'index 0** (`[""]` puis `joindre`) :
`noms[i] -> sources[i]`. Avec registre non vide, aucun accès disque pour les
imports et la racine absente est diagnostiquée exactement ; sans registre,
`lire_texte` lit le disque. L'exécution en ligne de commande imprime la
réponse brute : `rose compilateur/lecteur_source.rose <fichier.rose>`.

Le front-end de `emetteur.rose` est réutilisé **intégralement** (lexage,
analyse, repli, vérification, émission) ; il est enfermé dans une fonction,
comme dans le composite généré, et muni des optimisations G5 déjà validées
par `scripts/generer_rose_natif.py::injecte_front_opt` (sinon l'émission C0
dépassait 12 Gio). Aucune ligne de `emetteur.rose`, `front.rs`, `lexeur.rose`
ou `analyseur.rose` n'a été modifiée.

## 3. Chiffres réels

Toutes les commandes ci-dessous ont été exécutées dans le worktree, sous
`bash scripts/garde_fou_mem.sh 12000000 …` pour les longues.

### 3.1 Chaîne de bootstrap

| Contrôle | Résultat mesuré |
|---|---|
| `rose verif compilateur/lecteur_source.rose` | `vérifié`, rc=0 |
| `rose verif compilateur/tests_lecteur_source.rose` | `vérifié`, rc=0 |
| `rose rir compilateur/emetteur.rose` | 263 520 o, md5 `b3ed9262dd54776a6dbd07bc12368d0e` (**point fixe**) |
| C0 émis depuis ce worktree | 714 024 o, md5 `745577eac4b15e0d80dd5b6ccbfe2ca1`, 45 s, ELF aarch64 |
| `selfhost_total.sh --full` (C0 réutilisé) | a PASS, b PASS, c PASS, d PASS (`cmp` octet pour octet), e PASS (`P0 == P1`, sha256 `a2bac7…`) ; f = diagnostic attendu, g ignoré, h PARTIEL (état de référence, aucune régression) |
| C0 (front-end → RIR, ELF) sur `lecteur_source.rose` | 274 930 o, **octet pour octet identique** à `rose rir lecteur_source.rose` |
| P0 (RIR → ELF, ELF) sur ce RIR | 730 916 o, 25 s, md5 `7c195a681942eb257db2fa2a5347db76`, ELF aarch64 |
| Lecteur ELF issu de C0+P0 | exécute sans moteur Rust ; `tests_selfhost/t01_entiers.rose` → RIR identique à l'oracle |
| `validation_finale.sh` | **0 échec** (tests 314+327/0, point fixe, AOT 75/75, JIT 25/25, WASM 92 o, RVM 317811, AZ 31/31, HOST-1 317811) |

Le binaire `rose/target/release/rose` a été construit par l'étape 1 de
`validation_finale.sh` ; `rose/rose` est un lien symbolique de build, retiré
après la mesure (aucun fichier supplémentaire ne subsiste dans le worktree).
Salves intermédiaires : `tests rc=101` deux fois, pour deux causes
**externes aux fichiers livrés** — (a) `opt_wall_final::programme_minimal_jit_reste_conforme`
écrit dans `/tmp/rose-opt-wall-final.rose`, chemin partagé écrasé par une
salve concurrente ; (b) `opt_jit_listes` a SIGSEGV 1 fois sur 3 en isolement
(binaire static-pie, déjà documenté dans la skill Rose / CI). La salve finale
verte a été lancée avec isolation `TMPDIR=/tmp/opencode/ls_tmp` et les
drapeaux CI `RUSTFLAGS="" CARGO_PROFILE_RELEASE_STRIP=none` :
`RESULTAT: 0 échec(s)`.

### 3.2 Parité RIR et diagnostics

| Corpus | Voie | Résultat |
|---|---|---|
| `tests_selfhost/*.rose` (141 sources) | lecteur natif (moteur Rust, logique 100 % Rose) | **134/141 = 95,0 %** identiques à `rose rir` (RIR ou diagnostic) |
| `rose-cpp/tests/verifs/*.rose` (53 rejets) | lecteur natif | **53/53 = 100 %** diagnostics exacts |
| banc des 20 sources | lecteur natif | **20/20** identiques (`tests_lecteur_source.rose`, `=== BILAN OK=19 ERR=1` : 19 RIR + 1 rejet) |
| banc des 20 sources | lecteur ELF C0+P0 | **18/20** (2 écarts G5 détaillés §5) |
| les mêmes 141 sources | lecteur ELF C0+P0 | 107/141 = 75,9 % (écarts G5 de calcul, §5) |

Les 20 sources du banc couvrent entiers, booléens, Unicode, rationnels,
flottants, listes, tables, arithmétique, précédence, grands nombres, chaînes,
échappements, boucles, récursion et un rejet exact (`t55_liste_imbriquee`).

### 3.3 Imports

Fixtures créées hors dépôt, dans `/tmp/opencode/ls_imp/`, lancées depuis
`/tmp` (CWD différent du répertoire source) :

| Cas | Oracle | Lecteur natif | Lecteur ELF |
|---|---|---|---|
| `main.rose` importe `sub/util.rose` | RIR avec `double` | identique | identique |
| double import du même fichier (dedup) | RIR sans doublon | identique | identique |
| import avec instructions de tête dans le fichier importé | RIR avec `afficher_abstrait entier 999` | identique | identique |
| import absent, via registre | `importer : fichier introuvable 'absent.rose' (depuis …)` | exact | exact |
| racine absente, via registre | `importer : impossible de lire '…' (No such file or directory (os error 2))` | exact | exact |
| cycle `a -> b -> a` | `Fonction 'fa' inconnue` | RIR produit (cycle coupé, **1 écart**) | idem |

Sans registre, le lecteur natif laisse remonter l'erreur du moteur hôte
(`lecture du fichier impossible (…)`) et le lecteur G5 rend un programme vide
pour une racine absente : voir §5.

### 3.4 Encodage

`>> longueur("é")`, `>> ordre("é"[0])`, `>> caractere(233)` rendent
`1 / 233 / é` dans le moteur Rust **et** dans un ELF G5 émis par C0. Les
chaînes sont donc indexées en points de code des deux côtés : aucun
transcodage n'a été ajouté, et les sources Unicode (`t04_chaine_unicode`,
`g5b501`…`g5b505`) figurent dans les 134 parités native.

## 4. Ce que le lecteur corrige réellement

- **Chemins relatifs** : `emetteur.rose` lancé depuis `/tmp` sur
  `/tmp/opencode/ls_imp/main.rose` rend
  `Rose : erreur — lecture du fichier impossible (sub/util.rose)` ;
  le lecteur rend le RIR attendu, depuis le même CWD. Mesuré dans les deux
  voies (natif et ELF).
- **Cycles** : la récursion est bornée (32 niveaux) et chaque chemin
  normalisé n'est chargé qu'une fois ; `emetteur.rose` bouclait jusqu'à 32
  passes et dupliquait les définitions.
- **Diagnostics d'import** : deux messages exacts reproduits via registre,
  dans le moteur Rust et dans l'ELF G5.
- **Injectabilité** : `lire_programme_reg` fonctionne sans système de
  fichiers (WASM/navigateur) ; `lire_programme` conserve la lecture disque.

## 5. Écarts mesurés (non masqués)

1. **Sept divergences héritées du front-end Rose** (`emetteur.rose`, non
   modifié) sur 141 : `g5b606_rat_litteraux` et `t242_rat_litteraux_grands`
   (`conversion vers entier impossible` interne, ligne 2568 du lecteur),
   `t14_type` (`Fonction 'type' inconnue`), `t91_prng` (`Fonction 'prng'
   inconnue`), `t230_neg_grands_litteraux` (`-0` rendu `0`),
   `t104_surcharge_arith` et `t105_surcharge_arite` (diagnostic de
   redéfinition rendu à la ligne 4 au lieu de la ligne 2). La même salve
   `emetteur.rose` seul donnait **exactement les mêmes 7 écarts** : le
   lecteur n'en introduit aucun.
2. **Cycle d'import** : 0/1 sur le diagnostic (le lecteur coupe le cycle et
   produit un RIR, l'oracle rejette avec `Fonction 'fa' inconnue`).
3. **Racine absente sans registre** : le sous-ensemble auto-hébergé n'a ni
   `essayer`/`attraper` (non abaissables en RIR, `rir.rs:683`) ni primitive
   d'existence de fichier (`front.rs`, table des primitives) ; le lecteur ne
   peut donc pas intercepter l'erreur de `lire_texte`. Avec registre, le
   diagnostic est exact (§3.3).
4. **G5 — calcul du front-end** : 2 des 20 sources du banc divergent dans
   l'ELF : `t09_flottant` (`afficher_abstrait flottant 1` au lieu de `1.5`)
   et `t20_add` (`288230376151711751` au lieu de `7`). Ce sont les limites G5
   déjà documentées (`selfhost_total.sh --inventaire` : flottants, grands
   entiers > 64 bits), pas un défaut du lecteur : le RIR produit par C0 est
   bit-identique à celui du moteur Rust.
5. **Ordre de résolution des imports** : l'oracle essaie le chemin tel quel
   (CWD) **puis** à côté du fichier importateur ; le lecteur, sans registre,
   privilégie le second quand le chemin est relatif. Lorsque les deux
   candidats existent avec des contenus différents, le choix peut différer
   (cas non présent dans le corpus mesuré).

## 6. Reproduction

```bash
cd /tmp/opencode/horde_src
R=/home/clausake/Stefan/ROSE/rose/rose
$R verif compilateur/lecteur_source.rose
$R compilateur/tests_lecteur_source.rose            # 20 sources + 2 REG|
# bootstrap complet depuis CE worktree
bash scripts/garde_fou_mem.sh 12000000 $R rir compilateur/emetteur.rose > self.rir
bash scripts/garde_fou_mem.sh 12000000 $R compilateur/emetteur_elf.rose self.rir > C0.elf
bash scripts/garde_fou_mem.sh 12000000 ./C0.elf compilateur/lecteur_source.rose > lecteur.rir
bash scripts/garde_fou_mem.sh 12000000 ./C0.elf compilateur/emetteur.rose > em.rir
bash scripts/garde_fou_mem.sh 12000000 $R compilateur/emetteur_elf.rose em.rir > P0.elf
bash scripts/garde_fou_mem.sh 12000000 ./P0.elf lecteur.rir > lecteur.elf
chmod +x lecteur.elf
./lecteur.elf tests_selfhost/t01_entiers.rose       # OK|rir …
# validation globale (isoler TMPDIR : d'autres agents écrivent /tmp/rose-opt-wall-final.rose)
TMPDIR=/tmp/opencode/ls_tmp RUSTFLAGS="" CARGO_PROFILE_RELEASE_STRIP=none \
  bash scripts/validation_finale.sh                 # RESULTAT: 0 échec(s)
```

Les fichiers de mesure intermédiaires sont dans `/tmp/opencode/ls_*` ; aucun
artefact n'a été écrit hors de `/tmp` et des trois fichiers possédés.
