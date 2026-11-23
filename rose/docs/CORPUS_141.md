# CORPUS_141 — parité finale du moteur self-hosté (23/11/2026)

Constat **daté et mesuré**, pas une promesse. Worktree exclusif
`/tmp/opencode/rel_rel2` (HEAD `cdd57af`), sans commit ni push.

## 1. Résultat

| État | Avant | Après |
|---|---:|---:|
| CONFORMES | 133 | **138** |
| DIVERGENTS | 5 | **0** |
| BLOQUES | 3 | **3** |

Les 3 BLOQUES restants sont `t55_liste_imbriquee`, `t106_hof_param`,
`t107_hof_var` : les deux moteurs refusent, mais le self-hosté écrit son
diagnostic sur **stdout** avec un `|` préfixé (`|Rose : erreur — …`) alors
que Rust écrit sur **stderr** sans préfixe. Ce sont des refus des deux
côtés, pas des erreurs sur un programme valide.

Les 5 anciens DIVERGENTS sont maintenant **octet pour octet conformes**
(rc + stdout + stderr) :

| Test | Avant | Après |
|---|---|---|
| `t104_surcharge_arith` | self accepte (`3`/`0`), Rust refuse | les deux refusent, même message |
| `t105_surcharge_arite` | self accepte (`3`/`3`), Rust refuse | les deux refusent, même message |
| `t83_lire_texte` | self `rc=0`/vide, Rust `rc=1` | les deux `rc=1`, même message |
| `t84_ecrire_texte` | self `rc=0`/vide, Rust `rc=1` | les deux `rc=1`, même message |
| `t86_ecrire_puis_lire` | self `rc=0`/`0`, Rust `rc=1` | les deux `rc=1`, même message |

Un 6e écart observé pendant la mesure, `t999_critique_g5`
(`chaine(flottant("2"))` → `2/1` au lieu de `2`), a aussi été corrigé.
`t82_argument_hors` (deux refus identiques) est compté CONFORME par le
harnais ci-dessous ; les harnais qui classent « deux rc≠0 » en BLOQUE
le comptent à part (cf. §5).

## 2. Chaîne de mesure

```bash
# moteur self-hosté à partir du source possédé, avec le C0 existant
scripts/garde_fou_mem.sh 12000000 timeout 900 \
  /tmp/opencode/C0_h.elf compilateur/moteur_complet.rose > /tmp/opencode/moteur_fix.elf

# harnais (hors dépôt, dans /tmp/opencode/rel_rel2/harnais_corpus141.sh)
bash scripts/garde_fou_mem.sh 12000000 bash harnais_corpus141.sh
```

Oracle : `ROSE_SANS_JIT=1 /home/clausake/Stefan/ROSE/rose/rose`
(md5 `7614a534b9411917ef9cd49a834014a8`). Moteur compilé :
ELF aarch64 `1910156` octets, md5 `26d0774dbe1f1a83147e95bd48ece6b2`.

Le harnais exécute les **141** sources `tests_selfhost/*.rose` deux fois,
compare `rc`, stdout et stderr avec `cmp`, puis classe :

- **CONFORME** : `rc`, stdout **et** stderr identiques (y compris deux refus
  identiques — c'est le cas de t104/t105/t82 et, sans fixtures, de
  t83/t84/t86) ;
- **BLOQUE** : les deux refusent (`rc≠0`) mais un flux diffère ;
- **DIVERGENT** : tout le reste (désaccord de rc, ou sorties différentes
  avec au moins un succès).

Fixtures externes (créées si absentes) :
`/tmp/opencode/sh9_agents/battery/input.txt`,
`…/critique_entree.txt`, `…/out.txt`, `…/out2.txt`,
`/tmp/aotwork/entree.txt`.

Sortie réelle du harnais (avec et sans fixtures) :

```
tests=141  CONFORMES=138  DIVERGENTS=0  BLOQUES=3
BLOQUES : t106_hof_param t107_hof_var t55_liste_imbriquee
DIVERGENTS :
```

Le harnais historique `scripts/verifier_parite_selfhost.sh` (qui classe
*tout* « deux rc≠0 » en BLOQUE et ne compare que stdout) rend, sur le même
ELF corrigé, `135 CONFORMES / 0 DIVERGENT / 6 BLOQUES` — les 3 BLOQUES
supplémentaires étant `t104/t105/t82`, dont les trois flux sont pourtant
identiques. Aucune divergence n'y subsiste.

## 3. Correctifs appliqués

1. **`compilateur/lecteur_source.rose` — `def_fn()`** (et son miroir
   préfixé `lsrc_def_fn()` dans `compilateur/moteur_complet.rose`) :
   balayage de `$fnoms` avant enregistrement ; en cas de doublon, écriture
   directe de
   `Rose : erreur — [Ligne L, Colonne 0] Fonction 'f' définie deux fois`
   sur `/dev/stderr` puis `quitter(1)`. L'écriture directe (au lieu du canal
   `ERR|` du lecteur, qui sort sur stdout avec un `|` préfixé) est
   indispensable à l'égalité octet pour octet avec le diagnostic Rust.

2. **`compilateur/interpreteur_rir.rose` et `compilateur/moteur_complet.rose`
   — `call_prim()`** : le backend G5 ne signale pas d'erreur E/S (une
   ouverture impossible rend `""`, une écriture impossible est silencieuse).
   Les trois primitives lèvent maintenant le diagnostic Rust et quittent
   avec `rc=1` :
   - `lire_texte` : résultat vide sur un chemin hors `/dev/` →
     `lecture du fichier impossible (<chemin>)` ;
   - `ecrire_texte` : relecture différente de la donnée →
     `écriture du fichier impossible (<chemin>)` ;
   - `ecrire_bytes` / `ecrire_binaire` : même contrôle →
     `écriture d'octets impossible (<chemin>)`.
   Les chemins `/dev/` sont exclus (flux, pas fichiers) : `t85` écrit sur
   `/dev/stdout` et `t999` sur `/dev/null`.

3. **`compilateur/moteur_complet.rose` — `call_prim()`/`chaine`** : on
   retire le dénominateur `/1` des flottants entiers, comme le fait déjà
   `interpreteur_rir.rose` (`chaine(flottant("2"))` → `2`). Correctif t999.

## 4. Banc de régression

`compilateur/tests_corr_final.rose` (nouveau) est vérifié par les deux
moteurs et rend `tests_corr_final : 10 OK, 0 ECHEC` des deux côtés,
octet pour octet. Il reproduit en Rose pur : le balayage de détection des
doublons, les trois formulations d'erreur E/S, la détection d'échec par
relecture et le formatage `chaine(flottant("2"))`. Il n'appelle pas le vrai
lecteur sur un source à doublon : celui-ci appelle désormais `quitter(1)`
(parité Rust) et terminerait le banc. Son comportement réel est couvert par
t104/t105 du corpus.

## 5. Contrôles

- `rose verif` : `compilateur/lecteur_source.rose`,
  `compilateur/moteur_complet.rose`, `compilateur/tests_corr_final.rose` →
  `vérifié`, `rc=0`. (`interpreteur_rir.rose` ne se vérifie pas seul : module
  destiné à être concaténé avec `unicode_rose.rose`; son `rc=1` standalone
  est identique avant et après le patch.)
- Marqueurs préservés dans `moteur_complet.rose` après édition :
  `bi_pow` ×6, `mc_args` ×2, `rep_run` ×2, `cli_aide_pour` ×2, `cands` ×5.
- `garde_fou_mem.sh 12000000` utilisé sur l'émission C0 (aucun dépassement).
- Aucun fichier interdit modifié : `emetteur.rose`, `emetteur_elf.rose`,
  `rose/src/**` inchangés (vérifié par `git diff --name-only`).
