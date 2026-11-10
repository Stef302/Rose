# Banc Python : même calcul complexe que complexe.rose (1 000 000 itérations)
# z = re + im*i ; w = z*z ; accumulateur sur re(w) et im(w)
N = 1_000_000
total_re = 0.0
total_im = 0.0
for k in range(1, N + 1):
    z_re = k * 0.000001
    z_im = 1.0 - z_re
    w_re = z_re * z_re - z_im * z_im
    w_im = 2.0 * z_re * z_im
    total_re += w_re
    total_im += w_im
print(total_re)
print(total_im)
