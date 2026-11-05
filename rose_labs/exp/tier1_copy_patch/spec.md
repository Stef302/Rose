# Copy-and-Patch Tier 1 — Spécification

## Principe

Le copy-and-patch est une technique de compilation où des "patchs" de code
machine précompilés (templates) sont copiés en mémoire et assemblés avec des
relocations minimales pour produire du code exécutable.

Avantages :
- Compilation < 1 ms pour 1000 instructions
- Pas d'analyse complexe, pas d'allocation de registres
- Idéal pour le développement itératif et le rechargement à chaud

## Templates machine x86_64

Chaque template est une séquence d'octets de code machine avec des "trous"
(relocations) pour les adresses et immédiats.

### Templates disponibles

| Nom | Opcode x86_64 | Description |
|-----|---------------|-------------|
| `MOV_IMM_REG` | `48 C7 C7 imm32` | mov rdi, imm32 |
| `ADD_REG_REG` | `48 01 f8` | add rax, rdi |
| `SUB_REG_REG` | `48 29 f8` | sub rax, rdi |
| `IMUL_REG_REG` | `48 0f af f8` | imul rax, rdi |
| `CMP_REG_REG` | `48 39 f8` | cmp rax, rdi |
| `JMP_REL32` | `e9 rel32` | jmp rel32 |
| `JE_REL32` | `74 rel8` / `0f 84 rel32` | je rel |
| `RET` | `c3` | ret |
| `PUSH_RBP` | `55` | push rbp |
| `MOV_RBP_RSP` | `48 89 e5` | mov rbp, rsp |
| `POP_RBP` | `5d` | pop rbp |
| `LEA_RIP_REL32` | `48 8d 05 rel32` | lea rax, [rip+rel32] |

## Relocation

Chaque trou dans un template est identifié par :
- `offset` : position dans le template (octets depuis le début)
- `size` : taille en octets (4 pour imm32, 1 pour rel8)
- `type` : `IMM32`, `REL8`, `REL32`
- `target` : symbole ou adresse cible

## API

```rust
struct Template {
    bytes: Vec<u8>,
    relocations: Vec<Reloc>,
}

struct Reloc {
    offset: usize,
    size: usize,
    reloc_type: RelocType,
    target: String,
}

enum RelocType {
    Imm32,    // entier signé 32 bits
    Rel8,     // saut relatif 8 bits
    Rel32,    // saut relatif 32 bits
}

struct Patch {
    code: Vec<u8>,
}

impl Patch {
    fn new(template: &Template) -> Self { ... }
    fn patch(&mut self, offset: usize, value: u64) { ... }
    fn finalize(&self) -> Vec<u8> { ... }
}
```

## Utilisation prévue dans Rose

Le compilateur tier 1 sera appelé pour le mode "dev itératif" (-O1) :
1. Parser le source Rose en AST
2. Pour chaque expression, copier le template approprié
3. Appliquer les relocations (adresses de variables, cibles de saut)
4. Exécuter le code machine généré

## Statut

Squelette uniquement. L'implémentation complète reste à faire.
