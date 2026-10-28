#include "rose_runtime.h"
#include <cstdint>
#include <cstdio>
#include <string>

rose_ent r_fib(rose_ent r_n);

rose_ent r_fib(rose_ent r_n) {
    rose_ent r_t0 = 0;
    rose_bool r_t1 = 0;
    rose_ent r_t2 = 0;
    rose_ent r_t3 = 0;
    rose_ent r_t4 = 0;
    rose_ent r_t5 = 0;
    rose_ent r_t6 = 0;
    rose_ent r_t7 = 0;
    rose_ent r_t8 = 0;
    r_t0 = 2;
    r_t1 = (r_n < r_t0);
    if (r_t1) goto r__alors_49_0;
    else goto r__sinon_49_0;
  r__alors_49_0:
    return r_n;
    goto r__fin_49_0;
  r__sinon_49_0:
    goto r__fin_49_0;
  r__fin_49_0:
    r_t2 = 1;
    r_t3 = r_n - r_t2;
    r_t4 = r_fib(r_t3);
    r_t5 = 2;
    r_t6 = r_n - r_t5;
    r_t7 = r_fib(r_t6);
    r_t8 = r_t4 + r_t7;
    return r_t8;
}

int main(int argc, char **argv) {
    rose_pile_geante(argc, argv);
    rose_runtime_set_args(argc, argv);
    rose_ent r_a = 0;
    rose_ent r_b = 0;
    rose_tampon r_buf;
    rose_ent r_c = 0;
    rose_ent r_d = 0;
    rose_ent r_e = 0;
    rose_ent r_f = 0;
    rose_ent r_m0 = 0;
    rose_ent r_m1 = 0;
    rose_liste r_piles;
    rose_qd r_r = 0;
    rose_str r_s = "";
    rose_str r_t = "";
    rose_table r_tab;
    rose_ent r_total = 0;
    rose_float r_x = 0;

    bool _def_r_b = false;
    bool _def_r_buf = false;
    bool _def_r_c = false;
    bool _def_r_d = false;
    bool _def_r_e = false;
    bool _def_r_f = false;
    bool _def_r_m1 = false;
    bool _def_r_piles = false;
    bool _def_r_r = false;
    bool _def_r_s = false;
    bool _def_r_t = false;
    bool _def_r_tab = false;
    bool _def_r_x = false;

    rose_ent r_t10 = 0;
    rose_ent r_t11 = 0;
    rose_ent r_t12 = 0;
    rose_bool r_t13 = 0;
    rose_ent r_t14 = 0;
    rose_ent r_t15 = 0;
    rose_ent r_t16 = 0;
    rose_float r_t17 = 0;
    rose_ent r_t18 = 0;
    rose_ent r_t19 = 0;
    rose_bool r_t20 = 0;
    rose_float r_t21 = 0;
    rose_float r_t22 = 0;
    rose_float r_t23 = 0;
    rose_ent r_t24 = 0;
    rose_qd r_t25 = 0;
    rose_ent r_t26 = 0;
    rose_ent r_t27 = 0;
    rose_bool r_t28 = 0;
    rose_ent r_t29 = 0;
    rose_qd r_t30 = 0;
    rose_qd r_t31 = 0;
    rose_ent r_t32 = 0;
    rose_str r_t33 = "";
    rose_str r_t34 = "";
    rose_ent r_t35 = 0;
    rose_ent r_t36 = 0;
    rose_bool r_t37 = 0;
    rose_ent r_t38 = 0;
    rose_ent r_t39 = 0;
    rose_str r_t40 = "";
    rose_str r_t41 = "";
    rose_ent r_t42 = 0;
    rose_ent r_t43 = 0;
    rose_liste r_t44;
    rose_ent r_t45 = 0;
    rose_ent r_t46 = 0;
    rose_ent r_t47 = 0;
    rose_bool r_t48 = 0;
    rose_ent r_t49 = 0;
    rose_ent r_t50 = 0;
    rose_ent r_t51 = 0;
    rose_ent r_t52 = 0;
    rose_ent r_t53 = 0;
    rose_table r_t54;
    rose_ent r_t55 = 0;
    rose_ent r_t56 = 0;
    rose_bool r_t57 = 0;
    rose_str r_t58 = "";
    rose_str r_t59 = "";
    rose_str r_t60 = "";
    rose_ent r_t61 = 0;
    rose_ent r_t62 = 0;
    rose_ent r_t63 = 0;
    rose_ent r_t64 = 0;
    rose_str r_t65 = "";
    rose_dyn r_t66;
    rose_ent r_t67 = 0;
    rose_ent r_t68 = 0;
    rose_ent r_t69 = 0;
    rose_ent r_t70 = 0;
    rose_ent r_t71 = 0;
    rose_tampon r_t72;
    rose_str r_t73 = "";
    rose_tampon r_t74;
    rose_ent r_t75 = 0;
    rose_ent r_t76 = 0;
    rose_str r_t77 = "";
    rose_str r_t78 = "";
    rose_ent r_t79 = 0;
    rose_str r_t80 = "";
    rose_str r_t81 = "";
    rose_ent r_t82 = 0;
    rose_ent r_t83 = 0;
    rose_ent r_t9 = 0;
    r_t9 = rose_millisecondes();
    r_m0 = r_t9;
    r_t10 = 0;
    r_total = r_t10;
    r_t11 = 1;
    r_a = r_t11;
    goto r__cond_6_1;
  r__cond_6_1:
    r_t12 = 10000001;
    r_t13 = (r_a < r_t12);
    if (r_t13) goto r__corps_6_1;
    else goto r__fin_6_1;
  r__corps_6_1:
    r_t14 = r_a * r_a;
    r_total += r_t14;
    goto r__incr_6_1;
  r__incr_6_1:
    r_a += 1;
    goto r__cond_6_1;
  r__fin_6_1:
    rose_afficher_int(r_total);
    r_t17 = 0.0;
    r_x = r_t17;
    _def_r_x = true;
    r_t18 = 1;
    r_b = r_t18;
    _def_r_b = true;
    goto r__cond_13_2;
  r__cond_13_2:
    r_t19 = 1000001;
    r_t20 = (rose_lit(r_b, _def_r_b, "b") < r_t19);
    if (r_t20) goto r__corps_13_2;
    else goto r__fin_13_2;
  r__corps_13_2:
    r_t21 = 1.0;
    r_t22 = rose_div_f(r_t21, rose_vers_double(rose_lit(r_b, _def_r_b, "b")));
    r_t23 = rose_lit(r_x, _def_r_x, "x") + r_t22;
    r_x = r_t23;
    _def_r_x = true;
    goto r__incr_13_2;
  r__incr_13_2:
    r_t24 = rose_lit(r_b, _def_r_b, "b") + 1;
    r_b = r_t24;
    _def_r_b = true;
    goto r__cond_13_2;
  r__fin_13_2:
    rose_afficher_float(rose_lit(r_x, _def_r_x, "x"));
    r_t25 = rose_qd("1/3");
    r_r = r_t25;
    _def_r_r = true;
    r_t26 = 1;
    r_c = r_t26;
    _def_r_c = true;
    goto r__cond_18_3;
  r__cond_18_3:
    r_t27 = 1001;
    r_t28 = (rose_lit(r_c, _def_r_c, "c") < r_t27);
    if (r_t28) goto r__corps_18_3;
    else goto r__fin_18_3;
  r__corps_18_3:
    r_t29 = 1;
    r_t30 = rose_q_div(rose_q_de(r_t29), rose_q_de(rose_lit(r_c, _def_r_c, "c")));
    r_t31 = rose_lit(r_r, _def_r_r, "r") + r_t30;
    r_r = r_t31;
    _def_r_r = true;
    goto r__incr_18_3;
  r__incr_18_3:
    r_t32 = rose_lit(r_c, _def_r_c, "c") + 1;
    r_c = r_t32;
    _def_r_c = true;
    goto r__cond_18_3;
  r__fin_18_3:
    rose_afficher_q(rose_lit(r_r, _def_r_r, "r"));
    r_t33 = "ete radieux";
    r_t = r_t33;
    _def_r_t = true;
    r_t34 = "";
    r_s = r_t34;
    _def_r_s = true;
    r_t35 = 1;
    r_d = r_t35;
    _def_r_d = true;
    goto r__cond_26_4;
  r__cond_26_4:
    r_t36 = 1001;
    r_t37 = (rose_lit(r_d, _def_r_d, "d") < r_t36);
    if (r_t37) goto r__corps_26_4;
    else goto r__fin_26_4;
  r__corps_26_4:
    r_t38 = 0;
    r_t39 = 3;
    r_t40 = rose_sous_chaine(rose_lit(r_t, _def_r_t, "t"), r_t38, r_t39);
    r_t41 = r_t40 + rose_lit(r_s, _def_r_s, "s");
    r_s = r_t41;
    _def_r_s = true;
    goto r__incr_26_4;
  r__incr_26_4:
    r_t42 = rose_lit(r_d, _def_r_d, "d") + 1;
    r_d = r_t42;
    _def_r_d = true;
    goto r__cond_26_4;
  r__fin_26_4:
    r_t43 = rose_longueur(rose_lit(r_s, _def_r_s, "s"));
    rose_afficher_int(r_t43);
    r_t44 = rose_liste_new();
    r_t45 = 0;
    rose_liste_push_int(r_t44, r_t45);
    r_piles = r_t44;
    _def_r_piles = true;
    r_t46 = 1;
    r_e = r_t46;
    _def_r_e = true;
    goto r__cond_33_5;
  r__cond_33_5:
    r_t47 = 10001;
    r_t48 = (rose_lit(r_e, _def_r_e, "e") < r_t47);
    if (r_t48) goto r__corps_33_5;
    else goto r__fin_33_5;
  r__corps_33_5:
    rose_liste_push_int(rose_lit(r_piles, _def_r_piles, "piles"), rose_lit(r_e, _def_r_e, "e"));
    goto r__incr_33_5;
  r__incr_33_5:
    r_t50 = rose_lit(r_e, _def_r_e, "e") + 1;
    r_e = r_t50;
    _def_r_e = true;
    goto r__cond_33_5;
  r__fin_33_5:
    r_t51 = rose_liste_len(rose_lit(r_piles, _def_r_piles, "piles"));
    rose_afficher_int(r_t51);
    r_t52 = 5000;
    r_t53 = rose_a_entier(rose_liste_get_str(rose_lit(r_piles, _def_r_piles, "piles"), r_t52));
    rose_afficher_int(r_t53);
    r_t54 = rose_table_new();
    r_tab = r_t54;
    _def_r_tab = true;
    r_t55 = 1;
    r_f = r_t55;
    _def_r_f = true;
    goto r__cond_41_6;
  r__cond_41_6:
    r_t56 = 5001;
    r_t57 = (rose_lit(r_f, _def_r_f, "f") < r_t56);
    if (r_t57) goto r__corps_41_6;
    else goto r__fin_41_6;
  r__corps_41_6:
    r_t58 = "k";
    r_t59 = rose_chaine(rose_lit(r_f, _def_r_f, "f"));
    r_t60 = r_t58 + r_t59;
    r_t61 = rose_lit(r_f, _def_r_f, "f") * rose_lit(r_f, _def_r_f, "f");
    rose_table_inserer(rose_lit(r_tab, _def_r_tab, "tab"), r_t60, rose_dyn_de(r_t61));
    goto r__incr_41_6;
  r__incr_41_6:
    r_t63 = rose_lit(r_f, _def_r_f, "f") + 1;
    r_f = r_t63;
    _def_r_f = true;
    goto r__cond_41_6;
  r__fin_41_6:
    r_t64 = rose_table_longueur(rose_lit(r_tab, _def_r_tab, "tab"));
    rose_afficher_int(r_t64);
    r_t65 = "k2500";
    r_t66 = rose_table_get(rose_lit(r_tab, _def_r_tab, "tab"), r_t65);
    r_t66.afficher();
    r_t67 = 28;
    r_t68 = r_fib(r_t67);
    rose_afficher_int(r_t68);
    r_t69 = 2026;
    rose_graine(r_t69);
    r_t71 = 1000000;
    r_t72 = rose_tampon_new(r_t71);
    r_buf = r_t72;
    _def_r_buf = true;
    r_t73 = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-";
    r_t74 = rose_octets(r_t73.c_str());
    rose_remplir_hasard(rose_lit(r_buf, _def_r_buf, "buf"), r_t74);
    r_t76 = rose_tampon_longueur(rose_lit(r_buf, _def_r_buf, "buf"));
    rose_afficher_int(r_t76);
    r_t77 = "global_out.txt";
    r_t78 = "bench global Rose";
    rose_ecrire_texte(r_t77, r_t78);
    r_t80 = "global_out.txt";
    r_t81 = rose_lire_texte(r_t80);
    rose_afficher_str(r_t81.c_str());
    r_t82 = rose_millisecondes();
    r_m1 = r_t82;
    _def_r_m1 = true;
    r_t83 = rose_lit(r_m1, _def_r_m1, "m1") - r_m0;
    rose_afficher_int(r_t83);
    return 0;
    return 0;
}

