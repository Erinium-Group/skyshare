# Jalon C — Signaling : amis, listes, boîte aux lettres chiffrée

**Date :** 1er septembre 2026
**Périmètre :** jalon C de la refonte EriniumGroup = **jalon 1 de SkyShare**
**Specs parentes :**
`docs/superpowers/specs/2026-08-22-skyshare-architecture-design.md` (SkyShare)
`D:\Mods Minecraft\EriniumGroupWebsite\docs\superpowers\specs\2026-08-23-refonte-eriniumgroup-design.md` (site)

---

## 1. Objectif

Donner à SkyShare une identité, des amis et un canal de rencontre — **sans aucune
vidéo**.

À la fin du jalon, deux installations de l'application peuvent se connecter à Discord,
devenir amies, et **échanger une enveloppe scellée à travers le site** sans que celui-ci
puisse en lire le contenu. C'est la fondation sur laquelle le jalon 2 posera le premier
pixel.

Ce que le jalon C remplace : le prototype du jalon 0 exigeait de se copier-coller un bloc
de 700 caractères dans Discord. Après ce jalon, l'application sait à qui elle parle et
comment lui écrire.

### Ce que le jalon C ne fait pas

| Écarté | Pourquoi |
|--------|----------|
| Toute vidéo, capture, encodage | Jalon 2. Le spec SkyShare §9 pose « jalon 1 : aucune vidéo ». |
| Les tables `shares` et `join_requests` | Rien ne les remplirait. Voir décision D3. |
| Le mode public et les liens `/s/CODE` | Jalon 5 de SkyShare. |
| La salle d'attente | Jalon 5. |
| Le pair-à-pair, le perçage de NAT | Jalon 2. Le code existe et est validé (`sky-net`). |

---

## 2. Décisions

### D1 — Le nommage des tables suit la spec du site

Les deux specs parentes se contredisent. La spec SkyShare (22/08) prévoit huit tables
préfixées `sky_`, dont `sky_users`. La spec du site (23/08) en prévoit sept sans préfixe,
et réutilise la table `users` existante.

**La spec du site fait foi.** Elle est postérieure, et sa logique est vérifiable :
l'identité a été unifiée sur `users` lors du jalon A, et `sky_sessions` serait entré en
collision de sens avec la table `sessions` des sessions web — d'où son renommage en
`shares`.

*Coût si faux :* un renommage de tables avant qu'elles ne portent des données.

### D2 — Cinq tables, pas sept

`shares` et `join_requests` ne sont **pas créées** à ce jalon. Aucun code ne les
remplirait : il n'y a ni partage ni salle d'attente sans vidéo.

Le jalon A a établi ce que coûte un schéma écrit pour un comportement inexistant : quatre
tables y avaient été décrites telles qu'imaginées, et comme les instructions étaient en
`CREATE TABLE IF NOT EXISTS`, elles ne produisaient aucune erreur — le fichier documentait
une fiction, et une base neuve aurait obtenu une structure différente de la production.

**Ces deux tables arriveront au jalon 2, avec le code qui les remplit.**

*Coût si faux :* une migration de plus au jalon 2, additive.

### D3 — L'application partage le système de sessions du site

L'application reçoit un jeton de 7 jours et un jeton de renouvellement de 30 jours. Elle
renouvelle en silence. **Révoquer un appareil revient à révoquer sa ligne de session**,
celle que `getSession()` vérifie déjà à chaque requête.

Le rejet de l'alternative — un jeton d'appareil de longue durée — est le même argument
qu'à la tâche 4 du jalon A, où un second lecteur de session a été refusé : deux systèmes
d'authentification donneraient deux vérités sur qui est connecté, dont une seule
connaîtrait la révocation.

**Conséquence à traiter :** aujourd'hui `signRefreshToken` est **identique** à `signJWT`
— même durée de 30 jours, même charge. Le jalon C doit les différencier réellement : le
jeton de renouvellement dure plus longtemps, ne donne accès à aucune route de données, et
n'est utilisable que sur la route de renouvellement.

*Coût si faux :* une reconnexion Discord hebdomadaire pour les utilisateurs.

### D4 — On n'ajoute un ami que par code

L'ajout par pseudo Discord exact, prévu au §4.4 de la spec SkyShare, est **retiré**. Il
permet à quiconque de tester si une personne donnée utilise SkyShare : la réponse
« trouvé / pas trouvé » est l'information.

Sur un produit dont l'argument central est que personne ne voit rien, cette fuite détonne.
L'atténuation habituelle — une limitation de débit — n'est pas disponible : la spec du site
§4.7 établit que le compteur actuel est **inopérant sur Vercel**, et que compter dans
Postgres coûterait plus au quota qu'il ne protégerait.

**Le code fait huit caractères.** L'entropie tient lieu de limite de débit : environ
mille milliards de combinaisons, soit un balayage hors de portée à tout rythme de requêtes
réaliste. Le code est régénérable à tout moment.

*Coût si faux :* il faut connaître le code de quelqu'un pour l'ajouter — ce qui est le
comportement voulu, puisque l'usage réel passe par Discord où l'on se parle déjà.

### D5 — Le cœur Rust est propriétaire de l'état

Rust détient le jeton, la clé privée, la boucle de synchronisation et tous les appels
réseau. Le webview affiche et transmet des intentions.

Deux raisons. **Sécurité :** si le frontend fait les requêtes HTTP, le jeton doit traverser
vers JavaScript à chaque appel, et la garantie du coffre-fort système devient décorative.
**Coût évité :** au jalon 2, Rust devient propriétaire du réseau de toute façon — une
synchronisation écrite en JavaScript serait à réécrire.

*Coût si faux :* une douzaine de commandes Tauri à écrire là où un `fetch` aurait suffi.

### D6 — Le prototype est promu, pas gelé

`spike/crates/` devient `crates/` à la racine. `sky-crypto` et `sky-net` gardent leur code
validé et leur historique git. Les artefacts jetables — fichiers vidéo de test, sorties de
mesure, binaires — sont supprimés. `spike/docs/rapport-jalon-0.md` est conservé comme
archive.

*Coût si faux :* du code prouvé serait dupliqué plutôt que déplacé.

---

## 3. Les frontières

Trois acteurs, et ce qui traverse. Tout le reste en découle.

**Le site possède l'identité et la boîte aux lettres.** Il sait qui est ami avec qui,
stocke les clés publiques, transporte les enveloppes scellées. Il ne peut **jamais** lire
une adresse : les enveloppes lui sont opaques et il ne détient aucune clé privée.

**Le cœur Rust possède tout ce qui est secret et tout ce qui parle au réseau.**

**Le webview ne possède rien.** Il affiche, transmet une intention, reçoit un état
calculé.

| Frontière | Ce qui passe | Ce qui ne passe jamais |
|---|---|---|
| Rust → site | Jeton en en-tête, enveloppes scellées | Une adresse en clair |
| Rust → webview | État affichable | Le jeton, la clé privée |
| Site → Rust | Réponses de synchronisation, enveloppes | — |

**Le webview est supposé compromis.** Une page hostile qui s'y exécuterait ne doit pouvoir
ni voler le jeton ni signer quoi que ce soit ; elle ne peut que demander des actions à
Rust, qui décide. C'est ce qui rend la commande Tauri préférable à un `fetch`.

**Conséquence assumée :** le webview ne peut afficher ni la clé publique ni le code ami
sans que Rust les lui transmette explicitement. Chaque donnée qui traverse est un choix.

---

## 4. Schéma de base

Cinq tables nouvelles, plus une colonne sur `users`. Migration additive, appliquée par
`npm run db:migrate` — **jamais au démarrage de l'application**, acquis du jalon A.

### 4.1 `users` — une colonne ajoutée

```sql
ALTER TABLE users ADD COLUMN IF NOT EXISTS friend_code TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS users_friend_code_idx ON users(friend_code);
```

Huit caractères, alphabet sans ambiguïté visuelle (ni `O`/`0`, ni `I`/`1`/`l`), présenté
par groupes de quatre : `SKY-7F2A-9K3M`. Régénérable.

`users` porte onze comptes Discord réels : **ALTER uniquement, jamais de recréation.**

### 4.2 `devices`

| Colonne | Type | Rôle |
|---|---|---|
| `id` | SERIAL PK | |
| `user_id` | INTEGER NOT NULL → `users(id)` ON DELETE CASCADE | propriétaire |
| `public_key` | TEXT NOT NULL | clé publique X25519, base64 |
| `nom` | TEXT NOT NULL | libellé lisible (« PC bureau ») |
| `plateforme` | TEXT NOT NULL | `windows` / `macos` / `linux` |
| `session_id` | TEXT → `sessions(id)` | la session de cet appareil |
| `created_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() | |
| `last_seen_at` | TIMESTAMPTZ | mise à jour à chaque synchronisation |
| `revoked_at` | TIMESTAMPTZ | révocation individuelle |

`session_id` rend concrète la décision D3 : révoquer un appareil révoque sa session.

### 4.3 `friendships`

| Colonne | Type |
|---|---|
| `id` | SERIAL PK |
| `user_bas` | INTEGER NOT NULL → `users(id)` ON DELETE CASCADE |
| `user_haut` | INTEGER NOT NULL → `users(id)` ON DELETE CASCADE |
| `demandeur_id` | INTEGER NOT NULL → `users(id)` ON DELETE CASCADE |
| `statut` | TEXT NOT NULL — `en_attente` / `acceptee` / `bloquee` |
| `created_at`, `updated_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() |

```sql
CHECK (user_bas < user_haut)      -- interdit l'auto-amitié ET impose le tri
UNIQUE (user_bas, user_haut)      -- interdit le doublon dans les deux sens
CHECK (statut IN ('en_attente', 'acceptee', 'bloquee'))
```

Le couple est trié, ce qui fait perdre le sens de la demande : `demandeur_id` le mémorise,
sans quoi on ne saurait pas qui doit accepter.

### 4.4 `friend_lists` et `list_members`

```
friend_lists : id PK, user_id → users, nom, couleur, emoji, created_at
               UNIQUE (user_id, nom)

list_members : list_id → friend_lists ON DELETE CASCADE,
               membre_id → users ON DELETE CASCADE
               PRIMARY KEY (list_id, membre_id)
```

Un ami peut appartenir à plusieurs listes. Les listes ne servent que de filtre au moment
de partager — donc, à ce jalon, elles se créent et se remplissent sans être encore
consommées.

### 4.5 `envelopes`

| Colonne | Type |
|---|---|
| `id` | BIGSERIAL PK |
| `expediteur_device_id` | INTEGER NOT NULL → `devices(id)` ON DELETE CASCADE |
| `destinataire_device_id` | INTEGER NOT NULL → `devices(id)` ON DELETE CASCADE |
| `charge` | BYTEA NOT NULL |
| `created_at` | TIMESTAMPTZ NOT NULL DEFAULT NOW() |
| `expire_at` | TIMESTAMPTZ NOT NULL |

```sql
CHECK (octet_length(charge) <= 4096)   -- la base ne peut pas devenir un stockage
CHECK (expediteur_device_id <> destinataire_device_id)
CREATE INDEX ON envelopes(destinataire_device_id, expire_at);
```

**Trois protections structurelles, portées par la base et non par le code :** taille
bornée, expiration obligatoire à l'insertion (5 minutes), et effacement à la lecture.

Le nettoyage des enveloppes expirées se fait **à chaque lecture**, dans la même requête.
Pas de tâche planifiée : le jalon A en a trouvé une, morte depuis des semaines, qui visait
une route supprimée.

**La table reste vide en régime normal.**

---

## 5. L'API de signaling

Treize routes sous `/api/sky/*`. Toutes exigent une session valide — vérifiée par
`getSession()`, l'unique point de passage établi au jalon A.

### 5.1 La synchronisation est un seul point d'entrée

```
GET /api/sky/sync?version=N
```

Renvoie l'état complet de l'utilisateur — amis acceptés, demandes reçues, listes,
appareils, enveloppes en attente — ou `{ "inchange": true }` en quelques octets si rien
n'a changé depuis la version demandée.

`version` est un entier monotone par utilisateur, incrémenté à chaque écriture qui le
concerne. Le calcul se fait à partir du plus grand `updated_at` de ses lignes ; l'objectif
est qu'une réponse « rien de neuf » coûte une requête légère, pas que la version soit un
compteur exact.

**C'est le rythme qui change, pas la route :**

| Contexte | Cadence |
|---|---|
| App au premier plan | 30 s |
| App en arrière-plan | 5 min |
| Négociation en cours | 500 ms, pendant ~3 s |
| Connexion établie | **zéro requête** |

Les enveloppes voyagent dans cette réponse, et sont **effacées** en même temps qu'elles
sont rendues. C'est ce qui permet la négociation sans route dédiée : l'application
accélère à 500 ms, ramasse, retombe.

### 5.2 Le dépôt d'enveloppe vérifie l'amitié côté serveur

```
POST /api/sky/envelopes    { destinataire_device_id, charge }
```

Le serveur vérifie que l'expéditeur et le propriétaire de l'appareil destinataire sont
**amis au statut `acceptee`**. Sinon : refus.

C'est la protection la plus importante de l'API. Sans elle, n'importe qui déposerait chez
n'importe qui, et découvrirait au passage qui utilise l'application. Aucun humain ne
regarde ces routes.

Contrôles complémentaires : taille rejetée avant d'atteindre la base, appareil
destinataire non révoqué, expiration posée par le serveur et non par le client.

### 5.3 Les autres routes

**Appareils**
`POST /api/sky/devices` — enregistrer une machine et sa clé publique, lier la session
`DELETE /api/sky/devices/:id` — révoquer l'appareil **et** sa session

**Amis**
`POST /api/sky/friends` — ajouter par code
`POST /api/sky/friends/:id/accept`
`DELETE /api/sky/friends/:id` — retirer ou refuser
`POST /api/sky/friends/:id/block`
`POST /api/sky/friend-code` — régénérer le sien

**Listes**
`POST /api/sky/lists` · `PATCH /api/sky/lists/:id` · `DELETE /api/sky/lists/:id`
`PUT /api/sky/lists/:id/members`

### 5.4 Limitation de débit

**Décision maintenue de la spec du site §4.7 : on ne compte pas dans Postgres.** Une
écriture par requête coûterait plus au quota Neon qu'elle ne protège, et le compteur en
mémoire est inopérant sur Vercel.

Ce qui protège réellement : l'amitié vérifiée, la taille bornée, l'expiration, et
l'entropie du code ami sur huit caractères. **À réexaminer si un abus réel est constaté**
— pas avant.

---

## 6. L'application

### 6.1 La connexion — refaire correctement ce qui a été supprimé

Le flux de connexion d'une application native est **celui qui a été retiré du site le
23 août pour raison de sécurité** : l'ancien launcher ouvrait un serveur local et le site
lui renvoyait les jetons dans l'URL. Le port n'étant pas validé,
`state=launcher:1234@evil.example` produisait une redirection vers l'hôte d'un attaquant,
jetons compris.

Le motif RFC 8252 n'est pas en cause ; sa mise en œuvre l'était.

```
1. L'app tire un secret aléatoire, calcule son empreinte SHA-256,
   ouvre un serveur éphémère sur 127.0.0.1:47821
2. Elle ouvre le navigateur système sur le site, avec un state SIGNÉ
   contenant le port ET l'empreinte
3. Le site vérifie la signature — un state forgé est refusé AVANT tout
   appel à Discord — puis procède à l'échange OAuth
4. Le site redirige vers 127.0.0.1:47821 avec un CODE À USAGE UNIQUE,
   jamais un jeton
5. L'app échange ce code contre les jetons par POST, en présentant son secret
6. L'app ferme le serveur éphémère et range les jetons dans le coffre-fort système
```

**Les jetons n'apparaissent jamais dans une URL.** Un autre logiciel qui gagnerait la
course au port local obtiendrait un code inéchangeable. Le port voyage dans la signature,
donc il n'est plus falsifiable.

La vérification de signature existe : `signerState()` / `verifierState()`, écrites au
jalon A et éprouvées jusqu'à en corriger deux contournements — un suffixe arbitraire, puis
une troncature silencieuse de `Buffer.from(x, "hex")` sur un caractère non apparié.

**Une application Discord dédiée n'est pas nécessaire.** L'URL de redirection déclarée
auprès de Discord reste celle du site ; la redirection vers la boucle locale est faite par
le site, pas par Discord. L'écran de consentement affichera « EriniumGroup », ce qui est
exact.

### 6.2 Identité cryptographique

À la première connexion, l'application génère une paire X25519. La clé publique part en
base ; **la clé privée ne quitte jamais la machine**. Une clé par appareil, révocable
individuellement.

Le jeton de session et la clé privée vivent dans le coffre-fort du système — Credential
Manager, Trousseau, Secret Service — jamais en clair sur disque.

### 6.3 Structure du dépôt

```
crates/sky-crypto     promu du prototype — scellage X25519
crates/sky-net        promu du prototype — ICE, STUN, perçage (dormant à ce jalon)
crates/sky-app        cœur : session, synchronisation, état, commandes Tauri
app/                  interface Tauri — React, Vite, Tailwind
spike/docs/           archive du jalon 0, conservée
```

Un seul espace de travail Cargo à la racine. L'interface reprend les jetons de couleur et
les polices du site : l'application ressemble au site sans effort supplémentaire.

### 6.4 Le cœur

Rust détient le jeton, la clé privée et l'état. La boucle de synchronisation tourne côté
Rust et **pousse un événement** vers le webview quand l'état change — l'interface écoute,
elle n'interroge pas en boucle.

Commandes exposées : `etat`, `connexion`, `deconnexion`, `ajouter_ami`, `accepter`,
`retirer`, `bloquer`, `regenerer_code`, `creer_liste`, `modifier_liste`, `supprimer_liste`,
`definir_membres`, `revoquer_appareil`, `rafraichir`.

### 6.5 Quatre écrans

**Connexion** — un bouton.
**Amis** — la liste, les demandes reçues, un champ pour coller un code.
**Listes** — créer, nommer, colorer, cocher des membres.
**Mon compte** — le code ami régénérable, les appareils, révocation à l'unité.

Le partage n'apparaît nulle part. Il n'existe pas encore, et une interface qui le
montrerait grisé serait une promesse de plus.

---

## 7. Vérification

### 7.1 Ce qui doit être prouvé

**Le flux de connexion, de façon adverse.** Quatre épreuves, chacune devant échouer contre
le code d'avant et passer après :

| Épreuve | Attendu |
|---|---|
| `state` forgé | refusé **avant** tout appel à Discord |
| port falsifié dans le `state` | refusé — il est couvert par la signature |
| code intercepté sur la boucle locale | inéchangeable sans le secret |
| code rejoué une seconde fois | refusé |

**Le dépôt chez un non-ami est refusé**, contre la vraie base, avec deux comptes dont un
jetable créé et supprimé pour l'occasion.

**Les contraintes de la base se testent au niveau de la base.** S'auto-ajouter en ami,
créer un doublon dans l'autre sens, déposer une charge de plus de 4 096 octets : ces trois
refus doivent venir de Postgres. Le test s'écrit donc en SQL direct, **en contournant
l'API** — sinon il mesure la politesse du code applicatif, pas la contrainte.

**La révocation coupe réellement.** Révoquer un appareil lui ferme l'API à la requête
suivante. C'est le défaut critique du jalon A : `verifierSessionActive` existait, était
correcte, et n'était appelée nulle part — la serrure était construite, jamais branchée.

**La clé privée ne sort pas.** Absente de toute charge HTTP, de tout message vers le
webview, et du disque en clair.

### 7.2 Ce qu'on mesure au lieu de le croire

La spec SkyShare §4.5 annonce environ 112 000 requêtes par mois pour dix utilisateurs
actifs, soit 11 % du quota Vercel gratuit. **C'est une estimation.** Le jalon C la vérifie :
nombre de requêtes d'une session type, poids réel de la réponse « rien de neuf », et heures
de calcul consommées sur Neon par une synchronisation toutes les 30 secondes.

Si le chiffre est faux, la cadence est encore facile à changer.

### 7.3 Trois règles de méthode, tirées du jalon A

**Aucun fichier sans propriétaire.** `vercel.json`, `.env.example`, les fichiers de
verrouillage et `public/` ont traversé neuf tâches et autant de relectures sans que
personne ne les ouvre — aucun brief ne les possédait. Le premier déclarait une tâche
nocturne vers une route supprimée ; le second distribuait encore les secrets de repli
arrachés du code. **Le plan attribuera explicitement chaque fichier de configuration.**

**Un serveur réel, pas seulement des tests.** Un antislash avalé par un heredoc avait rendu
tout le routage inopérant pendant que la compilation, les tests et la construction étaient
au vert. Ce qui filtre des chemins se vérifie en interrogeant un vrai serveur. Corollaire :
ne jamais écrire de contenu à échappements via un heredoc ou `node -e`.

**Chaque test déclare sa portée.** Le test des redirections mortes du jalon A ne couvrait
que les routes d'API — c'était écrit, donc personne ne s'y est fié à tort, et le lien mort
qu'il ne voyait pas a été trouvé autrement.

### 7.4 Tout est vérifiable sans le propriétaire

Contrairement au jalon 0, il n'y a ici ni NAT, ni deux box, ni pair-à-pair : **deux
appareils simulés contre la vraie API suffisent à tout éprouver.** Aucune manipulation
humaine n'est requise.

---

## 8. Risques

| Risque | Impact | Atténuation |
|---|---|---|
| Le flux de connexion reproduit la faille supprimée | Vol de session | Signature du `state` obligatoire dès la première ligne, code à usage unique, secret jamais dans une URL. Quatre épreuves adverses au §7.1. |
| La synchronisation dépasse le quota Neon | Base suspendue | Mesure réelle au §7.2 avant d'aller plus loin. La cadence est un paramètre, pas une constante. |
| `signRefreshToken` reste identique à `signJWT` | Jeton de renouvellement sans valeur | Traité explicitement en D3 : durée distincte, portée restreinte à la route de renouvellement. |
| Le webview accède au jeton | Vol par une page hostile | Frontière D5 : le réseau est côté Rust, le jeton ne traverse jamais. |
| Les crates promus traînent du code de prototype | Dette silencieuse | D6 : promotion sélective, artefacts jetables supprimés, archive conservée. |

---

## 9. Ce qui reste ouvert

- **Le dépôt SkyShare n'a aucun remote GitHub.** Le choix initial était un dépôt public ;
  il n'a jamais été créé. À trancher avant la fin du jalon, car la clé privée d'appareil et
  le coffre-fort ne changent rien au fait qu'un dépôt public expose la structure.
- **Le bouton « Rejoindre la bêta » du site ne mène toujours nulle part.** Le jalon C ne
  produit pas de binaire distribuable. Une liste d'attente sur le site est le geste
  minimal qui rendrait la page cohérente — hors périmètre de ce jalon, à décider.
