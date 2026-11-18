# Auto-hébergement G4 — RIR texte → ELF aarch64 natif, écrit en Rose

Date : 18/11/2026. Périmètre : **`compilateur/emetteur_elf.rose` uniquement**
(point fixe `compilateur/emetteur.rose` intact, aucun `.rs`, `Cargo.toml`,
`PLAN.md` modifié, aucun commit).

## Ce qui est PROUVÉ

`emetteur_elf.rose` (2786 → 3026 lignes, md5 `afeffb3b…`) possède une voie
G4 : si `argument(0)` est un dump RIR texte (`rose rir prog.rose`), il écrit
**directement sur stdout** (`ecrire_bytes("/dev/stdout", …)`, sans aucun `>>`)
un ELF64 `ET_EXEC` aarch64 autonome. Aucun linker, libc, rustc, clang ni GMP.

Chaîne exacte mesurée :

```
cat > /tmp/p47.rose <<'EOF'
var x = 47
>> x
EOF
./rose/rose rir /tmp/p47.rose > /tmp/p47.rir
./rose/rose compilateur/emetteur_elf.rose /tmp/p47.rir > /tmp/test.elf
chmod +x /tmp/test.elf && /tmp/test.elf; echo $?     # → 47
```

RIR d'entrée réel (`rose rir`) :

```
rir
fonction $principal
  retour vide
  n_slots 1
  noms [0]x
  globaux 0
  ops
    0000 affect slot 0 = entier 47
    0001 afficher_abstrait slot 0
```

Sortie validée par trois instruments indépendants :

- `file /tmp/test.elf` → `ELF 64-bit LSB executable, ARM aarch64, version 1
  (SYSV), statically linked, no section header` ;
- `readelf -h` → `Type: EXEC`, `Machine: AArch64`, `Entry point 0x400078`,
  1 programme header ; `readelf -l` → un seul `LOAD`, `0x0 → 0x400000`,
  `R E`, align `0x1000` ;
- exécution native sur GX10 aarch64 → code de retour attendu.

Sous-ensemble couvert : `$principal` seul, `affect slot N = entier K`, puis
`retour_abstrait`/`afficher_abstrait` d'un `entier K` ou d'un `slot N`. Le
code machine émis est le stub minimal `mov x0,#K ; mov x8,#93 ; svc #0`
(132 octets : 64 de header + 56 de phdr + 12 de code), avec `K = code % 256`
(copie conforme de la convention syscalls de `natif_emetteur.rs`/`link_natif.rs`,
mais réécrite en Rose, sans appel à ces modules).

**Balayage de parité** : 12 valeurs (0,1,7,42,47,49,99,127,128,200,255,256)
→ 12 ELF **octet pour octet identiques** à une référence Python indépendante,
et 12 codes de retour corrects (256 → 0). Aucun écart.

**Refus explicite** : RIR contenant `app `, `appel`, `saut`, `(bin` ou une
seconde fonction → stderr `emetteur_elf G4 : RIR hors sous-ensemble …`,
`rc=2`, **0 octet** sur stdout. Aucun binaire silencieusement faux.

**Non-régression** : la voie historique (source Rose → ELF qui affiche le
résultat et en fait son code de retour) fonctionne toujours (`>> 5+3` →
ELF de 4132 octets, `file` valide, exécution → `8`). Deux bogues latents
du fichier d'origine, qui l'empêchaient de démarrer du tout, ont été corrigés
au passage : `retour` nu interdit par Rose (2 occurrences) et `"\177ELF"`
qui produisait `5c 31 37 37` au lieu de `7f` (Rose n'a pas d'échappement
octal) → `caractere(127) + "ELF"`.

## Ce qui RESTE (porte G4 NON atteinte)

Ne pas conclure « Rose sans Rust » : ce jalon ne fait que **synthétiser un
stub de sortie** à partir d'un RIR déjà calculé. Il ne remplace **ni** M3.1
(`rose/src/natif_emetteur.rs`, abaissement général RIR → code machine) **ni**
M3.2 (`rose/src/link_natif.rs`, writer ELF/PE complet avec table de slots,
trampoline, symboles et sections).

1. **Abaissement RIR général en Rose** : `bin`, `app`, `saut`, `saut_si`,
   `affect slot N = (bin …)`, `allouer`, `lire`, indexation, globaux. Aujourd'hui
   refusés (rc=2), pas compilés.
2. **Fonctions / appels** : il manque le stub `_start` de `link_natif.rs`
   (réservation `n_slots*8` sur la pile, `base=sp`, `tramp`, `bl`) et les
   trampolines d'appel, donc les frames et l'ABI AAPCS64.
3. **Runtime intégré** : `eval_fallback` / bigints sans GMP. Le stub de
   `link_natif.rs` rend 0 et refuse explicitement les programmes à runtime ;
   la porte honest est la même ici.
4. **Sortie PE64 / Windows** : non couverte (le fichier historique sait écrire
   un ELF x86_64, pas un PE).
5. **Structure ELF** : `e_shoff = 0`, aucune section (exécutable mais `readelf
   -S` vide). Pas de table de symboles, pas de `.data`/`.rodata`.
6. **Valeurs de retour > 255** : tronquées (le noyau n'expose de toute façon
   que 8 bits de `exit`). Les entiers exacts / bigints ne sont pas des codes
   de retour.

## Porte honnête

✅ **ATTEINT** : un compiler écrit intégralement en Rose lit du RIR texte et
produit un **vrai binaire ELF aarch64 qui s'exécute**, vérifié par `file`,
`readelf`, comparaison octet par octet et lancement réel (12/12).

❌ **NON ATTEINT** : G4 complet (compilateur Rose autonome produisant
n'importe quel programme natif, avec fonctions, appels et runtime, sans le
moteur Rust). Le moteur `rose/rose` reste nécessaire pour **exécuter
l'émetteur lui-même** ; le code machine général reste dans `natif_emetteur.rs`.

Prochaine étape naturelle : porter en Rose le stub `link_natif::stub_depart`
et l'abaissement des ops `bin`/`affect`/`retour_abstrait` en instructions
AAPCS64, puis seulement alors brancher les appels via un runtime embarqué.
