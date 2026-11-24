# Surveillance — émission du PE complet corrigé et banc d'exécution Wine

Horodatages en UTC. Toutes les valeurs ci-dessous ont été **mesurées** ; aucune
n'est déduite d'une spécification ou d'un journal antérieur.

## 1. Période de surveillance

| Instant (UTC) | Fait observé |
|---|---|
| 11:43:35 | PID `776384` vivant (`R`, 15 min 41 s), `rose_final.exe` = 0 o, `rose_final.err` = 0 o, `rose_final.rc` absent |
| 11:50:23 | `rose_final.rc` apparaît : `RC=0 size=1417216` ; PID `776384` terminé |
| 11:50:33 | Contrôles statiques du PE effectués |
| 11:50:44 → 11:51:04 | Banc Wine exécuté (20 s), `rc=0` pour le script lui-même |

La commande d'émission surveillée (vérifiée via `/proc/776382/cmdline` et
`/proc/776384/fd/1`) était :

```
cd /tmp/opencode/fix_pe_out && \
bash /tmp/opencode/fix_pe_exec/scripts/garde_fou_mem.sh 12000000 \
  ./emetteur22.elf /tmp/opencode/payload_c0_out/moteur_complet.rir \
  > rose_final.exe 2> rose_final.err ; \
echo "RC=$? size=$(stat -c%s rose_final.exe)" > rose_final.rc
```

Durée totale d'émission : 11:27:54 → 11:50:23 UTC, soit **~22 min 29 s**
(conforme aux ~1357 s de l'émission précédente).

## 2. Résultat de l'émission

- **RC = 0**, taille = **1 417 216 octets**, stderr = **0 octet**.
- `file rose_final.exe` → `PE32+ executable (console) x86-64, for MS Windows, 3 sections`.
- `md5sum` → `322023a94481df997eb309c60cad719c  rose_final.exe`.
- Sections : `.text` VMA `0x140001000` (0x159413 o), `.data` `0x14015b000`,
  `.pdata` `0x14015c000` ; image base `0x140000000`.
- `llvm-objdump -d --section=.text` → 215 278 lignes, 0 octet d'erreur,
  **0 marqueur `(bad)`/`invalid`**, et **0 appel direct vers `0x140001000`**
  (contre 4 256 sur le `rose.exe` d'origine). Le défaut d'appels réentrants
  identifié par l'agent correctif est donc bien éliminé dans le PE complet.

## 3. Banc Wine — protocole et résultats

Le banc `scripts/tester_pe_windows.sh` a été exécuté **depuis une copie isolée**
(`/tmp/opencode/fin_bench/scripts/`) afin de ne pas créer d'artefacts dans le
dépôt, avec :

```
WINE_ROOT=/tmp/opencode/fix_pe_exec/.preuve_exec_win/wine-amd64
PE=/tmp/opencode/fix_pe_out/rose_final.exe
ROSE_BIN=/home/clausake/Stefan/ROSE/bench/perf/algo/bin/rose
```

Journal complet : `/tmp/opencode/fin_bench/bench.log`.

### Témoin positif

`cmd.exe /c echo CONTROL_OK` sous Wine-amd64/qemu → **rc = 0**, stdout
`CONTROL_OK\r\n` (12 o). Le banc exécute bien un vrai PE x86-64.

Contrôle complémentaire : `mini8_e22.exe` (micro-PE de l'agent correctif)
→ **rc = 0**, stdout `19`. Le banc n'est donc pas en cause.

### Les 5 programmes

Référence : binaire natif aarch64 produit par `rose aot`. Colonne « PE » :
`rose_final.exe` sous Wine-amd64/qemu.

| Programme | `rose aot` (natif) | `rose.exe` sous Wine — rc | `rose.exe` — stdout | `rose.exe` — stderr | Valeur attendue | Conforme ? |
|---|---|---|---|---|---|---|
| `>> 2 + 2` | rc=0, binaire `4` | **29** | 35 o | 761 o | `4` | **non** |
| `>> 6 * 7` | rc=0, binaire `42` | **29** | 35 o | 236 o | `42` | **non** |
| `>> 100 / 4` | rc=0, binaire `25` | **29** | 35 o | 236 o | `25` | **non** |
| `>> 2 ^ 10` | rc=0, binaire `1024` | **29** | 35 o | 236 o | `1024` | **non** |
| `>> 1/3 + 1/6` | rc=0, binaire `0.5` | **29** | 35 o | 236 o | `0.5` | **non** |

Aucune des cinq valeurs n'a été produite par le PE.

Contenu réel des sorties (identique pour les 5 programmes, seul le PID change) :

- stdout (35 o) : `Can't attach process 0020: error 5` — message du débogueur
  Wine **après** la faute, pas une sortie du programme ;
- stderr (extrait constant) :

```
wine: Unhandled illegal instruction at address 0000000140157AAD (thread 0024), starting debugger...
wine: could not open working directory L"unix\\...\\", starting in the Windows directory.
```

Les avertissements `Wine cannot find the FreeType font library` sont des
fixmes sans rapport (ils apparaissent aussi avec le témoin `cmd.exe`).

## 4. Cause exacte de l'échec observé

- L'exception est **déterministe** : même adresse `0x140157AAD` pour les 5
  programmes, et **aussi quand `rose_final.exe` est lancé sans aucun argument**
  (rc=29, même message). Ce n'est donc pas lié au chemin du fichier `.rose`
  passé en argument ni à la conversion de chemin Win32.
- À l'adresse fautive, les octets du fichier sont `46 8a 14 09`, que
  `llvm-objdump` désassemble en `movb (%rcx,%r9), %r10b` : instruction
  x86-64 **valide**, dans une boucle de copie d'une aide à la concaténation
  de chaînes (fonction commencée en `0x14015799c`). Le désassemblage complet
  ne contient aucun opcode invalide et aucun `ud2` à proximité de cette
  adresse (les 7 `ud2` du binaire sont à `0x140039418`, `0x1400a687d`,
  `0x1400a8114`, `0x1400a8e88`, `0x1400aa11e`, `0x1400aa134`, `0x140138712`).
- Le signal est donc un **SIGILL levé à l'exécution par Wine/qemu**, avant
  toute écriture de la moindre valeur : ce n'est pas un opcode statiquement
  invalide dans `.text`, et ce n'est pas non plus l'ancien appel réentrant
  vers `0x140001000` (devenu 0). La cause profonde de ce SIGILL n'est pas
  établie par les seules mesures disponibles ; les faits mesurés sont le
  code `rc=29`, le message Wine ci-dessus, l'adresse et les octets
  désassemblés à cette adresse.
- Pour information, le PE complet précédent `rose_fix.exe` échoue
  différemment sous le même banc (`rc=1`, stdout et stderr vides) : le
  correctif a changé le mode de défaillance mais n'a pas rendu le PE
  complet exécutable.

## 5. Verdict

**Non : `rose.exe` ne s'exécute toujours pas correctement sous Wine.**

Ce qui est acquis : émission terminée sans erreur (`RC=0`), PE32+ x86-64
valide structurellement, 0 appel vers `0x140001000`, banc Wine fonctionnel
(témoin + micro-PE). Ce qui échoue encore : le PE complet lève une exception
`illegal instruction` déterministe en `0x140157AAD`, avec `rc=29`, pour les
5 programmes comme sans argument, avant toute sortie utile. Aucune des
valeurs de référence `4`, `42`, `25`, `1024`, `0.5` n'est produite.

Aucun code source n'a été modifié, aucun émetteur relancé, aucun commit ni
push effectué lors de cette surveillance.
