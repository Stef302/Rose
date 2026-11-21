/* RUNTIME NATIF AOT6 — x86-64 (SysV, Linux). Compiler-rt de Rose.
 * Signature : i64 routine(base: *mut i64, tramp: *const Trampoline)
 * rdi = base, rsi = tramp, résultat dans rax.
 * SORTIE 0x402000, TMP 0x403000, digits 0x404000.
 */

.text
.balign 16

/* ───────────────────────── $rt_hasard ───────────────────────── */
.globl rt_hasard
.balign 16
rt_hasard:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %rbx
    mov (%rbx), %r15               /* borne, lue avant d'écraser base[0] */
    movq $0, (%rbx)
    mov %rbx, %rdi
    mov $8, %esi
    xor %edx, %edx
    mov $318, %eax
    syscall
    mov (%rbx), %r9
    mov %rsp, %r10
    xor %r10, %r9
    movabs $0x9E3779B97F4A7C15, %r11
    add %r11, %r9
    mov %r9, (%rbx)
    mov %r9, %r12
    mov %r12, %r13
    shr $30, %r13
    xor %r13, %r12
    movabs $0xBF58476D1CE4E5B9, %r11
    imul %r11, %r12
    mov %r12, %r13
    shr $27, %r13
    xor %r13, %r12
    movabs $0x94D049BB133111EB, %r11
    imul %r11, %r12
    mov %r12, %r13
    shr $31, %r13
    xor %r13, %r12
    test %r15, %r15
    jz .Lh_zero
    lea -1(%r15), %r10
    mov %r15, %rax
    and %r10, %rax
    jz .Lh_p2
    mov %r12, %rax
    mul %r15
    mov %rdx, %rax
    jmp .Lh_fin
.Lh_p2:
    mov %r12, %rax
    and %r10, %rax
    jmp .Lh_fin
.Lh_zero:
    xor %eax, %eax
.Lh_fin:
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    pop %rbp
    ret

/* ───────────────────────── $rt_sortir ───────────────────────── */
.globl rt_sortir
.balign 16
rt_sortir:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    mov %rax, %r9                /* valeur rendue = code de retour */
    mov $0x402000, %r10
    mov (%r10), %r11
    mov $1, %edi
    lea 8(%r10), %rsi
    mov %r11, %rdx
    mov $1, %eax
    syscall
    mov %r9, %rax
    pop %rbx
    pop %rbp
    ret

/* ───────────────────────── $rt_lire_texte ──────────────────── */
.globl rt_lire_texte
.balign 16
rt_lire_texte:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    mov %rdi, %rbx
    /* AOT6 : lecture sur stdin (l'entrée standard), aucun prompt. */
    xor %edi, %edi
    mov $0x402008, %esi
    mov $4096, %edx
    xor %eax, %eax
    syscall
    sub $1, %rax
    jge .Ll_ok
    xor %eax, %eax
.Ll_ok:
    mov $0x402000, %r10
    mov %rax, (%r10)
    mov %r10, %rax
    pop %rbx
    pop %rbp
    ret

/* ───────────────────────── $rt_bigint_puissance ─────────────── */
.globl rt_bigint_puissance
.balign 16
rt_bigint_puissance:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    push %r12
    push %r13
    push %r14
    push %r15
    mov %rdi, %rbx
    mov (%rbx), %r12        /* a */
    mov 8(%rbx), %r13       /* b */
    mov $0x403000, %r10
    movq $1, (%r10)
    mov $1, %r14            /* nlimbs */
.Lbp_pow:
    test %r13, %r13
    jz .Lbp_dec
    xor %r15, %r15          /* carry */
    xor %rcx, %rcx          /* i */
.Lbp_mul:
    cmp %r14, %rcx
    jae .Lbp_mul_done
    mov (%r10,%rcx,8), %rax
    mul %r12                /* rdx:rax = limb * a */
    add %r15, %rax          /* + report */
    mov %rax, %rdx
    shr $32, %rdx           /* report = (limb*a + report) >> 32 */
    mov %eax, (%r10,%rcx,8) /* limbe = 32 bits bas */
    mov %rdx, %r15
    add $1, %rcx
    jmp .Lbp_mul
.Lbp_mul_done:
    test %r15, %r15
    jz .Lbp_next
    mov %r15, (%r10,%r14,8)
    add $1, %r14
.Lbp_next:
    sub $1, %r13
    jmp .Lbp_pow

.Lbp_dec:
    mov $0x404000, %rsi     /* tampon digits */
    xor %r8, %r8            /* ndigits */
.Lbp_outer:
    mov %r14, %rcx
    test %rcx, %rcx
    jz .Lbp_outer_done
.Lbp_zero:
    test %rcx, %rcx
    jz .Lbp_outer_done
    sub $1, %rcx
    mov (%r10,%rcx,8), %rax
    test %rax, %rax
    jz .Lbp_zero
    xor %r9, %r9            /* reste */
    mov %r14, %rcx
.Lbp_div:
    sub $1, %rcx
    mov (%r10,%rcx,8), %rax
    shl $32, %r9
    or %r9, %rax
    xor %edx, %edx
    mov $10, %r11d
    div %r11                /* rax = q, rdx = r */
    mov %rdx, %r9
    mov %rax, (%r10,%rcx,8)
    test %rcx, %rcx
    jnz .Lbp_div
    add $0x30, %r9d
    mov %r9b, (%rsi,%r8)
    add $1, %r8
    jmp .Lbp_outer
.Lbp_outer_done:
    /* trim zéros de tête (fin du tampon) */
.Lbp_trim:
    cmp $1, %r8
    jbe .Lbp_rev
    mov %r8, %rcx
    sub $1, %rcx
    cmpb $0x30, (%rsi,%rcx)
    jne .Lbp_rev
    sub $1, %r8
    jmp .Lbp_trim
.Lbp_rev:
    mov $0x402000, %r10
    mov %r8, (%r10)
    lea 8(%r10), %rdi
    xor %rcx, %rcx
.Lbp_revloop:
    cmp %r8, %rcx
    jae .Lbp_ret
    mov %r8, %rax
    sub %rcx, %rax
    sub $1, %rax
    mov (%rsi,%rax), %r9b
    mov %r9b, (%rdi,%rcx)
    add $1, %rcx
    jmp .Lbp_revloop
.Lbp_ret:
    mov %r8, %rax
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    pop %rbp
    ret

/* ─────────────────── $rt_formater_flottant ─────────────────── */
/* rax = bits IEEE-754. Écrit le texte dans SORTIE, rend sa longueur. */
.globl rt_formater_flottant
.balign 16
rt_formater_flottant:
    push %rbp
    mov %rsp, %rbp
    push %rbx
    push %r12
    push %r13
    push %r14
    push %r15
    sub $320, %rsp
    mov %rax, %r9              /* bits d'origine */
    movq %rax, %xmm0
    movapd %xmm0, %xmm1
    andpd .Labs_mask(%rip), %xmm1   /* xmm1 = |d| */
    lea .Lpow10(%rip), %rsi

    /* NaN / Inf */
    mov %r9, %rax
    shr $52, %rax
    and $0x7FF, %eax
    cmp $0x7FF, %eax
    jne .Lf_normal
    mov %r9, %rax
    movabs $0x000FFFFFFFFFFFFF, %r11
    and %r11, %rax
    jz .Lf_inf
    lea .Ls_nan(%rip), %r10
    mov $11, %r12d
    jmp .Lf_lit
.Lf_inf:
    movabs $0x8000000000000000, %r11
    test %r11, %r9
    jz .Lf_infpos
    lea .Ls_infneg(%rip), %r10
    mov $8, %r12d
    jmp .Lf_lit
.Lf_infpos:
    lea .Ls_inf(%rip), %r10
    mov $7, %r12d
.Lf_lit:
    mov $64, %r13d
    xor %rcx, %rcx
.Lf_litloop:
    mov (%r10,%rcx), %r11b
    mov %r11b, (%rsp,%r13)
    add $1, %rcx
    add $1, %r13
    cmp %r12d, %ecx
    jne .Lf_litloop
    jmp .Lf_store

.Lf_normal:
    /* partie entière : |d| < 1e16 et d == trunc(d) */
    movsd 128(%rsi), %xmm5     /* 1e16 */
    ucomisd %xmm5, %xmm1
    jae .Lf_general
    cvtsd2si %xmm1, %r10
    cvtsi2sd %r10, %xmm3
    ucomisd %xmm3, %xmm1
    jne .Lf_general
    movabs $0x8000000000000000, %r11
    test %r11, %r9
    jz .Lf_entpos
    neg %r10
.Lf_entpos:
    mov %r10, %rax
    call .Ldec_to_digits
    mov $64, %r13d
    call .Lcopy_digits
    jmp .Lf_store

.Lf_general:
    /* e10 par comparaison avec la table des puissances exactes */
    mov $0, %r8d
    mov $22, %r10d
.Lf_ebig:
    movsd (%rsi,%r10,8), %xmm7
    ucomisd %xmm7, %xmm1
    jb .Lf_esmall
    mov %r10d, %r8d
    jmp .Lf_efound
.Lf_esmall:
    sub $1, %r10d
    cmp $0, %r10d
    jge .Lf_ebig
    mov $1, %r10d
.Lf_eneg:
    movsd (%rsi), %xmm4       /* 1.0 */
    movsd (%rsi,%r10,8), %xmm7
    divsd %xmm7, %xmm4
    ucomisd %xmm4, %xmm1
    jb .Lf_eneg2
    neg %r8d
    jmp .Lf_efound
.Lf_eneg2:
    add $1, %r10d
    cmp $22, %r10d
    jle .Lf_eneg
    mov $-22, %r8d
.Lf_efound:
    mov $1, %edi             /* p */
.Lf_p:
    lea -1(%rdi), %ebx
    sub %r8d, %ebx           /* k */
    movapd %xmm1, %xmm3
    mov %ebx, %r10d
    cmp $0, %r10d
    jge .Lf_kabs
    neg %r10d
.Lf_kabs:
    cmp $22, %r10d
    jg .Lf_scale_loop
    movsd (%rsi,%r10,8), %xmm7
    cmp $0, %ebx
    jl .Lf_kneg
    mulsd %xmm7, %xmm3
    jmp .Lf_scaled
.Lf_kneg:
    divsd %xmm7, %xmm3
    jmp .Lf_scaled
.Lf_scale_loop:
    test %r10d, %r10d
    jz .Lf_scaled
    mulsd 8(%rsi), %xmm3
    sub $1, %r10d
    jmp .Lf_scale_loop
.Lf_scaled:
    /* correction FMA : le produit peut perdre son dernier bit */
    cmp $0, %ebx
    jl .Lf_noround
    cmp $22, %ebx
    jg .Lf_noround
    movapd %xmm3, %xmm8
    xorpd .Lsign_mask(%rip), %xmm8
    vfmadd231sd %xmm1, %xmm7, %xmm8   /* residu = d*pow10 - t */
    cvtsd2si %xmm8, %r11
    cvtsd2si %xmm3, %r13
    add %r11, %r13
    jmp .Lf_nready
.Lf_noround:
    cvtsd2si %xmm3, %r13
.Lf_nready:
    mov %r13, %r14
    mov %ebx, %r15d
    mov %r13, %rax
    call .Lreconstruire
    cmp %r9, %r11
    je .Lf_trouve
    add $1, %r13
    mov %r13, %rax
    call .Lreconstruire
    cmp %r9, %r11
    je .Lf_trouve
    sub $2, %r13
    mov %r13, %rax
    call .Lreconstruire
    cmp %r9, %r11
    je .Lf_trouve
    add $1, %r13
    add $1, %edi
    cmp $17, %edi
    jle .Lf_p
    mov %r14, %r13
    mov %r15d, %ebx
.Lf_trouve:
    /* N a-t-il plus de p chiffres ? */
    mov %r13, %rax
    xor %r12d, %r12d
.Lf_nbd:
    test %rax, %rax
    jz .Lf_nbddone
    xor %edx, %edx
    mov $10, %r11d
    div %r11
    add $1, %r12d
    jmp .Lf_nbd
.Lf_nbddone:
    cmp %edi, %r12d
    jle .Lf_digitok
    xor %edx, %edx
    mov $10, %r11d
    mov %r13, %rax
    div %r11
    mov %rax, %r13
    sub $1, %ebx
    add $1, %r8d
.Lf_digitok:
    mov %r13, %rax
    call .Ldec_to_digits
    cmp %edi, %r14d
    jge .Lf_nopad
    mov %edi, %r14d
    sub %r14d, %r14d
    mov %r14, %rcx
.Lf_padloop:
    test %rcx, %rcx
    jz .Lf_paddone
    sub $1, %rcx
    mov 16(%rsp,%rcx), %r11b
    mov %r14d, %eax
    add %ecx, %eax
    mov %r11b, 16(%rsp,%rax)
    jmp .Lf_padloop
.Lf_paddone:
    xor %rcx, %rcx
.Lf_padzero:
    cmp %r14d, %ecx
    jae .Lf_nopad
    movb $0x30, 16(%rsp,%rcx)
    add $1, %ecx
    jmp .Lf_padzero
.Lf_nopad:
    mov %edi, %r14d
.Lf_trim:
    cmp $1, %r14d
    jbe .Lf_trimdone
    mov %r14d, %ecx
    sub $1, %ecx
    cmpb $0x30, 16(%rsp,%rcx)
    jne .Lf_trimdone
    sub $1, %r14d
    jmp .Lf_trim
.Lf_trimdone:
    mov %edi, %r8d
    sub $1, %r8d
    sub %ebx, %r8d           /* expo */
    mov $64, %r13d
    /* %g : signe */
    xor %r10d, %r10d         /* drapeau point/e */
    movabs $0x8000000000000000, %r11
    test %r11, %r9
    jz .Lg_pos
    movb $0x2D, (%rsp,%r13)
    add $1, %r13
.Lg_pos:
    cmp $-4, %r8d
    jl .Lg_sci
    cmp %edi, %r8d
    jge .Lg_sci
    cmp $0, %r8d
    jl .Lg_frac
    jmp .Lg_fixed
.Lg_sci:
    mov 16(%rsp), %r11b
    mov %r11b, (%rsp,%r13)
    add $1, %r13
    cmp $1, %r14d
    jbe .Lg_sciexp
    movb $0x2E, (%rsp,%r13)
    add $1, %r13
    mov $1, %rcx
.Lg_scid:
    mov 16(%rsp,%rcx), %r11b
    mov %r11b, (%rsp,%r13)
    add $1, %rcx
    add $1, %r13
    cmp %r14d, %ecx
    jne .Lg_scid
.Lg_sciexp:
    movb $0x65, (%rsp,%r13)
    add $1, %r13
    mov $1, %r10d
    cmp $0, %r8d
    jge .Lg_expp
    movb $0x2D, (%rsp,%r13)
    add $1, %r13
    neg %r8d
    jmp .Lg_expn
.Lg_expp:
    movb $0x2B, (%rsp,%r13)
    add $1, %r13
.Lg_expn:
    mov %r8d, %eax
    call .Ldec_to_digits
    call .Lcopy_digits
    jmp .Lg_len
.Lg_frac:
    movb $0x30, (%rsp,%r13)
    add $1, %r13
    movb $0x2E, (%rsp,%r13)
    add $1, %r13
    mov $1, %r10d
    neg %r8d
    sub $1, %r8d
.Lg_zeros:
    test %r8d, %r8d
    jz .Lg_fracd
    movb $0x30, (%rsp,%r13)
    add $1, %r13
    sub $1, %r8d
    jmp .Lg_zeros
.Lg_fracd:
    call .Lcopy_digits
    jmp .Lg_len
.Lg_fixed:
    mov %r8d, %ecx
    add $1, %ecx            /* ip */
    cmp %ecx, %r14d
    jg .Lg_fixdot
    xor %rax, %rax
.Lg_fixz:
    mov 16(%rsp,%rax), %r11b
    mov %r11b, (%rsp,%r13)
    add $1, %rax
    add $1, %r13
    cmp %r14d, %eax
    jne .Lg_fixz
    sub %r14d, %ecx
.Lg_fixz2:
    test %ecx, %ecx
    jz .Lg_len
    movb $0x30, (%rsp,%r13)
    add $1, %r13
    sub $1, %ecx
    jmp .Lg_fixz2
.Lg_fixdot:
    xor %rax, %rax
.Lg_fixa:
    mov 16(%rsp,%rax), %r11b
    mov %r11b, (%rsp,%r13)
    add $1, %rax
    add $1, %r13
    cmp %ecx, %eax
    jne .Lg_fixa
    movb $0x2E, (%rsp,%r13)
    add $1, %r13
    mov $1, %r10d
.Lg_fixb:
    mov 16(%rsp,%rax), %r11b
    mov %r11b, (%rsp,%r13)
    add $1, %rax
    add $1, %r13
    cmp %r14d, %eax
    jne .Lg_fixb
.Lg_len:
    test %r10d, %r10d
    jnz .Lg_len2
    movb $0x2E, (%rsp,%r13)
    add $1, %r13
    movb $0x30, (%rsp,%r13)
    add $1, %r13
.Lg_len2:
    mov %r13d, %r14d
    sub $64, %r14d
.Lf_store:
    mov $0x402000, %r10
    mov %r14d, (%r10)
    lea 8(%r10), %rdi
    xor %rcx, %rcx
.Lf_final:
    cmp %r14d, %ecx
    jae .Lf_ret
    mov %rcx, %rax
    add $64, %rax
    mov (%rsp,%rax), %r11b
    mov %r11b, (%rdi,%rcx)
    add $1, %rcx
    jmp .Lf_final
.Lf_ret:
    mov %r14d, %eax
    add $320, %rsp
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbx
    pop %rbp
    ret

/* rax = N, r4d = k → r11 = bits du f64 reconstruit. */
.Lreconstruire:
    cvtsi2sd %rax, %xmm6
    mov %ebx, %r10d
    cmp $0, %r10d
    jge .Lr_pos
    neg %r10d
    cmp $22, %r10d
    jg .Lr_loopmul
    movsd (%rsi,%r10,8), %xmm7
    mulsd %xmm7, %xmm6
    jmp .Lr_bits
.Lr_loopmul:
    test %r10d, %r10d
    jz .Lr_bits
    mulsd 8(%rsi), %xmm6
    sub $1, %r10d
    jmp .Lr_loopmul
.Lr_pos:
    cmp $22, %r10d
    jg .Lr_loopdiv
    movsd (%rsi,%r10,8), %xmm7
    divsd %xmm7, %xmm6
    jmp .Lr_bits
.Lr_loopdiv:
    test %r10d, %r10d
    jz .Lr_bits
    divsd 8(%rsi), %xmm6
    sub $1, %r10d
    jmp .Lr_loopdiv
.Lr_bits:
    movabs $0x8000000000000000, %r11
    test %r11, %r9
    jz .Lr_nosign
    xorpd .Lsign_mask(%rip), %xmm6
.Lr_nosign:
    movq %xmm6, %r11
    ret

/* rax = valeur → digits dans [rsp], count dans r16d, retour rax = count. */
.Ldec_to_digits:
    mov %r12, 288(%rsp)     /* préserve le curseur de l'appelant */
    mov %r15, 296(%rsp)     /* préserve le compteur de l'appelant */
    mov %rax, %r11          /* valeur à convertir */
    mov $200, %r12d         /* curseur des digits inversés */
    mov $10, %r10d          /* diviseur (préservé) */
    xor %r15d, %r15d        /* nombre de digits */
.Ld1:
    mov %r11, %rax
    xor %edx, %edx
    div %r10
    mov %rax, %r14          /* quotient */
    mov %rdx, %rax          /* reste */
    add $0x30, %eax
    sub $1, %r12d
    mov %al, (%rsp,%r12)
    add $1, %r15d
    mov %r14, %r11
    test %r14, %r14
    jnz .Ld1
    xor %rcx, %rcx
.Ld2:
    mov (%rsp,%r12), %r10b
    mov %r10b, 24(%rsp,%rcx)
    add $1, %r12d
    add $1, %rcx
    cmp %r15d, %ecx
    jne .Ld2
    mov %r15d, %r14d        /* count dans r14 (attendu par l'appelant) */
    mov %r15d, %eax
    mov 296(%rsp), %r15
    mov 288(%rsp), %r12
    ret

/* copie r16d digits de [rsp] vers [rsp+r13], avance r13. */
.Lcopy_digits:
    xor %rcx, %rcx
.Lc1:
    mov 16(%rsp,%rcx), %r11b
    mov %r11b, (%rsp,%r13)
    add $1, %rcx
    add $1, %r13
    cmp %r14d, %ecx
    jne .Lc1
    ret

.balign 8
.Labs_mask: .quad 0x7FFFFFFFFFFFFFFF
.Lsign_mask: .quad 0x8000000000000000
.Lpow10:
    .double 1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7
    .double 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15
    .double 1e16, 1e17, 1e18, 1e19, 1e20, 1e21, 1e22
.Ls_nan: .ascii "non_nombre\n"
.Ls_inf: .ascii "infini\n"
.Ls_infneg: .ascii "-infini\n"
