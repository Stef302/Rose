# HORDE CLI — la ligne de commande Rose portée en Rose (poste J6 §11)

État : **23/11/2026**. Worktree exclusif `/tmp/opencode/horde_cli`, HEAD de
départ `101b48d`. Aucun commit, aucune publication, aucun fichier modifié hors
des trois fichiers possédés :

| Fichier | Rôle | Lignes / octets |
|---|---|---|
| `compilateur/cli_rose.rose` | couche CLI Rose + composite auto-hébergé (`rose_natif.rose` repris à l'octet près) | 15 782 l. / 603 614 o |
| `compilateur/tests_cli_rose.rose` | banc Rose de 20 assertions sur le dispatch réel | 86 l. / 21 285 o |
| `rose/docs/HORDE_CLI.md` | ce document | — |

Garde-fou utilisé sur toute commande longue :
`bash scripts/garde_fou_mem.sh 12000000 <commande>`.

---

## 1. Architecture — ce qui est réellement porté

Le moteur Rust n'est **pas appelé** dans la chaîne exécutée. `cli_rose.rose`
est un source Rose autonome :

1. **Couche CLI écrite en Rose** (en tête du fichier) : textes exacts de
   l'oracle `rose/rose` 0.1.73, table de dispatch `cli_aide_pour`, gestion des
   erreurs sur `/dev/stderr`, et le dispatch `version` / `help` / `info` /
   `verif` / `rir` / `transpile … --cible wasm` / `bc`.
2. **`cli_rir_du(source)`** : corps exact de `em_source_rir()` du composite,
   rendu paramétrable (le composite original lit `argument(0)` ; une CLI à
   mot-clé en tête a besoin de passer la source explicitement). 627 lignes,
   **0 globale ajoutée**.
3. **Composite `compilateur/rose_natif.rose` repris intégralement** :
   `verifier_source` (J1), `sw_module` / `sb_module` (J5), `emettre_elf_print`
   et le backend ELF/PE (compilation autonome), `g5_*`. Le dispatch CLI court-
   circuite le main du composite par `quitter(0)` ; les formes `source en tête`
   retombent sur ce main (compilation ELF et `--cible wasm|bc` natives).

Chaîne de production :

```bash
cd /tmp/opencode/horde_cli
# 1. vérification statique (moteur Rust).
rose/rose verif compilateur/cli_rose.rose
# 2. compilation SANS Rust, par le compilateur auto-hébergé C0 déjà construit.
C0=/tmp/opencode/C0_ext.elf
$C0 compilateur/cli_rose.rose > /tmp/opencode/cli_rose.elf
chmod +x /tmp/opencode/cli_rose.elf
# 3. exécution : /tmp/opencode/cli_rose.elf <commande> [args…]
```

`rose/rose` n'existe pas sous ce nom dans le worktree : le binaire de
référence utilisé est `docs/telechargements/rose-aarch64-linux` (ELF aarch64
statique, 5 456 976 o), alias `R` ci-dessous.

---

## 2. Commandes couvertes — taux réels

Taux calculé sur les **22 comparaisons exécutées** du pilote
(`/tmp/opencode/horde_cli_pilot.sh`, reproduit en §5) :

| # | Commande CLI | Équivalent `rose/rose` | Comparaison | Résultat |
|---|---|---|---|---|
| 1 | `version` | `version` | stdout + rc | voir §5 |
| 2 | `--version` | `--version` | stdout + rc | voir §5 |
| 3 | `help` | `help` | stdout + rc | voir §5 |
| 4 | `--help` | `--help` | stdout + rc | voir §5 |
| 5 | *(aucun argument)* | *(aucun argument)* | stderr + rc=1 | voir §5 |
| 6 | `info` | `info` | stdout + rc | voir §5 |
| 7–15 | `help comp`, `methode`, `run`, `jit`, `interprete`, `transpile`, `bc`, `rvm`, `aot` | identiques | stdout + rc | voir §5 |
| 16 | `help inconnu` | identique | stderr (2 lignes) + rc=1 | voir §5 |
| 17 | `verif` *(sans source)* | identique | stderr + rc=1 | voir §5 |
| 18 | `rir` *(sans source)* | identique | stderr + rc=1 | voir §5 |
| 19 | `verif prog.rose` | identique | stdout `vérifié` + rc | voir §5 |
| 20 | `rir prog.rose` | identique | dump RIR, **octet pour octet** | voir §5 |
| 21 | `transpile prog.rose --cible wasm -o f.wasm` | identique | message + **fichier .wasm octet pour octet** | voir §5 |
| 22 | `bc prog.rose -o f.rosebc` | identique | message + **fichier .rosebc octet pour octet** | voir §5 |
| +1 | `prog.rose sortie.elf` (forme source en tête) | `C0 prog.rose sortie.elf` | **ELF octet pour octet** | voir §5 |

**Taux** : renseigné après exécution du pilote en §5.

Capacités du composite effectivement atteintes par la CLI : **4 / 4**
(vérification `verifier_source`, compilation ELF/PE `emettre_elf_print`,
sérialiseurs `sw_module` / `sb_module`, RIR `cli_rir_du`).

---

## 3. Contrainte des 512 globales — mesurée

Mesure par `R rir compilateur/cli_rose.rose`, table `noms` du `$principal` :

| Source | `n_slots` | borne |
|---|---|---|
| `compilateur/rose_natif.rose` (composite nu) | 410 | 512 |
| `compilateur/cli_rose.rose` (composite + CLI) | **416** | 512 |

+6 globales (`cli0`, `hcmd`, `haide`, `fmt`, `octets`, `dst`, `j` partiellement
mutualisés) : **416 < 512**, aucune dérogation.

---

## 4. Validation exécutée

| Contrôle | Commande | Résultat |
|---|---|---|
| Vérification statique CLI | `R verif compilateur/cli_rose.rose` | `vérifié`, rc=0 |
| Vérification statique tests | `R verif compilateur/tests_cli_rose.rose` | `vérifié`, rc=0 |
| Banc Rose | `R compilateur/tests_cli_rose.rose` | `TESTS CLI : 20 OK, 0 ECHEC`, rc=0 |
| Point fixe (voie 1) | `R rir compilateur/emetteur.rose` | 263 520 o, md5 `b3ed9262dd54776a6dbd07bc12368d0e` |
| Point fixe (voie 2) | `R compilateur/emetteur.rose compilateur/emetteur.rose` | 263 520 o, md5 `b3ed9262dd54776a6dbd07bc12368d0e` |
| Point fixe (`cmp`) | `cmp pf1 pf2` | identique, stderr 0 o |
| C0 → ELF | `C0 compilateur/cli_rose.rose > cli.elf` | voir §5 |
| Comparaison 22 commandes | pilote §5 | voir §5 |

Le script `scripts/validation_finale.sh` du dépôt exige un binaire
`rose/target/release/rose` construit par cargo, absent de ce worktree. Les
contrôles qu'il porte et qui sont accessibles ici (verif, point fixe ×2, WASM,
RVM, HOST-1) sont rejoués avec le binaire de référence ci-dessus ; aucun
n'est remplacé par une affirmation.

---

## 5. Pilote de comparaison (reproductible)

```bash
ROOT=/tmp/opencode/horde_cli
R=$ROOT/docs/telechargements/rose-aarch64-linux
CLI=/tmp/opencode/cli_rose.elf
C0=/tmp/opencode/C0_ext.elf
W=/tmp/opencode/horde_cli_pilot; rm -rf $W; mkdir -p $W
P=$W/prog.rose; printf '>> 7*6\n' > $P
# ... voir /tmp/opencode/horde_cli_pilot.sh (boucle cmp sur stdout/stderr/rc,
#     puis cmp des fichiers .wasm/.rosebc/ELF produits)
```

Résultat : **voir la ligne « RESULTAT PILOTE » imprimée par le pilote**,
recopiée ici après exécution.

---

## 6. Ce qui manque encore (périmètre non couvert)

Commandes de `rose help` **non portées** dans la CLI auto-hébergée :

| Commande | Raison |
|---|---|
| `methode`, `run`, `jit`, `interprete`, `console`, `repl` | le composite est un compilateur mono-shot ; aucun interpréteur/REPL câblé (J3/J6 §5). |
| `latex`, `tokens`, `ast` | les émetteurs texte correspondants ne sont pas des `fn` exposées par le composite. |
| `llvm`, `aot`, `orc`, `link` | chaîne LLVM absente ; le helper AOT n'est pas publié (CLI_CLAIR.md §3). |
| `rvm` | la VM bytecode Rust n'est pas portée en Rose. |
| `wasm <lib.wasm>`, `update`, `assoc`, `debug-lenteur` | hors composite (introspection, réseau, Windows, profilage). |
| `comp <source>` (mot-clé en tête) | la compilation ELF passe par le main du composite, qui attend la **source en `argument(0)`** ; forme `comp` non disponible. |
| PE (`rose_natif_pe.rose`) | le CLI n'invoque que le composite ELF de ce worktree. |

Détail de fidélité : les 6 écarts connus du vérificateur Rose vs oracle Rust
(J1 §4) sont reproduits à l'identique par `verifier_source` : ce ne sont pas
des régressions de la couche CLI.
