# Banc Python : même calcul complexe que complexe.rose (1 000 000 d'itérations)
#
# Le programme Rose fait :
#     $z = $z_re + $z_im * i
#     $w = $z * $z
#     $total_re += re($w) ; $total_im += im($w)
#
# On exprime EXACTEMENT le même calcul avec un couple (re, im) : le produit
# complexe général (a+bi)(c+di), pas une formule « wi = 2*zr*zi » qui
# ferait moins de travail que Rose.
N = 1_000_000
total_re = 0.0
total_im = 0.0
for k in range(1, N + 1):
    z_re = k * 0.000001
    z_im = 1.0 - z_re
    # $z_im * i  avec i = 0 + 1i
    zi_re = z_im * 0.0
    zi_im = z_im * 1.0
    # $z = $z_re + $z_im * i   (réel + complexe)
    z_re = z_re + zi_re
    z_im = 0.0 + zi_im
    # $w = $z * $z   produit complexe général
    w_re = z_re * z_re - z_im * z_im
    p = z_re * z_im
    w_im = p + p
    total_re += w_re
    total_im += w_im
print(total_re)
print(total_im)
