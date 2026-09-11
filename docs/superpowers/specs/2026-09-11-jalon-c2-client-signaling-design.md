# Jalon C2 — le client de signaling

**Date :** 11/09/2026
**Dépôts touchés :** `D:\skyshare` (principal) et `EriniumGroupWebsite` (site)
**Spec amont :** `2026-08-22-skyshare-architecture-design.md` (décision D2, §5.1)
**Jalon précédent :** C1 — l'API de signaling, en production depuis le 04/09/2026

---

## 1. Objectif et critère de réussite

**Deux machines établissent une connexion pair-à-pair sans aucun copier-coller.**

Pas « l'enveloppe est arrivée et s'est ouverte » : la connexion réelle, celle que le
jalon 0 a mesurée à 0,4 s. Ce critère est retenu parce que le code de connexion existe
déjà et fonctionne — s'arrêter à l'enveloppe livrerait une plomberie que personne n'aurait
branchée, défaut que ce projet a produit quatre fois.

Le jalon n'est **pas** clos sur les tests automatisés seuls. Il exige un essai réel entre
deux machines, deux réseaux et deux comptes Discord distincts (§9).

---

## 2. Ce qui existe déjà

### Côté site — livré par C1, en production

- 12 routes `/api/sky/*`, 17 tables, 338 tests.
- La synchronisation rend, pour chaque ami au statut `acceptee`, ses appareils non
  révoqués sous la forme `{ id, public_key }` — et rien d'autre.
- Boîte aux lettres : dépôt et relève d'enveloppes, **4096 octets** par charge
  (`envelopes_taille`), purge des expirées déjà en place.
- Flux natif complet : `/api/auth/discord?port=P&empreinte=<hex>` → `state` **signé** →
  Discord → `/api/auth/callback` → code à usage unique → `http://127.0.0.1:P/?code=<c>` →
  `POST /api/auth/native { code, secret }` → jetons.

### Côté application — le spike du jalon 0

Workspace de cinq crates, ~5 200 lignes, Windows uniquement, **entièrement synchrone** :
`sky-capture`, `sky-encode`, `sky-net` (str0m 0.23), `sky-crypto`, `sky-probe` (CLI clap).

`sky-crypto` expose `Identity` : `generate`, `public_key() -> [u8; 32]`, `seal`, `open`.
Boîte scellée NaCl via `crypto_box`, surcoût **48 octets**, avec un test qui prouve la
garantie centrale — un tiers ne peut pas ouvrir.

`sky-net::handshake` empaquette, comprime (deflate) puis encode le bloc de signaling. Un
test mesure le bloc réel : **moins de 1200 caractères** en base64, soit ~900 octets bruts.
Il porte un identifiant de session de 4 octets, tiré par l'offre et recopié par la réponse.

**Ce qui manque entièrement :** client HTTP, serveur de boucle locale, accès au coffre-fort
du système. Le spike est purement local.

---

## 3. Décisions

> **Numérotation.** Les décisions **de ce jalon** sont D1 à D8. Toute référence à une
> décision d'une autre spec est explicitement qualifiée — « la décision D2 **de la spec
> d'architecture** ». Les deux numéros D2 désignent des choses sans rapport : ici la
> cadence d'interrogation, là-bas le sens de l'échange.

### D1 — Un CLI, et un crate `sky-compte` réutilisable

C2 étend `sky-probe` avec des sous-commandes. **Aucune interface graphique.**

Toute la logique vit dans un crate nouveau, `sky-compte`, qui ne sait rien du terminal :
ni `println!`, ni lecture d'entrée standard. Il expose des fonctions et rend des erreurs
typées. `sky-probe` ne fait qu'appeler et afficher.

*Pourquoi :* le seul consommateur de C2 est le jalon 2, qui a besoin que deux machines se
trouvent — pas d'un bouton. Une interface construite maintenant habillerait une plomberie
qu'on ne saura juger qu'une fois la vidéo branchée.

*Coût si faux :* quand l'interface arrivera, il faudra en extraire la logique. La
frontière `sky-compte` / `sky-probe` ramène ce coût près de zéro.

### D2 — Disponibilité explicite, pas d'interrogation permanente

L'hôte se déclare disponible (`sky host`). Son application interroge alors la boîte toutes
les **2 secondes** pendant une fenêtre de **30 minutes**, au terme de laquelle elle
s'arrête en le disant ; relancer la commande ouvre une nouvelle fenêtre. Le spectateur
interroge à la même cadence de **2 secondes**, pendant **60 secondes** au plus.
**Au repos, aucune des deux n'interroge.**

Les trois valeurs — 2 s, 30 min, 60 s — sont des points de départ argumentés, pas des
mesures : 2 s est sous le seuil où une attente se remarque, 30 min couvre une session de
jeu sans courir indéfiniment, 60 s suffit largement à un hôte déjà disponible. Elles sont
regroupées en constantes nommées d'un seul endroit, pour être ajustées après le premier
essai réel plutôt que devinées deux fois.

Rien n'est publié côté serveur : la disponibilité est purement locale. C'est cohérent avec
C1, qui a délibérément écarté la table `shares` faute de code pour la remplir.

*Pourquoi :* `docs/mesures-jalon-c1.md` recommande 10 à 15 minutes pour la synchronisation
de fond, ce qui laisse Neon dormir — mais à cette cadence un spectateur attendrait un quart
d'heure. La disponibilité explicite correspond au produit : on ne reçoit pas un partage
d'écran par surprise, on décide de partager. Coût indépendant du nombre d'utilisateurs
inactifs.

*Coût si faux :* si l'usage réel est « je laisse SkyShare ouvert et mes amis me rejoignent
quand ils veulent », il faudra une notification côté serveur — sujet du jalon 2 avec les
tables `shares` et `join_requests`.

### D3 — Le second facteur reste dans le navigateur

Aujourd'hui le flux natif **refuse** un compte protégé : redirection vers
`http://127.0.0.1:P/?error=totp_requis`. C2 remplace ce refus par le parcours normal — le
site crée la session partielle, envoie le navigateur sur sa page TOTP existante, et n'émet
le code à usage unique **qu'après** vérification.

L'application ne change pas : elle attend un code sur sa boucle locale, elle en reçoit un.

*Pourquoi :* le flux natif passe déjà par le navigateur, où le second facteur est
configuré. Le faire saisir dans l'application installerait l'habitude de taper son second
facteur ailleurs que sur le site.

*Coût si faux :* aucun scénario identifié où cette voie est inférieure. Le risque n'est pas
dans le choix, il est dans sa mise en œuvre — voir D4.

### D4 — Le `state` signé traverse l'étape TOTP, jamais le port

Le couple (port, empreinte) n'est **jamais** transporté en clair ni reconstruit après la
vérification. C'est le `state` **signé** lui-même qui traverse l'étape, et il est revalidé
par `lireStateNatif` au bout du parcours.

*Pourquoi :* reposer le port dans une URL après le second facteur reconstruirait exactement
la faille de vol de session corrigée le 23/08. Réutiliser la même fonction de validation
aux deux bouts évite une seconde implémentation qui divergerait.

*Coût si faux :* c'est le point le plus dangereux du jalon. Ce flux a déjà été cassé deux
fois — une faille en production, et un contournement du second facteur dans la spec de C1
rattrapé par un relecteur. Trois garanties doivent être prouvées par neutralisation (§5.3).

### D5 — L'inversion des rôles : c'est le spectateur qui produit l'offre

Le spike fait produire l'offre par l'hôte. La décision D2 de la spec d'architecture,
tranchée le 23/08 après les mesures du jalon 0, **inverse ce sens** — et cette spec vise le
spike nommément : « ce que cela garantit, et que le spike ne garantissait pas ».

C2 échange les rôles dans `cmd_host` et `cmd_view`.

*Pourquoi :* un hôte qui produit l'offre publie ses adresses avant de savoir à qui. En mode
public il n'a aucune clé à laquelle sceller : il les diffuserait à la cantonade — mesuré
sur le spike, deux adresses de l'émetteur lisibles en clair. Inversé, l'hôte ne publie que
son identité et sa clé publique. **Rien qui le localise.** La promesse « un inconnu
n'obtient rien tant que l'hôte n'a pas approuvé » devient structurelle.

*Coût si faux, et assumé par la spec amont :* le spectateur expose ses adresses avant
d'être accepté. Symétrique — c'est lui qui demande, c'est scellé, et seul l'hôte qu'il a
choisi de contacter peut lire.

*Coût d'implémentation :* réel. Ce n'est pas un remplacement de ligne, c'est un échange de
rôles dans les deux commandes.

### D6 — Pas de runtime asynchrone

`ureq` pour les appels HTTP, `tiny_http` pour le serveur de boucle locale. Pas de `tokio`.

*Pourquoi :* le workspace est entièrement synchrone et `str0m` est sans-IO. C2 fait quelques
appels HTTP et ouvre un serveur éphémère à usage unique.

*Coût si faux :* si le jalon 2 impose un runtime asynchrone, `sky-compte` devra être
adapté. Sa surface est petite et sans état partagé, ce qui rend la bascule mécanique.

### D7 — Coffre-fort via `keyring`, un appareil par installation

La clé privée, le jeton de session et le jeton de renouvellement vont dans le gestionnaire
d'identifiants du système via la crate `keyring`.

La clé est générée au premier lancement et ne bouge plus. Coffre perdu : on enregistre un
nouvel appareil et on révoque l'ancien — l'API sait déjà le faire.

*Pourquoi :* `keyring` enveloppe le gestionnaire Windows, le trousseau macOS et Secret
Service sous une seule interface. Le jalon 7 vise Mac et Linux.

*Coût si faux :* si `keyring` se révèle inadapté sur une plateforme, il est remplacé
derrière le module `coffre`, qui est la seule frontière à connaître le sujet.

### D8 — Tests à deux niveaux

Un **serveur double** local implémentant les douze routes, construit à partir des tests du
site et non d'une compréhension de l'API. Plus un **essai réel** entre deux personnes.

*Pourquoi :* la base de production contient 11 comptes Discord réels ; les tests
automatisés ne doivent jamais l'atteindre, et l'application ne peut pas fabriquer de compte
Discord jetable. Le double prouve le code, l'essai réel prouve le système.

*Coût si faux :* un double qui répond ce que l'application espère plutôt que ce que le vrai
serveur répond donnerait une confiance imméritée — exactement la classe de défaut « tests
verts qui ne mesurent rien ». D'où l'exigence de le dériver des tests du site.

---

## 4. Architecture

### `sky-compte` — nouveau

| Module | Responsabilité |
|---|---|
| `session` | Connexion native, code à usage unique, jetons, renouvellement |
| `coffre` | Clé privée et jetons dans le gestionnaire d'identifiants du système |
| `annuaire` | Amis, listes, appareils ; la synchronisation |
| `boite` | Dépôt et relève d'enveloppes ; scellage délégué à `sky-crypto` |

L'URL de base de l'API est un **réglage**, pas une constante — c'est ce qui rend le serveur
double possible sans code de test dans le code de production.

### `sky-probe` — touché

Nouvelles sous-commandes, qui ne font qu'appeler `sky-compte` et afficher :
`login`, `device register`, `device list`, `friends add <code>`, `friends accept <id>`,
`friends list`, `code`, `host`, `view <ami>`.

`<ami>` se désigne par le **nom Discord** ou par l'**identifiant** affiché par
`friends list`. Deux amis portant le même nom : la commande **refuse et demande
l'identifiant**, plutôt que d'en choisir un. Se tromper de destinataire, ici, veut dire
sceller ses adresses pour la mauvaise personne.

`cmd_host` et `cmd_view` : suppression des deux `stdin().read_line`, et **échange des
rôles** (D5).

### `sky-crypto` — touché à peine

`Identity` est aujourd'hui éphémère, avec un commentaire qui annonçait ce jalon :
« au jalon 1 elle ira dans le coffre-fort du système ». Ajout de la sérialisation de la clé
privée pour que `coffre` puisse la ranger. Le reste est inchangé.

---

## 5. La connexion native

### 5.1 Le parcours

1. L'application tire un `secret` au hasard et en calcule l'empreinte SHA-256.
2. Elle ouvre un serveur sur `127.0.0.1:P` — **P est son choix**, tout port libre convient.
   Le port n'est figé nulle part : le site accepte tout entier de 1 à 65535.
3. Elle ouvre le navigateur sur `/api/auth/discord?port=P&empreinte=<hex>`.
4. Le site signe (port, empreinte) dans le `state` et renvoie vers Discord.
5. Au retour, le site revalide la signature, émet un code à usage unique, et redirige vers
   `http://127.0.0.1:P/?code=<code>`.
6. L'application ferme son serveur et poste `{ code, secret }` sur `/api/auth/native`.
7. Elle reçoit ses jetons et les range dans le coffre.

Trois propriétés déjà en place font la solidité de ce flux : le port voyage **dans** la
signature ; l'URL de redirection ne porte **jamais de jeton**, seulement un code ; et
l'échange exige le secret, que seule l'application ayant lancé le flux détient.

### 5.2 L'étape du second facteur (D3)

Si le compte est protégé, le site crée la session partielle — celle que les douze routes
rejettent toutes — et envoie le navigateur sur sa page TOTP. Après vérification, il émet le
code à usage unique et redirige vers la boucle locale.

### 5.3 Les trois garanties à prouver par neutralisation

Chacune a son test, et chaque test doit rougir quand on retire la garde — **pour la bonne
raison et elle seule** :

1. **`emettreCode` n'est atteignable qu'après un second facteur vérifié.** Le test rejoue
   l'attaque que le relecteur de C1 avait reproduite (« session pleine obtenue sans second
   facteur »).
2. **Un `state` non signé, ou signé pour un autre port, n'aboutit à aucune redirection vers
   la boucle locale.**
3. **La session partielle ne donne accès à rien** sur les douze routes.

---

## 6. Le rendez-vous et l'échange

1. **Bob lance `sky host`.** Rien n'est publié côté serveur. Son application interroge la
   boîte toutes les 2 secondes pendant une fenêtre bornée.
2. **Alice lance `sky view bob`.** Sa synchronisation lui donne les appareils de Bob avec
   leurs clés publiques. Elle produit **son** offre, la scelle pour la clé de Bob, la
   dépose — sur **tous** les appareils non révoqués de Bob, qui sont peu nombreux et sont
   tous les siens.
3. **Bob relève**, ouvre — lui seul le peut — et y trouve l'offre *et* la clé publique
   d'Alice.
4. **Bob produit sa réponse**, la scelle pour Alice, la dépose.
5. **Alice relève**, ouvre, et la connexion s'établit.

Deux enveloppes, ~900 octets chacune contre 4096 autorisés.

**Règles à ne pas inverser :**

- **La compression précède toujours le scellement.** Un contenu chiffré est indistinguable
  du hasard et ne se comprime pas. Déjà vrai dans `handshake.rs`.
- **L'identifiant de session de 4 octets est conservé.** Tiré par l'offre, recopié par la
  réponse. Sans lui, une réponse d'un essai précédent produit « mauvaise clé ou message
  altéré » — un message qui envoie chercher un problème de chiffrement inexistant.

**Bob n'est pas disponible :** Alice interroge une minute, puis dit franchement que Bob n'a
pas répondu. L'enveloppe expire seule ; la purge existe déjà.

**Dette de C1 repliée ici :** `sontAmis` et `proprietaireDe` n'ont aucun appelant en
production — les routes réécrivent le prédicat sur place. C2 exerce ces routes pour de vrai,
donc une régression se verrait. Quatrième occurrence dans ce projet d'une protection
construite sans être branchée.

---

## 7. Erreurs

- **Le 401 indistinct.** `/api/auth/native` répond « Code refusé » sans distinguer code
  inconnu, expiré, déjà consommé ou mauvais secret. L'application affiche **un seul
  message** — sinon elle réintroduit côté client la distinction que le serveur a refusé de
  faire.
- **La latence de réveil n'est pas une panne.** Neon dort après 5 minutes ; le réveil a été
  mesuré à **748,8 ms** contre ~35 ms à chaud. Les délais d'attente sont dimensionnés en
  conséquence, et un commentaire dit pourquoi — pour que personne ne les « optimise ».
- **Le renouvellement de jeton.** La route existe côté site et a échoué pour 100 % des
  tentatives réelles pendant tout un jalon, faute d'appelant. C2 en est le premier appelant
  réel : le test l'exerce **par le vrai chemin**, jamais avec une charge fabriquée à la main
  — c'est précisément la charge fabriquée à la main qui était correcte, et celle du vrai
  chemin qui ne l'était pas.

---

## 8. Ce que C2 change côté site

- `/api/auth/callback` : la branche native ne refuse plus les comptes protégés (D3).
- La page TOTP et sa route de vérification : accueillir le parcours natif en portant le
  `state` signé (D4).
- Câblage de `sontAmis` et `proprietaireDe` dans les routes qui réécrivent leur prédicat.
- Les tests correspondants, dont les trois neutralisations du §5.3.

Cela implique une poussée sur `main`, donc un déploiement en production, avec l'accord
explicite du propriétaire.

---

## 9. Tests

**Niveau 1 — le serveur double.** Implémente les douze routes avec leurs codes de retour
réels, y compris le 401 indistinct et le 404 du bloqué. Construit **à partir des tests du
site**. Les tests automatisés tournent contre lui : rapides, déterministes, zéro contact
avec la production.

**Niveau 2 — l'essai réel.** Deux machines, deux réseaux, deux comptes Discord distincts.
Le double prouve le code ; seul l'essai réel prouve le système. **C2 n'est pas clos sans
lui.**

**Interdits absolus :** aucun test automatisé ne touche la base de production. Les 11
comptes Discord réels et leurs 5 sessions ne sont ni lus ni modifiés par le code de test.

---

## 10. Hors périmètre

Aucune interface graphique. Aucun changement à la chaîne vidéo. Aucune décision sur
l'écart 7 (`str0m` contre `webrtc-rs`) — C2 transporte une offre, son contenu reste le
problème du jalon 2. Pas de mode public ni de salle d'attente : ils attendent les tables
`shares` et `join_requests`, délibérément écartées par C1. Pas de gestion multi-comptes.
Pas de repli logiciel x264.
