# Point fixe M4.5 — valeur courante (19/11/2026, fin de journée)

**Valeur active** : `8ac53eed3d4967a0d7d9ab333256801c`

Identique sur les deux voies :
```
./rose/rose rir compilateur/emetteur.rose
./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose
```

## Pourquoi la valeur a changé

L'ancienne valeur `bdbef63681608a0b22a5637539e9d738` était correcte au
moment où elle a été mesurée. Elle a été remplacée parce que
`compilateur/emetteur.rose` (la source de l'émetteur) a été modifiée le
19/11 pour ajouter deux règles manquantes aux compilateurs Rose :

- `racine(n)` est typée `irrationnel` mais rend un entier exact sur carré
  parfait — l'addition `entier + irrationnel` est résolue à l'exécution ;
- la primitive `racine_decimales(n,d)` a été ajoutée à la table `std_sig`.

Le point fixe est une **propriété d'idempotence** (compiler l'émetteur avec
lui-même redonne le même dump), pas une constante immuable : toute modification
de la source de l'émetteur en change la valeur. Le document
`SELF_HOSTED_VERDIT_FINAL.md` et les notes de release datées citent
l'ancienne valeur avec leur date — ce sont des constats historiques, pas des
valeurs à réécrire.

## Vérification

```bash
cd /home/clausake/Stefan/ROSE
./rose/rose rir compilateur/emetteur.rose > /tmp/a
./rose/rose compilateur/emetteur.rose compilateur/emetteur.rose > /tmp/b
md5sum /tmp/a /tmp/b
# 8ac53eed3d4967a0d7d9ab333256801c  /tmp/a
# 8ac53eed3d4967a0d7d9ab333256801c  /tmp/b
```

Constat daté, pas consigne. Re-mesurer après tout changement de
`compilateur/emetteur.rose`.
