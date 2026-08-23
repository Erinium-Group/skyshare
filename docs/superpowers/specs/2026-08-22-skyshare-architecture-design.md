# SkyShare — Document d'architecture

> Partage d'écran pair-à-pair haute qualité, sans les plafonds de Discord.
> Date : 22 août 2026 · Statut : validé, référence stable du projet
> **Révisé le 23 août 2026** après le jalon 0, sur la base de
> `spike/docs/rapport-jalon-0.md`. Sections touchées : **§2 (décisions 4 et 5)**, §5.1,
> §5.2, §5.3, §5.5, §6.1, §6.2, §6.4, §6.5, §7.5, §9, §11 — douze au total.
> Chaque correction cite la mesure ou la recherche qui la
> fonde. Les **décisions produit** qui en découlent — D1 (que promet-on au matériel qui ne fait pas de 4:4:4 ?) et
> D2 (comment sceller l'offre de connexion ?) — ont été **tranchées le 23/08/2026** :
> elles appartiennent au propriétaire, pas au rapport de faisabilité.

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
| 4 | **Adresses réseau scellées de bout en bout** · **tranché — D2, voir §5.1** | Une connexion pair-à-pair expose nécessairement les IP aux deux pairs : c'est le protocole IP lui-même, aucun chiffrement ne le contourne. En revanche, ni Vercel ni la base ne voient jamais une adresse en clair, et aucune IP n'est affichée ni journalisée. **Corrigé au jalon 0 :** cette dernière phrase n'est vraie que de la *réponse*. L'*offre* part avant qu'une clé de destinataire n'existe, donc en clair — mesuré : deux adresses de l'émetteur lisibles dans le bloc. Tant que la boîte aux lettres est un simple relais, l'opérateur du serveur voit l'adresse publique de chaque émetteur. |
| 5 | **Amis → entrée directe · inconnus → salle d'attente** · **tranché — D2, voir §5.1** | La confiance est établie par la demande d'ami mutuelle. Un inconnu qui clique sur un lien public n'obtient aucune adresse tant que l'hôte n'a pas approuvé. **Corrigé au jalon 0 :** cette garantie n'est tenable que si l'offre est scellée *pour un destinataire connu*, donc après approbation — ce qui impose un annuaire de clés interrogeable avant production de l'offre. Avec un relais simple, l'offre a une forme de diffusion et tout lecteur du canal apprend l'adresse publique. |
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

> **Deux précisions établies au jalon 0** (`spike/docs/rapport-jalon-0.md`, écart 4).
>
> **La bibliothèque ICE ne découvre pas les adresses.** `str0m` est une bibliothèque
> sans-IO : elle joue l'agent ICE mais ne ramasse aucun candidat. L'interrogation des
> serveurs STUN et la déclaration des candidats sont à la charge de l'application, et
> constituent un composant à part entière de `sky-net`, pas un effet de bord de la
> bibliothèque. Sans lui, l'offre ne contient que l'adresse privée de la machine et
> aucune traversée de NAT n'est possible.
>
> **L'étape ② présuppose que la clé publique du destinataire est déjà connue.** C'est
> le point qui n'était pas explicite : tant que la boîte aux lettres n'est qu'un relais,
> l'offre part avant qu'une clé de destinataire n'existe, donc **en clair**. Mesuré sur
> le spike : le bloc d'offre contient deux adresses de l'émetteur, décodables par
> quiconque le reçoit ; seule la réponse est scellée.
>
> ### D2 — tranchée le 23/08/2026 : le sens de l'échange est inversé
>
> **C'est le spectateur qui produit l'offre, pas l'hôte.**
>
> Un annuaire de clés seul ne suffisait pas. Il résout le cas des amis, où l'on sait à
> qui l'on parle — mais pas le mode public : un hôte qui partage ne sait pas d'avance
> qui va cliquer, donc il n'a aucune clé à laquelle sceller. Il publierait ses adresses
> à la cantonade en attendant des visiteurs. Inverser le sens supprime le problème
> plutôt que de le contourner.
>
> **Le déroulement retenu :**
>
> 1. L'hôte publie son identité, son statut « en partage » et **sa clé publique**.
>    **Aucune adresse.** Rien de ce qu'il publie ne le localise.
> 2. Le spectateur qui veut rejoindre récupère cette clé, produit **son** offre, la
>    scelle avec la clé de l'hôte, la dépose.
> 3. L'hôte déchiffre — lui seul le peut — et voit qui demande. Ami accepté : entrée
>    directe. Inconnu : salle d'attente.
> 4. **Après acceptation seulement**, l'hôte produit sa réponse et la scelle avec la
>    clé du spectateur, contenue dans l'offre qu'il vient d'ouvrir.
>
> **Ce que cela garantit, et que le spike ne garantissait pas :**
>
> - Vercel ne voit **jamais** une adresse en clair, dans aucun sens — le principe
>   directeur n°2 redevient vrai sans réserve.
> - L'hôte n'expose ses adresses **qu'après avoir accepté**. La promesse du §5.3 —
>   « un inconnu qui clique n'obtient strictement rien tant que l'hôte n'a pas
>   approuvé » — devient structurelle au lieu d'être déclarative.
> - L'offre perd sa forme de diffusion : elle est adressée à une clé, pas collée dans
>   un canal partagé.
>
> **Ce que cela coûte, et qui est assumé :** le spectateur expose ses propres adresses
> avant d'être accepté. C'est acceptable et symétrique — c'est lui qui demande, elles
> sont scellées, et seul l'hôte qu'il a choisi de contacter peut les lire. Un hôte
> malveillant apprendrait l'adresse de qui tente de le rejoindre : c'est le prix de
> toute connexion directe, et il n'est payé que par celui qui a fait le premier pas.
>
> **Conséquence technique pour le jalon 2 :** cela inverse le rôle ICE contrôlant. Le
> jalon 0 avait l'hôte en initiateur ; la Tâche 7 est donc à reprendre dans ce sens.
> Le format du bloc scellé, lui, ne change pas.

### 5.2 Ce que Vercel peut observer

| Élément | Visible ? |
|---------|-----------|
| Adresse réseau | Non — enveloppe scellée · **tranché — D2, voir §5.1** |
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

> **Prérequis technique mesuré au jalon 0, à traiter au jalon 2.** Les trois modes
> supposent qu'un spectateur puisse rejoindre un partage **déjà en cours**. Avec le
> réglage de groupe d'images infini retenu au §6.4, les en-têtes de séquence ne sont
> émis qu'une seule fois, au tout début du flux : un spectateur qui arrive ensuite
> n'obtient qu'un flux indécodable — écran noir. Mesuré : 1 seul jeu d'en-têtes sur
> 1 201 images, et les flux coupés en leur milieu sont refusés par le décodeur. Voir
> §6.4 pour la correction à appliquer.

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

Abandon au bout de 8 secondes, avec diagnostic explicite et pistes concrètes (passer en
Wi-Fi, activer l'IPv6). Un test réseau est disponible dans les réglages, indépendamment
de tout partage.

**Jamais d'attente indéfinie.**

> **Correction du jalon 0.** Ce paragraphe promettait un diagnostic « indiquant lequel
> des deux réseaux pose problème ». Le jalon 0 a établi que ce n'est pas observable
> depuis un bord : chacun ne constate que l'absence de paquets, jamais la raison de
> cette absence. Un message qui désignerait un côté affirmerait une cause qu'il ne peut
> pas connaître — et dans le cas précis que Q5 existe pour tester, il ferait lire
> « rien ne s'est passé » là où il fallait lire « le réseau a tout bloqué ».
>
> Ce que le diagnostic distingue réellement, et c'est déjà beaucoup : **le perçage a
> échoué** (« cause probable : NAT strict d'un côté ») ; **le perçage a réussi mais le
> canal chiffré ne s'est pas ouvert** (« NAT n'est PAS en cause ») ; **aucun paquet n'est
> parvenu**, ce qui laisse volontairement deux causes ouvertes. S'y ajoute un compteur
> d'erreurs sur le port local, qui nuance le verdict quand une cause locale (pare-feu,
> interface qui change) n'est pas exclue.

---

## 6. Pipeline vidéo

### 6.1 Écarts délibérés avec Discord

| Limite chez Discord | Correction |
|---------------------|------------|
| Sous-échantillonnage 4:2:0 — couleur au quart de la résolution, texte illisible | **4:4:4** — couleur pleine résolution. **Confirmé sur NVIDIA Turing ou plus récent uniquement** (RTX 20xx et au-delà) — voir §6.4 |
| Débit plafonné (~2,5 Mbps, ~8 Mbps avec Nitro) | Plancher garanti défini par l'utilisateur, jusqu'à 100 Mbps |
| Congestion frileuse | Contrôle réécrit : descente lente, remontée rapide, jamais sous le plancher. **Non tenable en l'état** — voir l'encadré ci-dessous |

Le 4:4:4 est le facteur le plus déterminant pour la lisibilité du texte et du code.

> **Mesuré au jalon 0** (`spike/docs/rapport-jalon-0.md`, Q3). À cible commune de
> 10 Mbps sur un motif de texte à bords durs, HEVC 4:4:4 rend un PSNR de chrominance de
> 58,30 dB (U) et 48,58 dB (V), contre 18,21 / 19,29 dB pour H.264 4:2:0 et 18,12 /
> 19,27 dB pour AV1 4:2:0 — soit **+40,1 dB sur U et +29,3 dB sur V à débit comparable**,
> la luminance restant **bonne dans les trois cas** (50,2 à 74,9 dB) — non dégradée par le
> sous-échantillonnage, contrairement à la chrominance, sans que les trois valeurs soient
> proches entre elles pour autant. Deux codecs 4:2:0 différents
> convergent sur le même plancher de chrominance malgré des efficacités de luminance
> nettement différentes : l'écart est un artefact du sous-échantillonnage, pas le réglage
> d'un encodeur. **Ce que la mesure n'établit pas** : la lisibilité perçue à l'œil, qui
> reste un jugement humain.

> **Correction du jalon 0 — le régulateur pilote la cadence, pas le débit** (écart 5).
>
> La ligne « Congestion frileuse » ci-dessus se lit comme un ajustement fin et continu
> du **débit par image**. Ce n'est pas ce que le code peut faire aujourd'hui :
> `sky-encode` n'expose que la création d'une session (débit fixé à l'ouverture) et
> l'encodage d'une image, sans reconfiguration à chaud. Le seul levier restant au
> régulateur est de **sauter des images entières**.
>
> Les propriétés du régulateur lui-même sont, elles, démontrées analytiquement et par
> tests : plancher inviolable à n'importe quelle sévérité de perte, descente bornée à
> 15 % par tick, remontée au plafond en un tick. C'est la grandeur pilotée qui n'est pas
> celle annoncée.
>
> **Conséquence :** la promesse « en cas de congestion, perdre des images plutôt que de
> la netteté » n'est pas violée, elle est **vide** — l'autre terme du choix n'existe pas.
> **Le jalon 2 ne peut pas tenir ce §6.1 sans étendre l'API de `sky-encode` pour
> reconfigurer le débit à chaud** (`nvEncReconfigureEncoder`), ce que le matériel sait
> faire.

### 6.2 Chaîne de traitement

```
ÉCRAN → [ GPU : capture → redimensionnement → encodage ] → chiffrement → réseau
```

**La texture ne redescend jamais en mémoire centrale.** Seul le flux compressé
traverse le processeur. Un pipeline avec allers-retours GPU↔RAM saturerait un
processeur haut de gamme dès le 1440p60.

> **Mesuré au jalon 0** (Q4) : chaîne complète capture → encodage → réseau en 1440p60
> HEVC 4:4:4 à 30 Mbps sur RTX 4060, **0,53 % de processeur en médiane, 1,26 % au pic**,
> pendant que l'encodeur matériel travaille à 25 % en médiane (jamais nul). Deux mesures
> indépendantes concordent. L'ordre de grandeur « 1-3 % de CPU » est donc confirmé à
> cette résolution ; **le 4K 144 fps n'a pas été mesuré** et reste une extrapolation.

### 6.3 Capture Windows

**Windows.Graphics.Capture**, deux modes : écran entier (y compris jeux en plein
écran exclusif) et fenêtre précise.

Le curseur est capturé séparément et incrusté à l'affichage chez le spectateur : il
reste net malgré la compression et peut être masqué.

Audio via **WASAPI en mode loopback**, encodé en Opus à 256-510 kbps (contre 64-96
kbps chez Discord).

### 6.4 Codecs

Détection matérielle au premier lancement, puis négociation avec chaque spectateur.

Le tableau ci-dessous a été **corrigé après le jalon 0**. Trois de ses quatre lignes
étaient factuellement fausses ; le détail et les preuves sont dans
`spike/docs/rapport-jalon-0.md` (écarts 1, 2 et 6).

| Codec | Condition | Gain | Texte net (4:4:4) ? |
|-------|-----------|------|---------------------|
| **HEVC 4:4:4** | **NVIDIA Turing ou plus récent** (RTX 20xx et au-delà, septembre 2018) — mesuré sur RTX 4060. **Pas Pascal (GTX 10xx), pas Maxwell, pas Volta** : ces générations ne font pas de HEVC 4:4:4 du tout | ~30 % de moins que H.264, et la seule combinaison qui tienne à la fois sa cible de débit et le 4:4:4 | **Oui** — choix par défaut pour le partage d'écran |
| **AV1 4:2:0** | RTX 40+, RX 7000+, Arc | ~40 % de débit en moins à qualité égale | **Non** — voir écart 1. Pertinent pour partager de la *vidéo*, pas un écran de travail |
| **H.264 4:2:0** | Matériel des 12 dernières années | Socle universel de repli | **Non** |
| **H.264 4:4:4** | ⚠ **Ne pas utiliser en l'état** — voir écart 2 | — | Oui sur le papier, mais débit incontrôlable |
| **x264 logiciel** | Aucun encodeur matériel 4:4:4 détecté | **Une voie parmi cinq**, et celle que la note de référence du 23/08/2026 classe **dernière** : elle ne traite que l'émetteur, alors que le spectateur doit encore savoir décoder du 4:4:4. Coûte du processeur — signalé à l'utilisateur. **Coût réel non mesuré** : aucune mesure du jalon 0 ne couvre l'encodage logiciel | Oui, au prix du processeur |

**Écart 1 — AV1 ne fait pas de 4:4:4.** NVENC ne produit pas de 4:4:4 en AV1, même sur
architecture Ada. Vérifié par énumération matérielle des formats d'entrée par codec, puis
en sortie : un flux AV1 réellement produit sort en `Main` / `yuv420p`. **AV1 et « texte
net » s'excluent sur ce matériel.**

**Écart 2 — H.264 4:4:4 ne respecte pas la cible de débit.** Mesuré sur RTX 4060 (pilote
610.74) : 70,93 Mbps réels pour une cible de 10 Mbps, et un débit bloqué entre 67 et
72 Mbps quelle que soit la cible demandée entre 3 et 20 Mbps — donc indépendant de la
cible. Son p99 d'encodage est le double des autres. Les paramètres ont été journalisés et
sont identiques à ceux de H.264 4:2:0, qui tient sa cible : l'anomalie est propre au
profil High 4:4:4 Predictive sur cette combinaison matériel/pilote, non généralisée à
d'autres. Un codec dont le débit ne se pilote pas est inutilisable ici : ni plancher
garanti, ni plafond respecté, ni couches de qualité multiples (§6.5).

**Écart 6 — le 4:4:4 n'existe pas sur AMD, et n'est pas attesté sur Intel.** *Établi par
recherche documentaire, non testé sur matériel : aucune carte AMD ni Intel n'était
disponible au jalon 0.* Le SDK d'encodage AMF ne comporte aucune surface 4:4:4 ; sur
RDNA 3, soumettre un format 4:4:4 renvoie `AMF_INVALID_FORMAT`. Ce n'est pas une limite de
génération, la capacité est absente de la plateforme (le 4:4:4 ajouté en AMF 1.5.0
concerne le convertisseur de couleur, pas l'encodeur). Côté Intel, la documentation atteste
le 4:2:2 sur certaines configurations, aucune source ne confirme le 4:4:4 en encodage.

**Et la frontière n'est pas le fabricant, c'est la génération.** Le HEVC 4:4:4 commence à
**Turing** (RTX 20xx, septembre 2018) : Pascal (GTX 10xx), Maxwell et Volta ne le font pas
du tout. Une GTX 1080 est donc du même côté de la frontière qu'une carte AMD vis-à-vis du
codec retenu ci-dessus. La formulation exacte de l'écart est
**« NVIDIA de 2018 ou plus récent contre tout le reste »**, et non « NVIDIA contre les
autres ». Voir `docs/superpowers/notes/2026-08-23-compatibilite-toutes-cartes-graphiques.md`,
§1.2 et §1.4.

> ### D1 — tranchée le 23/08/2026 : la voie A, sans renoncer à rien
>
> **Retenue : l'empaquetage** (voie A de la note du 23/08/2026). La couleur pleine
> résolution est rangée dans une image porteuse encodée en 4:2:0 ordinaire, puis
> recomposée sur le processeur graphique du spectateur.
>
> **Les trois exigences posées par le propriétaire sont cumulatives, aucune ne cède :**
> compatibilité de toutes les cartes, qualité, et fluidité. Le 4:4:4 natif reste utilisé
> là où le matériel le permet ; l'empaquetage prend le relais ailleurs, sans que le
> produit annonce deux niveaux de promesse.
>
> **Pourquoi cette voie et pas les quatre autres :** elle ne demande au matériel que
> d'encoder une vidéo ordinaire et d'exécuter un shader — deux capacités présentes sur
> pratiquement tout le parc des quinze dernières années. Elle résout d'un même geste
> l'encodage sur AMD et Intel **et** le décodage sur mobile et navigateur, où le 4:4:4
> est tout aussi absent. Elle ne consomme qu'une session d'encodage, ce qui préserve le
> budget des trois couches de qualité du §6.5.
>
> **Ce qui reste à mesurer avant d'écrire son spec** — neuf inconnues, section 4 de la
> note. Les deux qui peuvent invalider la voie A : le comportement à haute résolution,
> une image 1440p empaquetée dépassant la taille d'une 4K ; et les trois pièges
> identifiés — rangement respectant la nature de chaque plan, artefacts aux frontières,
> modèle perceptuel de l'encodeur qui ignore que la zone basse porte de la couleur.
>
> **Prérequis :** extraire un trait `VideoEncoder`, inexistant aujourd'hui, et confirmer
> les constats sur matériel AMD et Intel réel — aucun n'était disponible au jalon 0.
>
> Les trois voies ci-dessous sont conservées comme repli documenté, non comme options
> ouvertes.
>
> Il n'y a **pas** de socle universel en 4:4:4. Un utilisateur AMD — ou un utilisateur
> NVIDIA d'avant 2018 — conserverait le débit libre, la résolution et la cadence, mais
> retomberait en 4:2:0 pour la couleur, donc au niveau de Discord sur le point précis qui
> motive le projet. Contrairement aux autres écarts, aucune quantité de travail ne
> l'ajoutera : c'est une contrainte matérielle.
>
> Trois voies, aucune indolore, à arbitrer entre « sans compromis partout » et « sans
> compromis sur le matériel récent ». *Dans les trois, « hors NVIDIA » se lit « hors
> NVIDIA Turing ou plus récent ».*
> 1. Encodage **logiciel** 4:4:4 là où le matériel ne sait pas le produire — texte net
>    préservé, mais on troque potentiellement la différenciation « texte net » contre la
>    différenciation « ne coûte rien à la machine ». **Coût non mesuré au jalon 0**, donc
>    voie ni retenue ni écartée. *La note de référence citée ci-dessus la classe dernière
>    sur cinq : elle ne traite que l'émetteur, pas la capacité de décodage du spectateur.*
> 2. Accepter le 4:2:0 sur ce matériel, en le **disant dans l'interface** plutôt qu'en
>    laissant l'utilisateur croire à un défaut du logiciel.
> 3. Hybride : 4:4:4 matériel là où il existe, 4:4:4 logiciel sous un seuil de résolution
>    ailleurs, 4:2:0 au-delà.
>
> **Prérequis commun aux trois :** aucune abstraction d'encodeur n'existe aujourd'hui.
> `sky-encode` ne contient que l'implémentation NVENC en dur, sans trait `VideoEncoder`.
> La décision 8 du §2 promet une abstraction OS pour la capture ; il n'y a pas
> d'équivalent pour l'encodage, et brancher un second fabricant demandera de l'extraire
> d'abord.
>
> **Quatre voies supplémentaires sont décrites hors de ce spec**, dans la note de
> référence citée ci-dessus : empaquetage de la couleur dans une image porteuse 4:2:0
> (sa candidate n°1), flux auxiliaire, double résolution, 4:2:2 intermédiaire. Elle
> recommande un spike dédié plutôt qu'une décision sur plan, et **rien ne peut être
> engagé avant confirmation sur matériel AMD et Intel réel** : tout le constat de
> l'écart 6 est documentaire.

Réglages communs : pas d'images bidirectionnelles (latence), rafraîchissement
progressif au lieu d'images-clés complètes (supprime les pics de débit périodiques
et les micro-saccades).

> **Écart 3 — conséquence directe du groupe d'images infini, à traiter au jalon 2.**
> Le rafraîchissement progressif implique une seule image-clé, au tout début du flux, donc
> **un seul jeu d'en-têtes de séquence**. Mesuré : 1 VPS, 1 SPS, 1 PPS sur 1 201 images,
> malgré les drapeaux de répétition — ceux-ci n'ont d'effet qu'aux images-clés, dont il
> n'y en a qu'une. Coupés en leur milieu, les flux H.264, HEVC et AV1 sont tous refusés
> par le décodeur.
>
> **Un spectateur qui rejoint un partage en cours ne verrait rien** (§5.3, §5.4, §6.5).
> Correction identifiée, non implémentée : soit demander explicitement l'émission des
> en-têtes sur une image choisie à l'arrivée de chaque spectateur, soit les récupérer une
> fois et les transmettre **hors du flux vidéo** — la seconde option est la plus économe
> en débit.

**Portabilité, à inscrire comme non traité plutôt que comme acquis.** Le jalon 0 n'a visé
qu'une cible, `x86_64-pc-windows-msvc`. ARM64 Windows n'a jamais été abordé et constitue
une inconnue complète, y compris sur la disponibilité d'un encodeur exploitable. Apple
Silicon est prévu au jalon 7 via VideoToolbox, non vérifié.

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

> **Deux prérequis mesurés au jalon 0.** (1) Une couche qui naît en cours de partage
> doit émettre ses en-têtes de séquence pour le spectateur qui l'ouvre — voir l'écart 3
> au §6.4, sans quoi ce spectateur n'obtient qu'un écran noir. (2) Les couches ne peuvent
> pas être dérivées par simple ajustement du débit d'une session existante tant que la
> reconfiguration à chaud n'est pas exposée — voir l'encadré du §6.1.

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

> **Limite du jalon 0 à lever avant le jalon 3.** Le mode Auto ne dispose aujourd'hui que
> du basculement entre couches ; l'adaptation fine du débit *à l'intérieur* d'une couche
> suppose la reconfiguration à chaud de l'encodeur, qui n'est pas exposée — voir
> l'encadré du §6.1. Sans elle, le seul levier de dégradation est le saut d'images.

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

**Statut du jalon 0 au 23 août 2026 : GO CONDITIONNEL.** Quatre des six questions sont
closes positivement — encodage matériel 4:4:4 depuis une texture GPU, gain de chrominance
mesuré, charge processeur, et **propriétés du régulateur, closes « en théorie » seulement**
(démontrées analytiquement et par tests unitaires, jamais éprouvées sur une congestion
réseau réelle, et portant sur une grandeur que le code ne pilote pas encore — voir
l'encadré du §6.1 et le §11). Aucune n'a produit de réponse négative, et aucun des six
écarts constatés n'invalide le projet. Deux questions restent
ouvertes faute de mesures qu'aucun agent ne pouvait prendre : la **connexion entre deux
box** (risque n°1) et le **débit de capture sur écran en mouvement réel**. Rapport
complet, preuves et conditions du passage à un GO ferme :
`spike/docs/rapport-jalon-0.md`.

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
| NAT symétrique / CGNAT chez un pair | Connexion impossible (~5-10 % des paires — **chiffre issu de la littérature, ni vérifié ni infirmé par le jalon 0**) | IPv6 tentée en priorité, diagnostic explicite différencié (le programme distingue désormais « perçage raté » de « perçage réussi, canal en échec »), ajout ultérieur possible d'un relais TURN. **Risque n°1, non levé : aucun test du jalon 0 n'a franchi un NAT** |
| Contrôle de congestion maison instable | Image qui pulse ou fige sous charge réseau | **Non validé sur réseau réel au jalon 0** : les propriétés du régulateur sont démontrées analytiquement et par tests unitaires, mais le seul signal de congestion qu'il ait reçu est un taux d'échec d'envoi local, jamais une perte de paquets. Repli sur l'algorithme standard conservé en option |
| **4:4:4 indisponible hors NVIDIA Turing ou plus récent** | La différenciation principale du produit (texte net) disparaît pour les utilisateurs AMD, probablement Intel, **et tout le parc NVIDIA d'avant septembre 2018** (GTX 10xx comprises) — ils retombent au niveau de Discord sur la couleur. La population concernée est plus large que « les non-NVIDIA » | Contrainte matérielle, non contournable par du travail. Trois voies décrites au §6.4, quatre autres dans la note du 23/08/2026, **décision D1 tranchée : voie A, empaquetage — voir §6.4**. Prérequis : extraire un trait `VideoEncoder`, inexistant aujourd'hui |
| Heures de calcul Neon dépassées | Base suspendue, site Erinium affecté | Sync groupé en une requête ; frais explicitement acceptés par le propriétaire |
| Limite de sessions d'encodage GPU | Blocage au-delà de ~8 flux | Architecture simulcast : le nombre d'encodages est indépendant du nombre de spectateurs |
| Portage macOS sans matériel de test | Jalon 7 bloqué | Runners macOS GitHub pour la construction ; test réel requis avant publication |
