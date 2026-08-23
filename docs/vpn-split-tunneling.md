# Utiliser SkyShare avec un VPN

> Guide destiné aux utilisateurs. Vérifié le 23 août 2026 — les menus évoluent,
> la section « Le vocabulaire » plus bas reste valable même après une refonte
> d'interface.

## Pourquoi c'est nécessaire

SkyShare connecte les deux machines **directement**, sans passer par un serveur.
Un VPN commercial partage une même adresse de sortie entre des milliers
d'abonnés et attribue un port différent à chaque destination : l'adresse que ta
machine découvre n'est alors utilisable par personne d'autre que le serveur qui
la lui a annoncée. La connexion directe ne peut pas s'établir — ce n'est pas une
limite de SkyShare, c'est la façon dont ces VPN fonctionnent. Discord, Zoom et
Steam rencontrent exactement le même mur et basculent sur un relais.

La solution tient en une phrase : **dire à ton VPN d'ignorer SkyShare**, et lui
seul. Tout ton autre trafic reste protégé.

---

## Étape 1 — Vérifier que c'est bien le problème

Coupe ton VPN, relance la connexion. Si elle passe, le VPN est en cause et la
suite de ce guide s'applique. Si elle échoue toujours, le problème est ailleurs
(voir la fin du document).

---

## Étape 2 — Exclure SkyShare du VPN

L'application à exclure est **`sky-probe.exe`** pendant les tests, et
**`SkyShare.exe`** une fois l'application installée. Exclus les deux si le doute
existe.

### NordVPN (Windows)

1. Ouvre NordVPN, clique sur l'engrenage en bas à gauche
2. **Split Tunneling**
3. Choisis **« Disable VPN for selected apps »** (désactiver le VPN pour les
   applications sélectionnées)
4. **Add Apps** → sélectionne SkyShare

### Surfshark (Windows)

Surfshark n'appelle pas ça split tunneling mais **Bypasser**.

1. Engrenage ⚙️ → onglet **VPN settings**
2. Onglet **Bypasser**
3. Active **Bypass VPN**
4. **Select apps** → coche SkyShare

### Proton VPN (Windows)

1. Paramètres → **Split tunneling**
2. Active, puis choisis le mode **Exclude** (exclure)
3. Ajoute SkyShare
4. **Redémarre Proton VPN et SkyShare** — Proton n'applique la règle qu'au
   prochain lancement des applications concernées

### ExpressVPN (Windows)

1. Menu ☰ → **Options** → onglet **Général**
2. Sous **Split tunneling**, clique sur **Paramètres**
3. Coche **« Ne pas autoriser les applications sélectionnées à utiliser le
   VPN »**
4. Ajoute SkyShare

### CyberGhost (Windows)

CyberGhost passe par les **Smart Rules** plutôt que par un menu dédié : cherche
**Smart Rules → App Rules** et ajoute une règle excluant SkyShare.

---

## Le vocabulaire — pour tous les autres VPN

Chaque éditeur nomme la même fonction différemment, et c'est là que la plupart
des gens abandonnent. Cherche dans les paramètres de ton VPN l'un de ces
termes :

| Terme | Utilisé par |
|-------|-------------|
| **Split tunneling** | le plus répandu |
| **Tunnellisation fractionnée** | interfaces en français |
| **Bypasser** | Surfshark |
| **Smart Rules** / **App Rules** | CyberGhost |
| **App exclusions** / **Excluded apps** | plusieurs clients |
| **Inverse split tunneling** | mode inverse : n'y mets **pas** SkyShare |
| **Per-app VPN** | clients mobiles |
| **Allow apps to bypass VPN** | formulations Windows |

**Le piège du mode inverse.** Deux modes existent partout et ils font l'opposé
l'un de l'autre :

- **Exclure** (« disable VPN for selected apps ») → tu ajoutes SkyShare à la
  liste. **C'est celui-là.**
- **Inclure** (« enable VPN only for selected apps ») → seules les applications
  listées passent par le VPN. Dans ce mode, il ne faut **surtout pas** ajouter
  SkyShare.

Ajouter SkyShare dans le mauvais mode produit exactement le contraire de l'effet
recherché — c'est l'erreur la plus fréquente.

---

## Si ton VPN n'a pas cette fonction

Certains clients n'offrent aucun split tunneling sur ordinateur, en particulier
les versions gratuites et les applications distribuées par le Mac App Store.
Trois solutions, de la plus simple à la plus technique :

1. **Couper le VPN le temps de la session.** Le plus simple, et suffisant si le
   VPN sert au confort plutôt qu'à une nécessité.
2. **Activer la redirection de port** si ton VPN la propose (AirVPN, PIA, et
   Proton VPN sur les offres payantes). Elle rend l'adresse prévisible et le
   perçage redevient possible. Cherche **port forwarding** dans les réglages.
3. **Passer par un ami relais.** Si un autre membre du groupe est connecté sans
   VPN et joignable par vous deux, SkyShare fait transiter le flux par lui.
   Aucune configuration de ton côté — il faut simplement que quelqu'un soit en
   ligne.

---

## Étape 3 — Vérifier que l'exclusion a pris

Relance la connexion SkyShare. Si elle s'établit, c'est réglé.

Deux causes d'échec courantes quand la règle semble pourtant correcte :

- **L'application n'a pas été redémarrée.** La plupart des clients VPN
  n'appliquent la règle qu'au lancement suivant du programme exclu. Ferme
  complètement SkyShare et rouvre-le.
- **Le mauvais exécutable a été exclu.** Vérifie que le chemin pointe bien vers
  `SkyShare.exe` (ou `sky-probe.exe`) et non vers un raccourci.

---

## macOS et Linux

**macOS** : le split tunneling par application y est bien plus rare que sur
Windows, et absent des versions distribuées par le Mac App Store, qui ne peuvent
pas installer l'extension réseau nécessaire. Si ton client n'a pas l'option,
télécharge la version depuis le site de l'éditeur plutôt que depuis l'App Store,
ou coupe le VPN pendant la session.

**Linux** : les clients en ligne de commande gèrent souvent l'exclusion par
sous-réseau ou par application. Chez Proton VPN et NordVPN, la commande ressemble
à `nordvpn whitelist` ou `protonvpn-cli` avec une option d'exclusion — consulte
`--help`, la syntaxe varie selon les versions.

---

## Ce que SkyShare ne peut pas faire

Aucun logiciel ne perce un VPN commercial sans relais — ni SkyShare, ni Discord,
ni Zoom. Quand tu vois un autre logiciel « fonctionner sous VPN », c'est qu'il
fait passer ton flux par ses propres serveurs, ce qui a un coût que quelqu'un
paie. SkyShare fait le choix inverse : la connexion directe, gratuite et sans
intermédiaire, avec le relais par un ami comme filet de secours.
