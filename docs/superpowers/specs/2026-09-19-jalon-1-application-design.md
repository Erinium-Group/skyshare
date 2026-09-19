# Jalon 1 — l'application SkyShare

**Date :** 19/09/2026 · **Statut :** conception validée section par section avec le propriétaire,
à relire avant le plan.

**Ce que ce jalon livre, en une phrase :** une vraie application Windows, qui démarre avec la
session et vit près de l'horloge, qui remplace la ligne de commande pour tout ce qui touche au compte
et aux amis, et qui sait **partager** et **regarder** — la connexion et ses mesures, sans image.

## 1. Où l'on part

Les jalons C1 et C2 ont livré, prouvé en production et à l'essai réel du 19/09/2026 :

- l'API de signaling du site (12 routes `/api/sky/*`, annuaire de clés, amis, listes, boîte aux
  lettres d'enveloppes scellées) ;
- la crate `sky-compte` : connexion Discord native (RFC 8252), trousseau Windows, renouvellement,
  annuaire, dépôt et relève d'enveloppes ;
- la négociation par boîte aux lettres, dans `sky-probe host` / `view` : canal ouvert **7,1 s**
  après le lancement de `view`, sans relais, entre deux réseaux et deux comptes Discord.

Il n'existe **aucune application graphique** : ni dossier `app/`, ni configuration Tauri.
La spec d'architecture (`2026-08-22-skyshare-architecture-design.md`) fixe Tauri + React et la
séparation des couches ; la spec de signaling (`2026-09-01-jalon-c-signaling-design.md`, §6.3 à
§6.5) fixe la frontière D5 — le cœur Rust possède l'état, l'interface n'en possède rien — et les
quatre écrans du jalon.

## 2. Décisions

Chacune a été tranchée par le propriétaire pendant la conception.

**D1 — Périmètre : les quatre écrans de la spec, plus le partage.**
Connexion, Amis, Listes, Mon compte — et les boutons « Partager mon écran » et « Regarder ».
C'est un écart assumé à la spec de signaling, qui excluait le partage du jalon 1 : la
négociation est prouvée, elle a sa place dans l'application.

**D2 — « Regarder » montre la connexion, pas l'image.**
La vidéo reçue est mesurée puis **jetée** — jamais écrite sur le disque. L'image arrive au
jalon 2, avec le transport corrigé (écart 7) et le décodage HEVC 4:4:4. Le panneau le dit en
toutes lettres.

**D3 — Disponibilité explicite.**
Un ami ne peut obtenir une réponse que si tu as cliqué « Partager ». La fenêtre de
disponibilité reste celle du C2 : **30 minutes** (`FENETRE_HOTE`), attente du spectateur
**60 s** (`ATTENTE_SPECTATEUR`), synchronisation toutes les **2 s** pendant l'une ou l'autre
(`CADENCE`). Hors partage, aucune demande n'est honorée : aucun geste d'un tiers ne déclenche
un partage.

**D4 — L'application vit comme Discord.**
Icône près de l'horloge, fermer la fenêtre la réduit au lieu de quitter, démarrage avec la
session Windows (réglable dans Mon compte), **une seule instance**. L'instance unique n'est pas
un confort : deux instances se voleraient les enveloppes, que le serveur efface en les livrant.

**D5 — Architecture « A » : une bibliothèque de partage commune.**
La négociation quitte le binaire `sky-probe` pour une crate `sky-partage`, utilisée par
`sky-probe` et par l'application. Une seule négociation à maintenir, gardée par les tests
existants. Écartés : recopier la négociation dans l'application (deux copies du code le plus
délicat du projet) ; piloter `sky-probe.exe` en lisant sa sortie (aucune vraie gestion d'état ni
d'erreur).

**D6 — Disposition : barre latérale.**
Navigation à gauche (Amis, Listes, Mon compte), bouton de partage toujours visible en bas de la
barre. Direction artistique du site : palette `#16120F` / `#1D1815` / `#26201B`, accent
`#C4664A`, succès `#6E8F6A`, alerte `#B0553F`, polices Instrument Serif (titres) et Manrope
(corps) — valeurs relevées dans `src/app/globals.css` du site. Maquettes validées :
`.superpowers/brainstorm/22558-1789840506/content/` (`disposition.html`, `partage.html`,
`ecrans.html`).

## 3. Architecture

```
spike/crates/
  sky-crypto   inchangée
  sky-net      inchangée (PeerLink, Pacer)
  sky-capture  inchangée
  sky-encode   inchangée
  sky-compte   + listes (lecture dans sync, création, modification, suppression, membres)
               + amis : retirer, bloquer ; code ami : régénérer ; appareils : révoquer
  sky-partage  NOUVELLE — rendez-vous et négociation, extraits de sky-probe
  sky-probe    commandes inchangées ; host et view deviennent des affichages de sky-partage
  sky-app      NOUVELLE — cœur Tauri : état, boucle de synchronisation, commandes, événements
app/           NOUVEAU — interface React + Vite + Tailwind
```

**`sky-partage`** reprend `rendez_vous.rs` et la colle de négociation de `cmd_host` /
`cmd_view`. Elle ne fait **aucune sortie terminal** : elle rend des événements typés (« demande
reçue », « connecté en 0,6 s », « mesures », « l'ami n'a pas répondu », « connexion trop
lente », « arrêté ») et accepte un signal d'arrêt. Le côté spectateur garde les mesures et
**jette** le flux ; l'écriture dans un fichier devient une option de `sky-probe view` seulement.

**`sky-app`** est le seul endroit qui tient l'état de la machine :

- une **boucle de synchronisation unique** : 30 s fenêtre visible, 5 min fenêtre réduite, 2 s
  pendant un partage ou une attente ; l'`Etat` précédent est transmis à chaque tour ;
- l'état en mémoire (moi, amis, demandes, listes, appareils, partage en cours) ;
- les **commandes Tauri** (intentions de l'interface) : `connexion`, `deconnexion`,
  `ajouter_ami`, `accepter_ami`, `retirer_ami`, `bloquer_ami`, `regenerer_code`,
  `revoquer_appareil`, `creer_liste`, `modifier_liste`, `supprimer_liste`, `definir_membres`,
  `partager`, `regarder`, `arreter`, `demarrage_automatique` ;
- les **événements** poussés à l'interface : `etat` (instantané complet après chaque
  changement) et `partage` (flux d'événements de `sky-partage`).

L'interface **n'a ni jeton, ni clé, ni accès réseau** : elle affiche des instantanés et envoie des
intentions (frontière D5 de la spec de signaling). `sky-compte` reste synchrone (`ureq`, décision
D6 du C2) ; `sky-app` l'appelle depuis des fils dédiés, jamais depuis le fil de l'interface.

Extensions Tauri 2 officielles : instance unique, démarrage automatique ; icône de la zone de
notification par l'API intégrée de Tauri.

## 4. Écrans et parcours

**Connexion** (premier lancement, ou session expirée) : un bouton « Se connecter avec Discord ».
Le navigateur s'ouvre ; l'application se met à jour seule au retour. Premier lancement après la
connexion : si aucun appareil n'est enregistré, l'application l'enregistre elle-même, sous le nom de
la machine. **Pas de renommage** à ce jalon : aucune route du site ne le permet, et §5 limite le
jalon à une seule modification du site (arbitrage du 19/09, relevé à l'écriture du plan).

**Amis** : champ « code ami » + Ajouter ; demandes reçues avec Accepter ; amis avec le nombre
d'appareils, un bouton **Regarder** (actif seulement si l'ami a au moins un appareil) et un
menu « … » avec Retirer et Bloquer.

**Listes** : à gauche, les listes (couleur, émoji, nombre de membres) et « Nouvelle liste » ; à
droite, l'édition de la liste choisie — nom, couleur, émoji, membres à cocher parmi les amis,
Enregistrer, Supprimer. Une phrase dit que les listes ne filtrent pas encore les partages.

**Mon compte** : nom Discord ; code ami avec Copier et Régénérer ; appareils (celui-ci signalé,
les autres révocables) ; case « Lancer SkyShare au démarrage de Windows » ; Se déconnecter.

**Partager** (bas de la barre) : choix de l'écran (principal par défaut), puis la zone passe en
orange — « En partage », écran, temps restant, Arrêter. Le panneau central montre qui regarde,
depuis quand, le débit envoyé et l'aller-retour. **L'icône près de l'horloge change tant que
le partage dure** : on ne partage jamais sans le savoir. Sans carte NVIDIA, le bouton est
désactivé et le dit (pas de repli logiciel — question ouverte du projet).

**Regarder** : attente jusqu'à 60 s, puis soit « Connecté en X s · connexion directe, sans
relais » avec débit reçu, images/s, gigue, durée et Arrêter, soit un échec.

**Échecs, tous en clair** : « X n'est pas en partage » ; « Aucune connexion directe n'a pu
s'établir entre vos deux réseaux » (un constat, pas une cause supposée : rien ne mesure lequel des
deux réseaux bloque) ; « La connexion était trop lente pour la vidéo » (tampon d'émission saturé — limite
connue du transport actuel, corrigée au jalon 2) ; « Session expirée — reconnecte-toi ».

## 5. Changement côté site

**Les membres d'une liste ne sont lus par aucune route.** `listesDe`
(`src/lib/sky/listes.ts:101`) ne sélectionne que `id, nom, couleur, emoji, created_at` ;
`definirMembres` écrit `list_members`, rien ne le relit. L'écran Listes ne pourrait pas afficher
les cases déjà cochées.

Correctif : chaque liste renvoyée par `GET /api/sky/sync` porte `membres: number[]`, obtenus
**dans la même requête** que les listes (agrégation), pour ne changer ni le nombre
d'allers-retours de la synchronisation ni son budget. Tests du site mis à jour ; les trois
vérifications (`tsc --noEmit`, `npm test`, `npm run build`). **Poussée sur `main` du site —
donc déploiement — sur accord explicite du propriétaire.** Aucune modification de schéma.

## 6. Limites connues, assumées

- **Tant que l'application tourne, `sky-probe`** lancé sur la même machine lui vole ses
  demandes (toute commande qui synchronise consomme les enveloppes). L'application le dit dans
  Mon compte ; `sky-probe` reste un outil d'essai.
- Un `login` fait révoquer puis réenregistrer l'appareil (C2) : l'application ne se reconnecte
  donc **jamais** pendant un partage ou une attente — elle attend la fin.
- Le flux vidéo échoue encore sur des réseaux lents (écart 7) ; l'application l'affiche, elle
  ne le corrige pas.
- Application **non signée** : avertissement « éditeur inconnu » à l'installation (signature et
  mises à jour automatiques : jalon 6).

## 7. Hors périmètre

Image et décodage (jalon 2) · transport vidéo corrigé, écart 7 (jalon 2) · simulcast et
plusieurs spectateurs (jalon 3) · lecteur, zoom, audio (jalon 4) · lien public et salle
d'attente (jalon 5) · signature, installateur signé, mises à jour automatiques (jalon 6) · macOS
et Linux (jalon 7) · filtrage des partages par liste · notifications système · repli
d'encodage logiciel.

## 8. Vérification

- **`sky-partage`** : les tests actuels de `rendez_vous.rs` et de la négociation **déplacés, pas
  réécrits**, avec leurs neutralisations. `sky-probe host` / `view` se comportent exactement
  comme à l'essai réel.
- **`sky-compte`** : listes, retrait, blocage, régénération du code, révocation — testés contre
  le serveur factice, **dérivé du code du site et jamais plus permissif que lui** (nom de liste
  1 à 40 caractères, émoji 8 octets au plus, couleur `#RRGGBB` ou nulle, 200 membres au plus,
  refus uniforme « liste introuvable ou non-ami », 409 sur nom déjà pris). Chaque paramètre
  envoyé est testé **contre la ligne du site qui le lit** (leçon de l'essai réel, 19/09/2026).
- **`sky-app`** : chaque commande est une fonction Rust testable sans fenêtre ; test dédié de
  l'**instance unique** ; la boucle de synchronisation transmet l'état précédent et change de
  cadence au bon moment (horloge injectée).
- **`app/`** : tests des composants qui décident — « Regarder » inactif sans appareil, bon
  message pour chaque échec, état « en partage ». Pas de test d'apparence.
- **Essai réel tôt, pas à la fin** : dès que la connexion et l'écran Amis existent, installation
  sur les deux machines, connexion Discord depuis l'application, ajout d'un ami. Puis un second
  essai réel du partage complet, qui clôt le jalon : même protocole que le C2 (deux machines,
  deux réseaux, deux comptes Discord).
- Standard du projet : chaque protection prouvée en la retirant — le test doit rougir pour la
  bonne raison et elle seule ; `cargo test`, `cargo clippy --all-targets -- -D warnings`.

## 9. Installation

`tauri build` produit un installateur Windows (NSIS). Il s'installe sur le PC fixe et sur le
portable ; on réinstalle à chaque nouvelle version jusqu'au jalon 6.

## 10. Questions encore ouvertes

- **Mise en veille pendant un partage** : Windows peut suspendre la machine pendant une longue
  disponibilité. Comportement à observer à l'essai réel avant de décider s'il faut l'empêcher.
- **Nom par défaut de l'appareil** : le nom de la machine peut contenir des caractères que le
  site refuse ; il sera validé côté client par la règle déjà suivie par `sky-compte`, avec repli
  sur « Appareil SkyShare » — le repli que `sky-compte` utilise déjà, pour qu'un seul nom générique
  existe dans le produit.
