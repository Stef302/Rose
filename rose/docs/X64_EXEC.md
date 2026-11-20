# X64_EXEC — exécution RÉELLE d'un binaire x86-64 produit par Rose

**Constat daté : 2026-11-20 (mesures 08:24–08:39 UTC ; horloge machine
`2026-11-20T09:39+01:00`).** Aucun commit, aucune publication. Tous les
artefacts sont sous `/tmp/opencode/x64_1_agents/`.

## Résumé en une phrase

L'émetteur `.rose` produit un **PE32+ Windows** qui ne peut PAS être exécuté
ici (preuve ci-dessous) ; une **voie ELF64 x86-64 statique** a été ajoutée à
`rose/src/natif_emetteur.rs`, et les binaires qu'elle produit **s'exécutent
vraiment** sous `qemu-x86_64-static` **et** `box64` : `f(7)` sort `42`,
`fib(10)` sort `55`, et un balayage `fib(0..12)` donne 13/13 résultats
conformes.

## 1. Environnement mesuré

```
uname -m                         → aarch64
/usr/bin/qemu-x86_64-static      → ELF aarch64 static-pie, qemu 8.2.2
/usr/local/bin/box64             → ELF aarch64 dynamique, Box64 arm64 v0.4.5
/home/clausake/.local/bin/rose   → Rose 0.1.5 (instalé : pas de commande `rir`)
rose/target/release/rose         → reconstruit 2026-11-20 09:39 +01:00 (commande `rir` OK)
```

⚠ Découverte de mesure : **qemu-user refuse silencieusement un ELF qui n'a pas
le bit exécutable** (sortie 1, sans message). Tous les ELF ci-dessous sont
créés en `0755` par la fonction ajoutée (`set_permissions(0o755)`), et le
contrôle a été refait après `chmod +x` sur le témoin.

## 2. Ce que l'émetteur Rose `.rose` émet (PE32+)

Source : `compilateur/emetteur_x64.rose` (non modifié). Programme d'entrée
`/tmp/opencode/x64_1_agents/prog2.rose` :

```rose
fn f($x: entier) -> entier { retour $x * 6 }
fn fib($n: entier) -> entier {
  si ($n < 2) { retour $n }
  retour fib($n - 1) + fib($n - 2)
}
f(7)
fib(10)
```

Commandes et sorties réelles :

```
$ rose/target/release/rose rir prog2.rose > prog2.rir
$ rose/target/release/rose compilateur/emetteur_x64.rose prog2.rir > prog2.exe
$ ls -l prog2.exe ; file prog2.exe
3072 octets
PE32+ executable (console) x86-64, for MS Windows, 3 sections
$ xxd -g1 -l 16 prog2.exe
4d 5a 00 00 ...                      ← signature MZ
```

Note : la première tentative avec `afficher(...)` dans `$principal` échoue
avec `X64 : appel non résolu : afficher` — l'émetteur PE ne résout que les
fonctions déclarées. Le programme ci-dessus n'utilise donc que `f` et `fib`.
C'est une limite de l'émetteur PE, pas du correctif.

### 2.1 Blocage technique du PE (preuves)

```
$ qemu-x86_64-static ./prog2.exe ; echo $?
1                                    (aucun message)
$ strace -f -o pe.strace qemu-x86_64-static ./prog2.exe
read(3, "MZ\0\0\0...", 64) = 64
read(3, "MZ\0\0\0...", 1024) = 1024
exit_group(1)
$ box64 ./prog2.exe
[BOX64] Error: Reading elf header of /tmp/opencode/x64_1_agents/prog2.exe,
        Try to launch natively instead
./prog2.exe: 1: Syntax error: word unexpected (expecting ")")
rc=2
```

Diagnostic : qemu-user et box64 sont des **chargeurs ELF Linux** ; ils lisent
`\x7fELF` et rejettent `MZ`. Il n'existe aucun chemin d'exécution PE x86-64
sur cette machine (Wine local = aarch64 sans WoW64). **Aucun succès
structurel n'est présenté comme un succès d'exécution.**

## 3. Voie ELF64 x86-64 ajoutée (correctif)

Fichier modifié (unique) : `rose/src/natif_emetteur.rs`, **+111 lignes**,
fonction publique `ecrire_elf_x86_64_autonome(code, slots_initiaux, attendu,
chemin)` (`#[cfg(unix)]`). `elf_linker.rs` n'a pas été modifié : il n'est
déclaré que dans le binaire (`main.rs:198`), pas dans `lib.rs`, donc
inatteignable depuis un harnais externe ; la nouvelle fonction est dans
`natif_emetteur.rs`, qui est `pub mod` dans `lib.rs:106`.

L'ELF produit est un **ET_EXEC statique** (`e_machine=62`, `e_entry=0x400100`,
un `PT_LOAD` RWX, base `0x400000`, code à `0x100`, slots à `0x1000`, 8192
octets). Le `_start` écrit en dur :

```
movabs rax, <valeur> ; mov [base+8*slot], rax     (pour chaque slot initial)
mov rdi, base        ; rdi = pointeur des slots (signature i64 f(base, tramp))
mov rsi, 0           ; rsi = trampoline NUL (aucun AppelRir/fallback)
call code_emis
cmp rax, attendu
je  ok
mov edi,255 ; mov eax,60 ; syscall               → exit 255 si écart
ok: mov rdi,rax ; mov eax,60 ; syscall           → exit(resultat)
```

Le code machine invoqué est bien celui de `emettre(..., Architecture::X86_64)`.
Octets réels extraits de `f7.elf` (prologue + `imul`), sans désassemblage
(outil local sans backend x86) :

```
55 48 89 e5 53 41 54 48 89 fb 49 89 f4   push rbp; mov rbp,rsp; push rbx;
                                          push r12; mov rbx,rdi; mov r12,rsi
48 8b 43 00 48 89 43 10                   mov rax,[rbx]; mov [rbx+0x10],rax
48 c7 c0 06 00 00 00                      mov rax,6
48 8b 43 10 48 8b 4b 18                   mov rax,[rbx+0x10]; mov rcx,[rbx+0x18]
48 0f af c1                               imul rax,rcx
41 5c 5b 5d c3                            pop r12; pop rbx; pop rbp; ret
```

Harnais de production (hors dépôt) : `/tmp/opencode/x64_1_agents/harness/`
(dépendance `rose = { path = .../rose }`, uniquement l'API publique). Il
construit les corps RIR correspondant **exactement** au dump
`rose/target/release/rose rir` des sources Rose ci-dessus. `fib` y est écrit
sous forme **itérative** (boucle `SautSiFauxAbstrait`/`Saut`/`Affect`) pour ne
pas dépendre de la trampoline de récursion : 0 appel runtime, 0 fallback.

⚠ Limite honnête : l'émetteur machine ne couvre pas `afficher`. Le résultat
est donc observable par le **code de sortie du processus**, pas par une ligne
affichée. C'est une exécution réelle (syscall `exit` tracé par qemu), pas une
simulation.

## 4. Exécution réelle — commandes et sorties

```
$ ./harness/target/release/x64_harness
f: 62 octets émis
fib: 172 octets émis, 0 appels runtime, 0 fallbacks
référence: f(7)=42 fib(10)=55

$ file f7.elf fib10.elf
ELF 64-bit LSB executable, x86-64, version 1 (SYSV), statically linked

$ qemu-x86_64-static ./f7.elf    ; echo $?      → 42
$ qemu-x86_64-static ./fib10.elf ; echo $?      → 55
$ box64 ./f7.elf                 ; echo $?      → 42
$ box64 ./fib10.elf              ; echo $?      → 55
$ qemu-x86_64-static -strace ./fib10.elf
1565733 exit(55)
```

Balayage (un ELF par n, 13 valeurs) et contrôle négatif :

```
fib(0)=0  fib(1)=1  fib(2)=1  fib(3)=2  fib(4)=3  fib(5)=5  fib(6)=8
fib(7)=13 fib(8)=21 fib(9)=34 fib(10)=55 fib(11)=89 fib(12)=144   → 13/13 OK
qemu-x86_64-static ./f7_bad.elf ; echo $?   → 255  (attendu volontaire 43)
```

Le contrôle négatif prouve que le code de sortie vient bien de la comparaison
`rax == attendu` dans le `_start`, et non d'une constante.

## 5. Validation après correctif (tout mesuré le 2026-11-20)

|Contrôle|Commande|Résultat réel|
|---|---|---|
|Vérificateur Rose|`./rose/rose verif compilateur/emetteur_x64.rose`|`vérifié`, rc=0|
|cargo test release|`cargo test --release` (dans `rose/`)|**431 passés, 0 échec**, 6 ignorés (223+207+6+1 tests)|
|Jardin|`./rose/rose jardins/tests/premiers.rose`|`── jardin/premiers : 20 tests, 0 échec ──`|
|fib25|`./rose/rose /tmp/.../fib25.rose`|`75025`|
|Point fixe ×2|`md5(rose rir compilateur/emetteur.rose)` et `md5(rose compilateur/emetteur.rose compilateur/emetteur.rose)`|`b3ed9262dd54776a6dbd07bc12368d0e` **×2**, MD5 égaux|
|Banc 9 domaines|copie locale de `bench_sp1.py`, binaire `rose/target/release/rose` reconstruit, `NSAMP=9 PIN_CPU=19`|9 domaines × 9 échantillons = **81/81 valeurs vérifiées**, rc=0, aucune régression|
|x86-64 cross|qemu + box64 sur `f7.elf`/`fib10.elf`|42 / 55 sur les deux|

Le banc 9 domaines a été lancé avec le binaire **reconstruit après** le
correctif (`rose/target/release/rose`, 4 932 512 o, 09:39 +01:00). Sorties :
`/tmp/opencode/x64_1_agents/bench9.log`, `bench_multi_results.csv` (387
lignes), `bench_multi_summary.json`. Le runtime **interprété** n'a pas été
modifié (aucun fichier de l'interpréteur touché), et les 81 valeurs ont été
revérifiées à chaque échantillon par le banc lui-même.

`rose verif` a aussi été demandé sur les fichiers modifiés : le seul fichier
modifié est un `.rs`, pour lequel `rose verif` n'a pas de grammaire ; il est
validé par `cargo test --release` ci-dessus. Le fichier `.rose` du périmètre
(`compilateur/emetteur_x64.rose`) reste vérifié.

## 6. État des lieux factuel

- **Émis** : un PE32+ x86-64 structurellement valide (3072 o, `MZ`, Machine
  `0x8664`) par l'émetteur `.rose` ; deux ELF64 x86-64 statiques autonames
  (8192 o chacun) par la nouvelle voie Rust.
- **Exécuté** : les deux ELF, sous qemu-x86_64-static **et** box64, avec les
  codes de sortie attendus ; 13 valeurs fib + 1 contrôle négatif.
- **Non exécutable ici** : le PE, pour cause de format (preuves §2.1).
- **Correctif** : `rose/src/natif_emetteur.rs`, +111 lignes, aucune suppression,
  aucun changement de l'émetteur aarch64, ni du PE, ni du runtime interprété.
- **Reste** : rien dans le périmètre. Le `git status` du dépôt montre aussi des
  modifications appartenant à d'autres agents (`compilateur/emetteur_elf.rose`,
  `rose/src/i128x.rs`, `rose/src/interp.rs`, `rose/src/jit.rs`) : non touchées.
