/*
 * RUNTIME NATIF AOT6 — aarch64 (AAPCS64, Linux)
 * ==============================================
 * « compiler-rt » de Rose : assemblé une fois, ses octets sont intégrés
 * dans chaque binaire AOT par natif_emetteur. À l'exécution, ce sont de
 * vraies instructions aarch64 qui tournent dans le binaire émis.
 *
 * Signature commune : i64 routine(base: *mut i64, tramp: *const Trampoline)
 * x19 = base (callee-saved), x20 = tramp, résultat dans x0.
 *
 * Zones absolues (ELF ET_EXEC, base 0x40_0000) :
 *   SORTIE_ADDR 0x40_9000 : [u64 len][octets...]
 *   TMP_ADDR    0x40_B000 : workspace bigint
 */

.text
.balign 16

/* ─────────────────────────── $rt_hasard(base) ───────────────────────── */
.globl rt_hasard
.balign 16
rt_hasard:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    stp x19, x20, [sp, #-16]!
    mov x19, x0
    mov x20, x1
    ldr x15, [x19]                  /* borne, lue AVANT d'écraser base[0] */
    str xzr, [x19]
    mov x0, x19
    mov x1, #8
    mov x2, xzr
    mov x8, #278
    svc #0
    ldr x9, [x19]
    mov x10, sp
    eor x9, x9, x10
    ldr x11, =0x9E3779B97F4A7C15
    add x9, x9, x11
    str x9, [x19]
    mov x12, x9
    eor x12, x12, x12, lsr #30
    ldr x13, =0xBF58476D1CE4E5B9
    mul x12, x12, x13
    eor x12, x12, x12, lsr #27
    ldr x13, =0x94D049BB133111EB
    mul x12, x12, x13
    eor x12, x12, x12, lsr #31
    /* borne déjà dans x15 */
    cbz x15, .Lh_zero
    mov x10, #1
    sub x10, x15, x10
    and x10, x15, x10
    cbz x10, .Lh_p2
    umulh x0, x12, x15
    b .Lh_fin
.Lh_p2:
    and x0, x12, x10
    b .Lh_fin
.Lh_zero:
    mov x0, xzr
.Lh_fin:
    ldp x19, x20, [sp], #16
    ldp x29, x30, [sp], #16
    ret

/* ───────── $rt_sortir(base) : écrire SORTIE sur stdout ──────────────── */
.globl rt_sortir
.balign 16
rt_sortir:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    stp x19, x20, [sp, #-16]!
    mov x9, x0                      /* valeur rendue = code de retour */
    ldr x10, =0x402000
    ldr x11, [x10]
    mov x0, #1
    add x1, x10, #8
    mov x2, x11
    mov x8, #64
    svc #0
    mov x0, x9
    ldp x19, x20, [sp], #16
    ldp x29, x30, [sp], #16
    ret

/* ───────── $rt_formater_flottant(base) ────────────────────────────────
 * x0 = bits IEEE-754. Écrit le texte dans SORTIE, rend sa longueur.
 * Miroir de front::texte_flottant / run::chaine_de.
 *
 * Scratch pile (128 octets réservés) :
 *   [sp+0 .. sp+15]   : digits du significande (nd digits)
 *   [sp+16.. sp+79]   : texte de sortie en cours (cursor x13)
 */
.globl rt_formater_flottant
.balign 16
rt_formater_flottant:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    stp x19, x20, [sp, #-16]!
    stp x21, x22, [sp, #-16]!
    sub sp, sp, #256
    mov x19, x0
    mov x20, x1

    mov x9, x0
    fmov d0, x0
    fabs d1, d0

    adr x7, .Lfloat_table
    ldr d4, [x7]                /* 1.0 */
    ldr d2, [x7, #8]            /* 10.0 */
    ldr d5, [x7, #128]          /* 10^16 = 1e16 */

    /* ── NaN / Inf ── */
    ldr x10, =0x7FF0000000000000
    and x10, x9, x10
    ldr x11, =0x7FF0000000000000
    cmp x10, x11
    b.ne .Lf_normal
    ldr x10, =0x000FFFFFFFFFFFFF
    and x10, x9, x10
    cbz x10, .Lf_inf
    adr x10, .Ls_nan
    mov x12, #11
    b .Lf_copie_lit
.Lf_inf:
    ldr x10, =0x8000000000000000
    and x10, x9, x10
    bne .Lf_infneg
    adr x10, .Ls_inf
    mov x12, #7
    b .Lf_copie_lit
.Lf_infneg:
    adr x10, .Ls_infneg
    mov x12, #8
.Lf_copie_lit:
    mov x13, #64
    mov x14, #0
.Lf_lit_loop:
    ldrb w15, [x10, x14]
    strb w15, [sp, x13]
    add x14, x14, #1
    add x13, x13, #1
    cmp x14, x12
    b.ne .Lf_lit_loop
    mov x16, x12
    b .Lf_store

    /* ── partie entière (run::chaine_de) ── */
.Lf_normal:
    fcmp d1, d5
    b.ge .Lf_general
    fcvtas x10, d1
    scvtf d3, x10
    fcmp d1, d3
    b.ne .Lf_general
    ldr x11, =0x8000000000000000
    and x11, x9, x11
    cbz x11, .Lf_ent_pos
    neg x10, x10
.Lf_ent_pos:
    mov x0, x10
    bl .Ldec_to_digits      /* digits dans [sp+0], nd dans x16 */
    mov x13, #64
    bl .Lcopy_digits        /* copie [sp+0..nd] vers [sp+x13], avance x13 */
    b .Lf_store

    /* ── exposant décimal ── */
.Lf_general:
    /* e10 : plus grand k de [-22,22] avec 10^k <= |d|. Comparaison par
     * table (10^k exacts) — pas de dérive par divisions répétées. */
    mov x2, #0
    mov x10, #22
.Lf_ebig:
    add x11, x7, x10, lsl #3
    ldr d7, [x11]
    fcmp d1, d7
    b.lo .Lf_esmall
    mov x2, x10
    b .Lf_efound
.Lf_esmall:
    sub x10, x10, #1
    cmp x10, #0
    b.ge .Lf_ebig
    mov x10, #1
.Lf_eneg:
    add x11, x7, x10, lsl #3
    ldr d7, [x11]
    fdiv d7, d4, d7
    fcmp d1, d7
    b.lo .Lf_eneg2
    neg x2, x10
    b .Lf_efound
.Lf_eneg2:
    add x10, x10, #1
    cmp x10, #22
    b.le .Lf_eneg
    mov x2, #-22
.Lf_efound:
    mov x3, #1
    mov x17, #0
    mov x18, #0
.Lf_p:
    sub x4, x3, #1
    sub x4, x4, x2          /* k */
    fmov d3, d1
    mov x10, x4
    cmp x10, #0
    b.ge .Lf_kabs
    neg x10, x10
.Lf_kabs:
    cmp x10, #22
    b.gt .Lf_scale_loop
    add x11, x7, x10, lsl #3
    ldr d7, [x11]               /* d7 = 10^|k| (exact) */
    cmp x4, #0
    b.lt .Lf_kneg
    fmul d3, d3, d7         /* k >= 0 : t = |d| * 10^k */
    b .Lf_scaled
.Lf_kneg:
    fdiv d3, d3, d7         /* k < 0  : t = |d| / 10^(-k) */
    b .Lf_scaled
.Lf_scale_loop:
    cbz x10, .Lf_scaled
    fmul d3, d3, d2
    sub x10, x10, #1
    b .Lf_scale_loop
.Lf_scaled:
    /* Correction FMA : ad*10^k peut perdre son dernier bit en f64.
     * err = fma(ad, 10^k, -t) est le résidu exact ; on l'ajoute à round(t). */
    cmp x4, #0
    b.lt .Lf_no_corr
    cmp x4, #22
    b.gt .Lf_no_corr
    fneg d8, d3
    fmadd d8, d1, d7, d8       /* résidu exact du produit */
    fcvtas x10, d8
    fcvtas x5, d3
    add x5, x5, x10
    b .Lf_n_ready
.Lf_no_corr:
    fcvtas x5, d3
.Lf_n_ready:
    mov x17, x5
    mov x18, x4
    bl .Lreconstruire
    cmp x11, x9
    b.eq .Lf_trouve
    /* L'arrondi de |d|*10^k peut perdre un bit : essayer N±1. */
    add x5, x5, #1
    bl .Lreconstruire
    cmp x11, x9
    b.eq .Lf_trouve
    sub x5, x5, #2
    bl .Lreconstruire
    cmp x11, x9
    b.eq .Lf_trouve
    add x5, x5, #1             /* restaure N */
    add x3, x3, #1
    cmp x3, #17
    b.le .Lf_p
    mov x5, x17
    mov x4, x18
.Lf_trouve:
    /* N = 10 * nombre de chiffres de N ; si N >= 10^p, on divise */
    mov x11, x5
    mov x12, #0
.Lf_nbd:
    cbz x11, .Lf_nbd_done
    mov x14, #10
    udiv x11, x11, x14
    add x12, x12, #1
    b .Lf_nbd
.Lf_nbd_done:
    cmp x12, x3
    b.le .Lf_digits_ok
    mov x14, #10
    udiv x5, x5, x14
    sub x4, x4, #1
    add x2, x2, #1
.Lf_digits_ok:
    mov x0, x5
    bl .Ldec_to_digits
    /* padding à p chiffres : x16 = p, insertion de zéros au début */
    cmp x16, x3
    b.ge .Lf_no_pad
    sub x14, x3, x16
    mov x15, x16
.Lf_pad:
    cbz x15, .Lf_pad_done
    sub x15, x15, #1
    ldrb w11, [sp, x15]
    add x10, x15, x14
    strb w11, [sp, x10]
    b .Lf_pad
.Lf_pad_done:
    mov x10, #0
.Lf_pad0:
    cmp x10, x14
    b.ge .Lf_no_pad
    mov w11, #0x30
    strb w11, [sp, x10]
    add x10, x10, #1
    b .Lf_pad0
.Lf_no_pad:
    mov x16, x3
    /* trim zéros finaux */
.Lf_trim:
    cmp x16, #1
    b.le .Lf_trim_done
    sub x10, x16, #1
    ldrb w11, [sp, x10]
    cmp w11, #0x30
    b.ne .Lf_trim_done
    sub x16, x16, #1
    b .Lf_trim
.Lf_trim_done:
    mov x2, x3
    sub x2, x2, #1
    sub x2, x2, x4          /* expo */
    mov x13, #64            /* curseur du texte de sortie */

    /* ── %g ── */
    mov x10, #0                  /* drapeau : un '.' ou 'e' a été émis */
    ldr x11, =0x8000000000000000
    and x11, x9, x11
    cbz x11, .Lg_pos
    mov w15, #0x2D
    strb w15, [sp, x13]
    add x13, x13, #1
.Lg_pos:
    cmp x2, #-4
    b.lt .Lg_sci
    cmp x2, x3
    b.ge .Lg_sci
    cmp x2, #0
    b.lt .Lg_frac
    b .Lg_fixed

.Lg_sci:
    ldrb w15, [sp, #0]
    strb w15, [sp, x13]
    add x13, x13, #1
    cmp x16, #1
    b.le .Lg_sci_exp
    mov w15, #0x2E
    strb w15, [sp, x13]
    add x13, x13, #1
    mov x14, #1
.Lg_sci_d:
    ldrb w15, [sp, x14]
    strb w15, [sp, x13]
    add x14, x14, #1
    add x13, x13, #1
    cmp x14, x16
    b.ne .Lg_sci_d
.Lg_sci_exp:
    mov w15, #0x65
    strb w15, [sp, x13]
    add x13, x13, #1
    mov x10, #1
    cmp x2, #0
    b.ge .Lg_expp
    mov w15, #0x2D
    strb w15, [sp, x13]
    add x13, x13, #1
    neg x2, x2
    b .Lg_expn
.Lg_expp:
    mov w15, #0x2B
    strb w15, [sp, x13]
    add x13, x13, #1
.Lg_expn:
    mov x0, x2
    bl .Ldec_to_digits
    bl .Lcopy_digits
    b .Lg_len

.Lg_frac:
    mov w15, #0x30
    strb w15, [sp, x13]
    add x13, x13, #1
    mov w15, #0x2E
    strb w15, [sp, x13]
    add x13, x13, #1
    mov x10, #1
    neg x14, x2
    sub x14, x14, #1
.Lg_zeros:
    cbz x14, .Lg_fracd
    mov w15, #0x30
    strb w15, [sp, x13]
    add x13, x13, #1
    sub x14, x14, #1
    b .Lg_zeros
.Lg_fracd:
    bl .Lcopy_digits
    b .Lg_len

.Lg_fixed:
    add x14, x2, #1
    cmp x16, x14
    b.gt .Lg_fixdot
    mov x15, #0
.Lg_fixz:
    ldrb w11, [sp, x15]
    strb w11, [sp, x13]
    add x13, x13, #1
    add x15, x15, #1
    cmp x15, x16
    b.ne .Lg_fixz
    sub x15, x14, x16
.Lg_fixz2:
    cbz x15, .Lg_len
    mov w11, #0x30
    strb w11, [sp, x13]
    add x13, x13, #1
    sub x15, x15, #1
    b .Lg_fixz2
.Lg_fixdot:
    mov x15, #0
.Lg_fixa:
    ldrb w11, [sp, x15]
    strb w11, [sp, x13]
    add x13, x13, #1
    add x15, x15, #1
    cmp x15, x14
    b.ne .Lg_fixa
    mov w11, #0x2E
    strb w11, [sp, x13]
    add x13, x13, #1
    mov x10, #1
.Lg_fixb:
    ldrb w11, [sp, x15]
    strb w11, [sp, x13]
    add x13, x13, #1
    add x15, x15, #1
    cmp x15, x16
    b.ne .Lg_fixb
.Lg_len:
    /* Rose ajoute ".0" si aucun point ni 'e' n'a été émis */
    cbz x10, .Lg_pas_point
    b .Lg_len2
.Lg_pas_point:
    mov w15, #0x2E
    strb w15, [sp, x13]
    add x13, x13, #1
    mov w15, #0x30
    strb w15, [sp, x13]
    add x13, x13, #1
.Lg_len2:
    sub x16, x13, #64

/* Point de sortie unique : [sp+16 .. sp+16+x16] → SORTIE+8. */
.Lf_store:
    ldr x10, =0x402000
    str x16, [x10]
    add x11, x10, #8
    mov x14, #0
.Lf_final_copy:
    cmp x14, x16
    b.ge .Lf_ret
    add x12, x14, #64
    ldrb w15, [sp, x12]
    strb w15, [x11, x14]
    add x14, x14, #1
    b .Lf_final_copy
.Lf_ret:
    mov x0, x16
    add sp, sp, #256
    ldp x21, x22, [sp], #16
    ldp x19, x20, [sp], #16
    ldp x29, x30, [sp], #16
    ret

/* N = x5, k = x4 → x11 = bits du f64 reconstruit N * 10^k (signe appliqué).
 * Préserve x5/x4/x9 ; clobber x10,x11,x12,d6,d7. */
.Lreconstruire:
    scvtf d6, x5
    mov x10, x4
    cmp x10, #0
    b.ge .Lr_pos
    neg x10, x10
    cmp x10, #22
    b.gt .Lr_loop_mul
    add x12, x7, x10, lsl #3
    ldr d7, [x12]
    fmul d6, d6, d7
    b .Lr_bits
.Lr_loop_mul:
    cbz x10, .Lr_bits
    fmul d6, d6, d2
    sub x10, x10, #1
    b .Lr_loop_mul
.Lr_pos:
    cmp x10, #22
    b.gt .Lr_loop_div
    add x12, x7, x10, lsl #3
    ldr d7, [x12]
    fdiv d6, d6, d7
    b .Lr_bits
.Lr_loop_div:
    cbz x10, .Lr_bits
    fdiv d6, d6, d2
    sub x10, x10, #1
    b .Lr_loop_div
.Lr_bits:
    ldr x12, =0x8000000000000000
    and x12, x9, x12
    cbz x12, .Lr_no_sign
    fneg d6, d6
.Lr_no_sign:
    fmov x11, d6
    ret

/* x0 → digits décimaux à [sp+0], count dans x0 (et x16). */
.Ldec_to_digits:
    mov x11, x0
    mov x12, #200
    mov x22, #10
    mov x15, #0
.Ld1:
    udiv x14, x11, x22
    msub x10, x14, x22, x11
    add x10, x10, #0x30
    sub x12, x12, #1
    strb w10, [sp, x12]
    add x15, x15, #1
    mov x11, x14
    cbnz x14, .Ld1
    mov x14, #0
.Ld2:
    ldrb w10, [sp, x12]
    strb w10, [sp, x14]
    add x12, x12, #1
    add x14, x14, #1
    cmp x14, x15
    b.ne .Ld2
    mov x16, x15
    mov x0, x15
    ret

/* copie [sp+0..x16] vers [sp+x13], avance x13 */
.Lcopy_digits:
    mov x14, #0
.Lc1:
    ldrb w15, [sp, x14]
    strb w15, [sp, x13]
    add x14, x14, #1
    add x13, x13, #1
    cmp x14, x16
    b.ne .Lc1
    ret

.Lfloat_table:
    .double 1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7
    .double 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15
    .double 1e16, 1e17, 1e18, 1e19, 1e20, 1e21, 1e22
.Ls_nan:    .ascii "non_nombre\n"
.Ls_inf:    .ascii "infini\n"
.Ls_infneg: .ascii "-infini\n"

/* ───────── $rt_lire_texte(base) ─────────────────────────────────────── */
.globl rt_lire_texte
.balign 16
rt_lire_texte:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    stp x19, x20, [sp, #-16]!
    mov x19, x0
    mov x20, x1
    /* AOT6 : lecture sur stdin (l'entrée standard), aucun prompt. */
    mov x0, #0
    ldr x1, =0x402008
    mov x2, #4096
    mov x8, #63
    svc #0
    subs x2, x0, #1
    b.ge .Ll_ok
    mov x2, #0
.Ll_ok:
    ldr x10, =0x402000
    str x2, [x10]
    mov x0, x10
    ldp x19, x20, [sp], #16
    ldp x29, x30, [sp], #16
    ret

/* ───────── $rt_bigint_puissance(base) ─────────────────────────────────
 * base[0] = a, base[1] = b (entiers ≥ 0). Calcule a^b en précision
 * arbitraire : limbes binaires 32 bits dans TMP_ADDR, puis conversion
 * décimale par divisions répétées. Le texte est écrit dans SORTIE.
 */
.globl rt_bigint_puissance
.balign 16
rt_bigint_puissance:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    stp x19, x20, [sp, #-16]!
    stp x21, x22, [sp, #-16]!
    mov x19, x0
    mov x20, x1
    ldr x0, [x19]               /* a */
    ldr x1, [x19, #8]           /* b */
    ldr x10, =0x403000          /* TMP_ADDR */
    mov x11, #1
    str x11, [x10]
    mov x12, #1                 /* nlimbs */

.Lbp_pow:
    cbz x1, .Lbp_dec
    mov x13, #0                 /* carry */
    mov x14, #0                 /* i */
.Lbp_mul:
    cmp x14, x12
    b.ge .Lbp_mul_done
    ldr x15, [x10, x14, lsl #3]
    mul x15, x15, x0
    add x15, x15, x13
    mov x11, #0xFFFFFFFF
    and x16, x15, x11
    str x16, [x10, x14, lsl #3]
    lsr x13, x15, #32
    add x14, x14, #1
    b .Lbp_mul
.Lbp_mul_done:
    cbz x13, .Lbp_next
    str x13, [x10, x12, lsl #3]
    add x12, x12, #1
.Lbp_next:
    sub x1, x1, #1
    b .Lbp_pow

    /* conversion décimale : divisions par 10 sur le tableau de limbes */
.Lbp_dec:
    ldr x18, =0x404000          /* tampon digits inversés */
    mov x16, #0                 /* ndigits */
.Lbp_outer:
    mov x21, x12
.Lbp_zero_check:
    cbz x21, .Lbp_outer_done
    sub x21, x21, #1
    ldr x22, [x10, x21, lsl #3]
    cbz x22, .Lbp_zero_check
    mov x13, #0                 /* reste / report entre limbes */
    mov x14, x12
.Lbp_div:
    sub x14, x14, #1
    ldr x15, [x10, x14, lsl #3]
    lsl x13, x13, #32
    orr x15, x13, x15
    mov x11, #10
    udiv x17, x15, x11         /* quotient du limbe */
    msub x13, x17, x11, x15    /* reste = report vers le limbe suivant */
    str x17, [x10, x14, lsl #3]
    cmp x14, #0
    b.ne .Lbp_div
    /* le reste final est le chiffre décimal de ce tour */
    add x17, x13, #0x30
    strb w17, [x18, x16]
    add x16, x16, #1
    b .Lbp_outer
.Lbp_outer_done:
    /* les zéros de tête sont à la FIN du tampon (digits du poids fort) */
.Lbp_trim:
    cmp x16, #1
    b.le .Lbp_rev_init
    sub x15, x16, #1
    ldrb w13, [x18, x15]
    cmp w13, #0x30
    b.ne .Lbp_rev_init
    sub x16, x16, #1
    b .Lbp_trim
.Lbp_rev_init:
    /* inverse les digits dans SORTIE+8 */
    ldr x10, =0x402000
    str x16, [x10]
    add x11, x10, #8
    mov x14, #0
.Lbp_rev:
    cmp x14, x16
    b.ge .Lbp_ret
    sub x15, x16, x14
    sub x15, x15, #1
    ldrb w13, [x18, x15]
    strb w13, [x11, x14]
    add x14, x14, #1
    b .Lbp_rev
.Lbp_ret:
    mov x0, x16
    ldp x21, x22, [sp], #16
    ldp x19, x20, [sp], #16
    ldp x29, x30, [sp], #16
    ret
