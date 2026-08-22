# SkyShare — Document d'architecture

> Partage d'écran pair-à-pair haute qualité, sans les plafonds de Discord.
> Date : 22 août 2026 · Statut : validé, référence stable du projet

---

## 1. Objectif

Un logiciel de partage d'écran de bureau (Windows, macOS, Linux) qui supprime les
trois limites de Discord : résolution, images par seconde et débit. Le flux circule
directement entre les machines, sans serveur relais, donc sans coût d'infrastructure
et sans plafond imposé par un tiers.

### Principes directeurs

1. **Coût d'infrastructure nul.** Aucun serveur à louer. Le seul backend est le site
   Vercel existant, qui ne transporte que quelques kilo-octets par connexion.
2. **Le média ne touche jamais un serveur.** Image et son circulent uniquement entre
   les pairs, chiffrés de bout en bout.
3. **Aucun plafond artificiel.** La seule limite est le matériel de l'utilisateur.
4. **Zéro impact sur l'existant.** Le site EriniumGroup et son schéma de base ne sont
   ni lus, ni modifiés, ni ralentis.
5. **Honnêteté technique.** Quand une connexion ne peut pas aboutir, l'app le dit et
   explique pourquoi. Jamais d'attente indéfinie.

---

## 2. Décisions d'architecture

| # | Décision | Justification |
|---|----------|---------------|
| 1 | **Tauri (UI React) + cœur Rust natif** | Une UI web permet le lecteur riche exigé (multi-flux, zoom, recadrage) ; un cœur natif est indispensable pour la capture GPU et les encodeurs matériels. Electron plafonnerait à ~1080p60 — c'est précisément la limite de Discord, qui est une app Electron. |
| 2 | **Zéro serveur : signaling sur Vercel + Neon** | Coût nul, aucune machine à administrer. Contrepartie assumée : ~5-10 % des paires (CGNAT, NAT symétrique, 4G) ne pourront pas se joindre. La couche transport est abstraite pour permettre l'ajout ultérieur d'un relais TURN sans réécriture. |
| 3 | **WebRTC via `str0m`, contrôle de congestion réécrit** | On conserve ICE (perçage NAT éprouvé) et DTLS-SRTP (chiffrement obligatoire), on remplace le limiteur de débit — c'est lui, et non une politique commerciale, qui bride la qualité chez Discord. |
| 4 | **Adresses réseau scellées de bout en bout** | Une connexion pair-à-pair expose nécessairement les IP aux deux pairs : c'est le protocole IP lui-même, aucun chiffrement ne le contourne. En revanche, ni Vercel ni la base ne voient jamais une adresse en clair, et aucune IP n'est affichée ni journalisée. |
| 5 | **Amis → entrée directe · inconnus → salle d'attente** | La confiance est établie par la demande d'ami mutuelle. Un inconnu qui clique sur un lien public n'obtient aucune adresse tant que l'hôte n'a pas approuvé. |
| 6 | **10+ spectateurs via simulcast 3 couches** | Le coût GPU reste constant quel que soit le nombre de spectateurs ; seul le réseau croît linéairement. Indispensable car les cartes grand public plafonnent à ~8 sessions d'encodage simultanées. |
| 7 | **Son du partage uniquement, pas de micro** | Un vocal correct (écho, bruit, mixage, push-to-talk) est un projet à part entière que Discord assure déjà gratuitement, et où les utilisateurs sont déjà connectés. |
| 8 | **Windows d'abord, abstraction OS dès le premier jour** | Chaque OS a une API de capture radicalement différente. Seul `sky-capture` change au portage ; les quatre autres modules sont déjà multiplateformes. |
| 9 | **Tables `sky_*`, migration séparée** | Isolation totale de l'existant. Liaison par `discord_id` uniquement. |
| 10 | **Sync manuel + 30 s / 5 min** | Maintient la consommation à ~112 000 requêtes/mois pour 10 utilisateurs, soit ~11 % du quota Vercel gratuit. |
| 11 | **Dépôt public GitHub** | CI illimitée et gratuite. Sur dépôt privé, les runners macOS consomment 10× le quota et épuiseraient les 2 000 min/mois en une douzaine de builds. |

---

## 3. Architecture générale

Trois couches, avec une règle absolue : **le flux vidéo ne transite jamais par Vercel.**

```
   MACHINE ÉMETTRICE                                  MACHINE SPECTATRICE
┌──────────────────────┐                          ┌──────────────────────┐
│  Interface (React)   │                          │  Interface (React)   │
│  amis · listes · UI  │                          │  lecteur · zoom      │
├──────────────────────┤                          ├──────────────────────┤
│  Cœur Rust           │                          │  Cœur Rust           │
│  capture · encodage  │                          │  décodage · rendu    │
│  transport · crypto  │                          │  transport · crypto  │
└──────────┬───────────┘                          └───────────┬──────────┘
           │                                                  │
           │  ① rendez-vous (quelques Ko, scellé E2E)         │
           └──────────────┐                    ┌──────────────┘
                          v                    v
              ┌──────────────────────────────────────────┐
              │   VERCEL (site Erinium existant)         │
              │   auth Discord · amis · listes           │
              │   boîte aux lettres de signaling         │
              │   ── ne voit ni image, ni son, ni IP ──  │
              └──────────────────────────────────────────┘

           ② connexion directe — vidéo + audio chiffrés
           ◄══════════════════════════════════════════════►
```

### Responsabilités

- **Interface** — ne connaît rien au réseau. Elle affiche, envoie des commandes au
  cœur et reçoit des événements. Remplaçable intégralement sans toucher au reste.
- **Cœur Rust** — ne connaît rien à Discord ni à la base. Il reçoit « connecte-toi à
  ce pair avec ces paramètres » et exécute. Testable en ligne de commande, sans UI.
- **Backend** — simple annuaire. Il sait qui est ami avec qui et transporte des
  enveloppes scellées qu'il ne peut pas ouvrir. Si Vercel tombe, les partages en
  cours continuent ; seuls les nouveaux ne peuvent plus démarrer.

### Modules du cœur Rust

| Module | Rôle | Dépend de |
|--------|------|-----------|
| `sky-capture` | Capture écran et audio, une implémentation par OS | — |
| `sky-encode` | Encodeurs matériels, couches de qualité | — |
| `sky-crypto` | Identité de l'appareil, scellage des adresses | — |
| `sky-net` | ICE, DTLS-SRTP, contrôle de congestion | `sky-crypto` |
| `sky-core` | Orchestration, API exposée à l'interface | les quatre autres |

---

## 4. Identité, amis et données

### 4.1 Connexion Discord depuis une app native

Méthode conforme à la RFC 8252, avec PKCE :

```
1. L'app ouvre un serveur éphémère sur 127.0.0.1:47821 (inaccessible de l'extérieur)
2. L'app ouvre le navigateur système sur le site Erinium
3. L'utilisateur autorise sur l'écran Discord
4. Le site échange le code, crée la session, redirige vers 127.0.0.1:47821
5. L'app récupère le jeton et ferme le serveur éphémère
```

**Rejeté :** la redirection vers `skyshare://auth?token=...`. N'importe quelle
application installée peut enregistrer le même protocole et intercepter le jeton.

Une **application Discord dédiée « SkyShare »** est créée, distincte de celle du
site : l'écran de consentement affiche le bon nom et les deux systèmes restent
indépendants.

Le jeton de session est stocké dans le coffre-fort du système (Credential Manager,
Trousseau, Secret Service) — jamais en clair sur disque.

### 4.2 Identité cryptographique

À la première connexion, l'app génère une paire de clés. La clé publique est publiée
en base ; la clé privée ne quitte jamais la machine. Une clé par appareil, révocable
individuellement.

C'est cette paire qui permet le scellage des adresses : les candidats ICE sont
chiffrés avec la clé publique du destinataire **avant** d'être postés sur Vercel.

### 4.3 Schéma de base

Huit tables préfixées `sky_`, dans une migration séparée. Aucune table existante
n'est lue ni modifiée.

| Table | Contenu |
|-------|---------|
| `sky_users` | ID Discord, pseudo, avatar, dernière activité |
| `sky_devices` | Machines et clés publiques |
| `sky_friendships` | Relations : en attente / acceptée / bloquée |
| `sky_friend_lists` | Listes : nom, couleur, emoji |
| `sky_list_members` | Appartenance aux listes |
| `sky_sessions` | Partages en cours : visibilité, réglages, jeton public |
| `sky_envelopes` | Boîte aux lettres chiffrée du signaling |
| `sky_join_requests` | Salle d'attente |

Contraintes structurelles imposées par la base, pas par le code applicatif :

- `sky_friendships` stocke le couple trié (plus petit identifiant en premier) avec un
  index unique : impossible de créer deux fois la même amitié dans les deux sens, ni
  de s'ajouter soi-même.
- `sky_envelopes` expire après 5 minutes et chaque enveloppe est effacée à la lecture.
  La table reste vide en régime normal.

### 4.4 Amis et listes

Ajout par pseudo Discord exact ou par code ami court (`SKY-7F2A-9K`). La demande doit
être acceptée ; avant acceptation, aucune présence n'est visible.

Un ami peut appartenir à plusieurs listes simultanément. Les listes servent
uniquement de filtre au moment de partager.

### 4.5 Rythme de synchronisation

| Contexte | Rythme |
|----------|--------|
| Bouton rafraîchir | manuel, à tout moment |
| App au premier plan | 30 s |
| App en arrière-plan | 5 min |
| Négociation en cours | 500 ms, pendant ~3 s uniquement |

Un sync = **une seule requête** groupée (amis + demandes + partages actifs), avec une
réponse « rien de neuf » de quelques octets. Une fois la connexion établie, le
compteur retombe à zéro : une session de 4 heures ne coûte aucune requête.

Budget pour 10 utilisateurs actifs : ~112 000 requêtes/mois, soit ~11 % du quota
Vercel gratuit.

**Point de vigilance connu et accepté :** un sync toutes les 30 s maintient la base
Neon éveillée pendant les heures d'activité, ce qui consomme les heures de calcul du
plan gratuit. Le propriétaire a explicitement exclu les frais de base et de trafic
Vercel du périmètre « coût zéro ».

---

## 5. Rencontre pair-à-pair

### 5.1 Établissement d'une connexion

```
① L'émetteur interroge un serveur STUN public → obtient son adresse publique
② Il scelle cette adresse avec la clé publique du spectateur, la dépose sur Vercel
③ Le spectateur la récupère et la déchiffre avec sa clé privée
④ Les deux envoient des paquets simultanément : chaque box interprète le paquet
   entrant comme la réponse à sa propre requête sortante et le laisse passer
⑤ Tunnel chiffré établi. Vercel n'est plus sollicité.
```

Serveurs STUN publics, gratuits, sans inscription (`stun.l.google.com`,
`stun.cloudflare.com`). Ils ne voient transiter aucune donnée : ils répondent
uniquement « voici l'adresse d'où tu m'écris ».

**L'IPv6 est tentée en priorité.** Quand les deux pairs l'ont activée — cas courant
sur les box françaises — il n'y a plus de NAT du tout et le perçage devient inutile.

### 5.2 Ce que Vercel peut observer

| Élément | Visible ? |
|---------|-----------|
| Adresse réseau | Non — enveloppe scellée |
| Image, son | Non — ne transitent jamais |
| Clé privée | Non — ne quitte jamais la machine |
| « L'appareil A a écrit à l'appareil B » | **Oui** — métadonnée visible |

Le contenu est inviolable ; les métadonnées de mise en relation ne le sont pas.
L'exposition reste faible puisqu'il s'agit de la base contrôlée par le propriétaire,
mais le document ne prétend pas à une confidentialité absolue.

### 5.3 Modes de partage

| Mode | Qui peut entrer | Salle d'attente |
|------|-----------------|-----------------|
| Public | Toute personne ayant le lien | Oui pour les inconnus, non pour les amis |
| Privé | Tous les amis acceptés | Non |
| Liste | Membres de la liste choisie | Non |

Dans tous les cas, un bandeau permanent affiche qui regarde, avec éjection immédiate
en un clic (connexion coupée, pas seulement masquée).

### 5.4 Liens publics

```
https://eriniumgroup.vercel.app/s/7F2A9K
```

Page légère qui tente d'ouvrir `skyshare://join/7F2A9K`, avec repli sur le
téléchargement si l'app n'est pas installée. L'app reprend le lien au premier
lancement.

**Ce lien ne contient aucun secret**, seulement un identifiant public de session.
Son interception est sans effet : il faut être connecté à Discord et passer la salle
d'attente. C'est l'inverse du lien d'authentification, qui transporte un jeton et
doit être protégé.

Le lien est révocable à tout moment : les spectateurs connectés restent, les
nouveaux sont refusés.

### 5.5 Échec de connexion

Abandon au bout de 8 secondes, avec diagnostic explicite indiquant lequel des deux
réseaux pose problème et les pistes concrètes (passer en Wi-Fi, activer l'IPv6).
Un test réseau est disponible dans les réglages, indépendamment de tout partage.

**Jamais d'attente indéfinie.**

---

## 6. Pipeline vidéo

### 6.1 Écarts délibérés avec Discord

| Limite chez Discord | Correction |
|---------------------|------------|
| Sous-échantillonnage 4:2:0 — couleur au quart de la résolution, texte illisible | **4:4:4** — couleur pleine résolution |
| Débit plafonné (~2,5 Mbps, ~8 Mbps avec Nitro) | Plancher garanti défini par l'utilisateur, jusqu'à 100 Mbps |
| Congestion frileuse | Contrôle réécrit : descente lente, remontée rapide, jamais sous le plancher |

Le 4:4:4 est le facteur le plus déterminant pour la lisibilité du texte et du code.

### 6.2 Chaîne de traitement

```
ÉCRAN → [ GPU : capture → redimensionnement → encodage ] → chiffrement → réseau
```

**La texture ne redescend jamais en mémoire centrale.** Seul le flux compressé
traverse le processeur. C'est ce qui permet du 4K 144 fps à 1-3 % de CPU ; un
pipeline avec allers-retours GPU↔RAM saturerait un processeur haut de gamme dès le
1440p60.

### 6.3 Capture Windows

**Windows.Graphics.Capture**, deux modes : écran entier (y compris jeux en plein
écran exclusif) et fenêtre précise.

Le curseur est capturé séparément et incrusté à l'affichage chez le spectateur : il
reste net malgré la compression et peut être masqué.

Audio via **WASAPI en mode loopback**, encodé en Opus à 256-510 kbps (contre 64-96
kbps chez Discord).

### 6.4 Codecs

Détection matérielle au premier lancement, puis négociation avec chaque spectateur.

| Codec | Condition | Gain |
|-------|-----------|------|
| AV1 | RTX 40+, RX 7000+, Arc | ~40 % de débit en moins à qualité égale |
| HEVC | GTX 10+, RX 400+, Intel 7ᵉ gén | ~30 % de moins que H.264 |
| H.264 4:4:4 | Matériel des 12 dernières années | Socle universel |
| x264 logiciel | Aucun encodeur détecté | Fonctionne, coûte du CPU — signalé à l'utilisateur |

Réglages communs : pas d'images bidirectionnelles (latence), rafraîchissement
progressif au lieu d'images-clés complètes (supprime les pics de débit périodiques
et les micro-saccades).

### 6.5 Couches de qualité

L'utilisateur règle **une seule chose** : sa qualité maximale. Les couches
inférieures en sont dérivées.

```
1440p60 · 30 Mbps  →  1080p60 · 10 Mbps  →  720p30 · 3 Mbps
```

- **Les couches inutilisées ne sont pas encodées.** Une couche naît quand le premier
  spectateur la demande, meurt quand le dernier la quitte.
- **Le coût GPU ne dépend pas du nombre de spectateurs.** Seul le réseau croît, et la
  jauge d'upload l'affiche en direct.

### 6.6 Profils

| Profil | Réglages |
|--------|----------|
| Fluidité | 1080p · 120 fps · 20 Mbps |
| Netteté | 1440p · 60 fps · 30 Mbps |
| Économe | 1080p · 60 fps · 8 Mbps |
| Manuel | Libre, jusqu'à 4K · 144 fps · 100 Mbps |

Aucun plafond artificiel en mode manuel. L'app signale en direct si le GPU ou
l'upload décroche.

---

## 7. Client de visionnage

### 7.1 Dispositions

- **Focus** — un grand flux, les autres en miniatures
- **Grille** — 2×2, 3×3
- **Fenêtres détachées** — chaque flux devient une fenêtre système indépendante,
  positionnable sur n'importe quel écran, avec option toujours au premier plan

Le décodage matériel n'a pas de limite de sessions : six flux en 1440p60 coûtent
environ 15 % d'un GPU moderne.

### 7.2 Zoom et déplacement

- Molette → zoom centré sur le curseur
- Clic maintenu + glisser → déplacement
- Double-clic → retour à la vue complète
- Zoom jusqu'à 800 %

**Le zoom est strictement local** : il n'affecte ni l'émetteur ni les autres
spectateurs.

**Au-delà de 150 % de zoom, le client bascule sur la couche de qualité maximale
disponible.** Sans cela, zoomer ne ferait qu'agrandir les artefacts de compression.

### 7.3 Format d'image

| Mode | Effet |
|------|-------|
| Ajusté (défaut) | Tout visible, bandes noires si nécessaire |
| Remplir | Occupe toute la zone, bords rognés |
| Ratio forcé | 16:9, 4:3, 21:9 |
| Recadrage libre | Zone choisie de l'écran source |

Le recadrage est mémorisé par personne et restauré au partage suivant.

### 7.4 Audio

Volume et coupure **par flux**, choix du périphérique de sortie par flux, décalage
audio réglable (±200 ms).

### 7.5 Qualité côté spectateur

Menu par flux : **Auto · 1440p · 1080p · 720p**. Le changement est instantané — il
s'agit de s'abonner à une couche déjà encodée, sans renégociation ni coupure.

Le mode Auto mesure la bande passante réelle et n'affecte que le spectateur
concerné : une connexion faible ne dégrade jamais l'expérience des autres.

Panneau de statistiques accessible au clavier : débit reçu, images par seconde,
latence, pertes.

### 7.6 Latence contre fluidité

Curseur unique, de 0 ms (réactivité maximale) à 120 ms de tampon (fluidité
maximale). Défaut : 40 ms.

---

## 8. Distribution

### 8.1 Signature de code

| OS | Niveau retenu | Coût | Expérience au premier lancement |
|----|---------------|------|--------------------------------|
| Linux | aucun | 0 € | Aucun obstacle |
| Windows | aucun | 0 € | Avertissement SmartScreen, contournable en deux clics |
| macOS | **signature ad-hoc** | **0 €** | Gatekeeper bloque une fois → Réglages Système → « Ouvrir quand même » |

**Décision : sortie non signée, avec une exception obligatoire sur macOS.**

La signature ad-hoc (`codesign -s -`, sans compte Apple, automatisable en CI) n'est
pas optionnelle : sur Apple Silicon, un binaire arm64 dépourvu de toute signature est
tué au démarrage par le système, sans message exploitable pour l'utilisateur. Elle ne
supprime pas l'avertissement Gatekeeper, elle rend seulement l'application exécutable.

La friction ne concerne que la **première** installation : les mises à jour livrées par
l'updater intégré ne transitent pas par le navigateur, ne reçoivent donc pas l'attribut
de quarantaine, et s'installent silencieusement.

Les certificats payants (~250-400 €/an sur Windows ou ~10 $/mois via Azure Trusted
Signing ; 99 $/an pour la notarisation Apple) ne se justifieront qu'à partir du moment
où des inconnus téléchargeront l'application et abandonneront devant l'avertissement.

La somme de contrôle de chaque installateur est publiée sur la page de téléchargement.

### 8.2 Construction automatique

À chaque tag de version, trois runners GitHub construisent en parallèle et publient
une release : `.msi`/`.exe` (Windows), `.dmg` (macOS), `.AppImage`/`.deb` (Linux).

**Dépôt public** : CI illimitée et gratuite. Les secrets vivent dans les variables
d'environnement GitHub, jamais dans le code.

Mise à jour automatique via l'updater intégré de Tauri, hébergée sur les Releases
GitHub. Signature cryptographique propre à Tauri (gratuite, sans rapport avec les
certificats OS).

---

## 9. Jalons

Chaque jalon est livrable et testable, et recevra son propre spec et son propre plan
au moment d'être attaqué.

| # | Jalon | Résultat | Ordre de grandeur |
|---|-------|----------|-------------------|
| 0 | Faisabilité | Binaire jetable : capture, encode, transmet à une autre machine | 3-5 jours |
| 1 | Fondations | Connexion Discord, amis, listes. Aucune vidéo. | 1-2 semaines |
| 2 | Premier pixel | Partage 1-à-1 entre amis, qualité fixe, UI minimale | 2-3 semaines |
| 3 | Qualité | Simulcast 3 couches, profils, jauge d'upload, 10+ spectateurs | 2 semaines |
| 4 | Lecteur | Multi-flux, zoom, recadrage, audio par flux, choix de qualité | 2 semaines |
| 5 | Public | Mode public, salle d'attente, liens et deep links | 1 semaine |
| 6 | Distribution | CI GitHub, installateurs, mise à jour automatique | 3-5 jours |
| 7 | Mac et Linux | Portage de `sky-capture` | 1-2 semaines |

Ces durées sont des ordres de grandeur, pas des engagements. Le jalon 2 est le plus
susceptible de déraper : c'est là que le réseau théorique rencontre les box réelles.

**Le jalon 0 est non négociable.** Il valide ou invalide les paris des sections 3 à 6
avant qu'on construise par-dessus. Un mur découvert à trois jours d'investissement
coûte infiniment moins qu'un mur découvert à trois mois.

---

## 10. Hors périmètre

Écarté volontairement, avec la raison :

| Élément | Raison |
|---------|--------|
| Chat vocal / micro | Discord le fait déjà bien, les utilisateurs y sont déjà connectés. Le pipeline audio est néanmoins dimensionné pour accueillir une seconde piste. |
| Relais TURN | Coût récurrent. La couche transport est abstraite pour permettre l'ajout ultérieur sans réécriture. |
| Enregistrement des sessions | Aucun besoin exprimé. |
| Version web (sans installation) | Un navigateur ne peut pas atteindre la qualité visée ; ce serait reconstruire les limites qu'on fuit. |

### Évolutions identifiées

- **Relais entre pairs** — un spectateur en forte bande passante retransmet à
  d'autres, formant un arbre de diffusion. Divise l'upload de l'hôte par 3 à 4, sans
  aucun serveur.
- **Mode loupe** — l'émetteur n'encode que la région observée par un spectateur
  zoomé, en résolution native.
- **Notification Discord** — un webhook annonce « X vient de lancer un partage » dans
  un salon, compensant le rythme de synchronisation de 30 s / 5 min.

---

## 11. Risques

| Risque | Impact | Atténuation |
|--------|--------|-------------|
| NAT symétrique / CGNAT chez un pair | Connexion impossible (~5-10 % des paires) | IPv6 tentée en priorité, diagnostic explicite, ajout ultérieur possible d'un relais TURN |
| Contrôle de congestion maison instable | Image qui pulse ou fige sous charge réseau | Validé au jalon 0 sur réseau réel ; repli sur l'algorithme standard conservé en option |
| Heures de calcul Neon dépassées | Base suspendue, site Erinium affecté | Sync groupé en une requête ; frais explicitement acceptés par le propriétaire |
| Limite de sessions d'encodage GPU | Blocage au-delà de ~8 flux | Architecture simulcast : le nombre d'encodages est indépendant du nombre de spectateurs |
| Portage macOS sans matériel de test | Jalon 7 bloqué | Runners macOS GitHub pour la construction ; test réel requis avant publication |
