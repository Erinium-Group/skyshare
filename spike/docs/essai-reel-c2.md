# Essai réel du jalon C2 — mode d'emploi

Ce que l'essai prouve : deux machines, sur deux réseaux et deux comptes Discord
différents, négocient leur connexion **par le site**, sans que personne ne copie
un bloc de texte à la main. C'est la dernière chose qui manque pour clore le jalon.

Durée : environ vingt minutes, la première connexion comprise.

---

## Qui fait quoi

| | Machine A — toi | Machine B — ton correspondant |
|---|---|---|
| Rôle | **émet** (`host`) | **reçoit** (`view`) |
| Carte graphique | **NVIDIA obligatoire** (ta RTX 4060) | n'importe laquelle |
| Système | Windows | Windows |
| Compte Discord | le tien | **le sien, différent du tien** |
| Réseau | ta connexion | **une autre connexion** — pas ton Wi-Fi |

Rien à installer : un seul fichier `sky-probe.exe` à lancer depuis un terminal.
Aucun identifiant n'est échangé entre vous : chacun se connecte à son propre compte.

---

## Étape 1 — chacun de son côté : se connecter et enregistrer sa machine

Dans un terminal, dans le dossier où se trouve `sky-probe.exe` :

```
sky-probe.exe login
```

Le navigateur s'ouvre sur la page de connexion Discord. S'il y a une double
authentification, elle se fait là. Le terminal reprend la main tout seul et
affiche « Connecté en tant que … ».

```
sky-probe.exe device register "PC-salon"
```

Le nom est libre : il s'affichera chez l'ami. Une seule fois par machine.

```
sky-probe.exe code
```

Affiche ton code ami, de la forme `SKY-ABCD-EFGH`. **Échangez vos deux codes**
(Discord, SMS, peu importe : un code ami ne donne aucun accès, il permet
seulement d'envoyer une demande).

## Étape 2 — devenir amis

Machine A :

```
sky-probe.exe friends add SKY-CODE-DE-B
```

Machine B :

```
sky-probe.exe friends list
```

La demande reçue apparaît avec un numéro. Reprends ce numéro :

```
sky-probe.exe friends accept 12
```

Chacun relance `friends list` : l'autre doit apparaître dans les amis, avec son
appareil. **Note le nom Discord exact de A**, il sert à l'étape suivante.

## Étape 3 — l'essai

Machine A, **en premier** (elle attend jusqu'à trente minutes) :

```
sky-probe.exe host --source synthetique --seconds 15
```

`--source synthetique` envoie une image de test au lieu de ton écran réel. C'est
la connexion qu'on mesure, pas la vidéo : rien de ton écran ne part.

Machine B, ensuite :

```
sky-probe.exe view "NomDiscordDeA" --seconds 15
```

Le nom est celui vu dans `friends list`, guillemets compris s'il contient un espace.

**Pendant l'attente, sur les deux machines : aucune autre commande `sky-probe`.**
Ni `login`, ni `friends list`, ni `device list`, ni `code`. Chacune de ces
commandes efface les messages en attente et casse la négociation en silence —
c'est une limite connue, pas un accident.

## Étape 4 — ce qu'il faut rapporter

1. Le **délai** entre le lancement de `view` et le moment où le terminal annonce
   la connexion.
2. Le **nombre de synchronisations** affiché de chaque côté.
3. Si une **enveloppe est restée non ouverte**, ou tout message d'erreur, même
   passager — recopié tel quel.
4. Ce qui s'est passé s'il a fallu s'y reprendre à deux fois.

Une capture d'écran du terminal suffit pour les trois premiers points.

---

## Si ça ne marche pas

- **« … n'a pas répondu — est-il en partage ? »** → `host` n'était pas lancé, ou
  il avait fini ses trente minutes, ou une autre commande a consommé le message.
  Relancer `host`, puis `view`.
- **`view` ne trouve pas l'ami** → le nom doit être celui de `friends list`,
  exactement.
- **Rien ne se passe pendant longtemps** → `sky-probe.exe netcheck`, sur chaque
  machine séparément, dit si le réseau autorise une connexion directe.
- **Autre chose** → tout arrêter (Ctrl+C) et garder le texte du terminal.

Aucune adresse IP n'est affichée par ces commandes, et aucune n'est enregistrée
par le site : la capture d'écran du terminal peut être partagée sans risque.

---

## À savoir avant de recommencer un autre jour

Une session dure sept jours. Passé ce délai, il faut refaire `login`, qui
**révoque l'appareil courant et le réenregistre** automatiquement. C'est normal —
mais ne jamais le lancer pendant une attente en cours.
