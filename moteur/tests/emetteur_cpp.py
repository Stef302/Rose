#!/usr/bin/env python3
"""Porte M4.4 : emetteur_cpp.rose (RIR -> C++) ≡ transpiler_rir (rose build).

Usage : python3 moteur/tests/emetteur_cpp.py [--n N]
"""
import subprocess, os, sys, tempfile, glob

ROSE = os.path.join(os.path.dirname(__file__), '..', '..', 'moteur', 'cpp', 'rose')
# fallback: use the built binary
if not os.path.exists(ROSE):
    ROSE = os.path.join(os.path.dirname(__file__), '..', '..', 'bin', 'rose')
EMETTEUR = os.path.join(os.path.dirname(__file__), '..', '..', 'rose', 'emetteur_cpp.rose')
GMP = os.environ.get('ROSE_GMP', '/tmp/opencode/gmp-linux')

def run(cmd, **kw):
    env = dict(os.environ, ROSE_GMP=GMP)
    return subprocess.run(cmd, capture_output=True, text=True, env=env, **kw)

def test_program(src, name):
    d = tempfile.mkdtemp(prefix='ecpp_')
    rose_file = os.path.join(d, name + '.rose')
    open(rose_file, 'w').write(src)
    # oracle : rose build -> .cpp
    r = run([ROSE, 'build', rose_file, '-o', os.path.join(d, name + '.bin')])
    cpp_file = os.path.join(d, name + '.bin.cpp')
    if not os.path.exists(cpp_file):
        return False, 'pas de .cpp produit par rose build'
    oracle_cpp = open(cpp_file).read()
    # RIR
    r2 = run([ROSE, 'rir', rose_file])
    rir_file = os.path.join(d, name + '.rir')
    open(rir_file, 'w').write(r2.stdout)
    # emetteur_cpp.rose
    r3 = run([ROSE, EMETTEUR, rir_file])
    if r3.returncode != 0:
        return False, 'emetteur_cpp rc=%d: %s' % (r3.returncode, r3.stderr.strip()[:200])
    emitted = r3.stdout
    if emitted != oracle_cpp:
        # find first differing line
        ol = oracle_cpp.splitlines()
        el = emitted.splitlines()
        for i, (a, b) in enumerate(zip(ol, el)):
            if a != b:
                diff = 'ligne %d:\n  oracle: %s\n  emis  : %s' % (i+1, a, b)
                break
        else:
            diff = 'longueurs differentes: oracle=%d emis=%d' % (len(ol), len(el))
        return False, diff
    return True, 'identique (%d octets)' % len(oracle_cpp)

# Programmes de test V1
PROGRAMS = {
    'hello': '''fn addition($a: entier, $b: entier) -> entier {
  retour $a + $b
}

>> addition(3, 4)
''',
    'arith': '''$x = 10
$y = 3
>> $x + $y
>> $x - $y
>> $x * $y
''',
    'func': '''fn double($n: entier) -> entier {
  retour $n * 2
}

>> double(5)
>> double(double(3))
''',
    'affiche': '''>> 42
>> "bonjour"
''',
    'submul': '''$a = 10
$b = 3
>> $a - $b
>> $a * $b
>> 0 - $a
''',
    'si_simple': '''$x = 5
si ($x > 3) {
  >> "grand"
} sinon {
  >> "petit"
}
''',
    'boucle': '''$i = 0
tant_que ($i < 5) {
  >> $i
  $i = $i + 1
}
''',
    'pour': '''pour $k de 0 vers 5 {
  >> $k
}
''',
    'str_idx': '''$s = "abc"
>> $s[0]
>> $s[1:2]
>> longueur($s)
''',
    'liste': '''$l = [1, 2, 3]
>> longueur($l)
joindre($l, 4)
>> $l[0]
''',
    'cmp': '''$a = 5
$b = 3
si ($a == $b) { >> "eq" } sinon { >> "ne" }
si ($a > $b) { >> "gt" }
si ($a <= $b) { >> "le" } sinon { >> "nle" }
''',
    'arith2': '''>> 7 div 2
>> 7 % 2
>> 2 ^ 3
''',
    'grand': '''$x = 9223372036854775807
>> $x
>> $x + 1
''',
    'floats': '''$x = 1.5
$y = 2.0
>> $x + $y
>> $x * $y
>> $x / $y
''',
    'str_ops': '''$s = "hello"
>> sous_chaine($s, 1, 3)
>> longueur($s)
>> ordre("a")
>> caractere(65)
''',
    'neg': '''$x = 5
>> 0 - $x
>> non vrai
>> vrai et faux
>> vrai ou faux
''',
    'nested': '''fn carre($n: entier) -> entier {
  retour $n * $n
}

fn somme_carres($a: entier, $b: entier) -> entier {
  retour carre($a) + carre($b)
}

>> somme_carres(3, 4)
''',
    'br_cond': '''$x = 0
tant_que ($x < 3) {
  si ($x == 1) {
    >> "un"
  } sinon {
    >> "autre"
  }
  $x = $x + 1
}
''',
    'rationnel': '''>> 1 / 3
>> 1 / 3 + 1 / 3
>> 1 / 3 + 1 / 3 + 1 / 3
>> 7 / 2
''',
    'rat_arith': '''$a = 1 / 2
$b = 1 / 3
>> $a + $b
>> $a - $b
>> $a * $b
>> $a / $b
''',
    'gd2': '''$y = 2 ^ 64
$z = $y * 3 - 1
>> $z
''',
    'gd_func': '''fn double($n: entier) -> entier {
  retour $n * 2
}

$g = 4611686018427387904
>> double($g)
''',
    'fparam': '''fn plus2($x: flottant) -> flottant {
  retour $x + 1.5
}

>> plus2(2)
>> plus2(2.25)
''',
    'conv': '''$i = entier("123")
>> $i
$f = flottant("2.5")
>> $f
>> entier(3.75)
>> flottant(7)
>> chaine(42)
''',
    'minmax': '''>> min(3, 7)
>> max(2, 1)
>> min(2.5, 1)
>> abs(0 - 5)
''',
    'gd_builtin': '''$x = factorielle(30)
>> $x
$y = fibonacci(90)
>> $y
>> 2 ^ 100
''',
    'tampon': '''$b = tampon(8)
tampon_pose($b, 0, 65)
>> tampon_case($b, 0)
>> longueur($b)
$a = octets("abc")
>> longueur($a)
''',
    'flagpees': '''$g = vrai
si ($g) { >> 1 }
$w = 42
>> $w
''',
    'recursif': '''fn fact($n: entier) -> entier {
  si ($n <= 1) { retour 1 }
  retour $n * fact($n - 1)
}

>> fact(5)
''',
    'remutuel': '''fn pair($n: entier) -> booleen {
  si ($n == 0) { retour vrai }
  retour impair($n - 1)
}

fn impair($n: entier) -> booleen {
  si ($n == 0) { retour faux }
  retour pair($n - 1)
}

>> pair(10)
''',
    'listeref': '''fn remplir($v: liste<chaine>) {
  joindre($v, "a")
  joindre($v, "b")
}

$v = ["x"]
remplir($v)
>> longueur($v)
>> $v[3]
''',
    'table': '''$t = table()
inserer($t, "un", 1)
inserer($t, "deux", 2)
>> $t["deux"]
>> cle_existe($t, "zz")
>> longueur($t)
>> $t
$k = cles($t)
>> longueur($k)
>> supprimer($t, "un")
>> longueur($t)
''',
}

def main():
    n = 0; ok = 0; fail = 0
    for name, src in PROGRAMS.items():
        good, msg = test_program(src, name)
        n += 1
        if good:
            ok += 1
            print('OK   %s : %s' % (name, msg))
        else:
            fail += 1
            print('FAIL %s : %s' % (name, msg))
    print('\n%d/%d programmes identiques' % (ok, n))
    sys.exit(0 if fail == 0 else 1)

if __name__ == '__main__':
    main()

# patch PROGRAMS to add rational arithmetic test
