# Essai « toutes cartes », sous-jalon 1 — fiche pour le propriétaire

Ce que l'essai prouve : **une machine sans NVIDIA récente reçoit et affiche un écran** — ici le
portable AMD (Ryzen 7 5700U, Vega 8) regardant la machine NVIDIA, en **HEVC 4:2:0** par Media
Foundation, ou en **H.264** si l'extension HEVC lui manque. Il relève aussi ce qu'aucun instrument
ne mesurait : la **latence de bout en bout**. Il ne prouve rien de la qualité du 4:4:4 (le
portable ne le décode pas) ni du partage depuis une machine non NVIDIA (sous-jalon 3).

Rédigée le 03/10/2026 en relisant le code et les rapports de tâche, **sans rien exécuter côté
spectateur AMD**. Tout ce qui est écrit « déduit » n'a jamais été observé. Seul le chemin
Media Foundation → affichage a été éprouvé, **sur la RTX 4060 seulement** : sur AMD, il ne l'a
été que par la sonde du 02/10 (décodage), pas par l'application.

Les commandes `cargo` et `sky-probe` se tapent dans Git Bash, **une par bloc, dans l'ordre**.
**Chaque commande ou geste qui ouvre une fenêtre ou un navigateur est annoncé juste au-dessus.**

---

## ⛔ Pendant tout l'essai

- **Ne jamais définir `RUST_LOG="str0m=debug"`** (ni aucun `RUST_LOG` visant `str0m`) : ses traces
  écrivent l'adresse des deux machines **en clair**, donc dans toute photo ou capture partagée.
- **Une seule application SkyShare à la fois par machine.** Elle synchronise toute seule et
  consommerait les messages de la négociation. Pour l'essai A, l'application de l'icône près de
  l'horloge doit être **fermée** (icône → **Quitter**). Pour l'essai B, quitte aussi l'ancienne
  version installée sur la machine NVIDIA : l'instance unique renverrait vers elle au lieu de
  lancer la nouvelle.
- Tant qu'un partage attend, **aucune commande `sky-probe`** sur la même machine.

---

## 0. Préparer la version portable (machine NVIDIA)

```
powershell -File D:\skyshare\spike\scripts\version-portable.ps1
```

Le script construit l'application empaquetée (`npm run build`, puis `tauri build`, **sans ouvrir
aucune fenêtre**), vérifie par `dumpbin` qu'**aucune DLL NVIDIA** n'est importée et que
`mfplat.dll` l'est, puis écrit `D:\skyshare\dist\SkyShare-portable\` (`SkyShare.exe` et cette
fiche) et `D:\skyshare\dist\SkyShare-portable.zip`. Il affiche la taille de l'archive et le SHA-256
de `SkyShare.exe` : **note-le**, tu le compareras sur le portable.

Après la construction, `git status` montre `spike/crates/sky-app/Cargo.toml` modifié : c'est
normal (fins de ligne), `git diff --ignore-cr-at-eol` doit être vide. Ne pas le commiter.

**Copie l'archive sur le portable** (clé USB, partage réseau — au choix), décompresse-la, et
vérifie l'empreinte (PowerShell) :

```
Get-FileHash -Algorithm SHA256 .\SkyShare.exe
```

Elle doit être celle affichée par le script.

**Aucune installation.** `SkyShare.exe` se lance directement, mais **l'application n'est pas
signée** : Windows peut afficher « Windows a protégé votre ordinateur » (éditeur inconnu) ; c'est
attendu (la signature relève du jalon 6). Le choix d'exécuter quand même t'appartient.

---

## 1. Essai A — la machine NVIDIA seule : Media Foundation sur NVIDIA

Ce qu'il prouve : le chemin Media Foundation (HEVC 4:2:0 puis H.264) s'affiche juste, sur la
machine où il a été développé, **avant** de dépendre du portable. **Il ne prouve rien de l'AMD.**

**Montage — deux identités sur la machine.** `view` ne regarde qu'un **ami** : hôte et spectateur
sur une même machine exigent deux comptes. Reprendre **tel quel** le montage du §3 de
`spike/docs/essai-jalon-2.md` (hôte en `debug` sous le **second** compte Discord, déjà préparé si
l'essai du jalon 2 a été fait ; spectateur `sky-probe view` en `--release` sous le compte principal),
avec ses mises en garde sur le compte à choisir dans le navigateur. Si la préparation n'est pas
faite, c'est le moment (une seule fois). *Déduit du code, jamais essayé.*

Construire le spectateur en `release` :

```
cd /d/skyshare/spike && cargo build --release -p sky-probe
```

Hôte, image de test (**ne pas** changer `--format` : `auto` propose tout ce que la carte encode,
et la négociation retient le meilleur format commun avec ce que le spectateur offre) :

```
cd /d/skyshare/spike && cargo run -p sky-probe -- host --source synthetique --seconds 90
```

### A1 — H.264

> **⚠️ La commande suivante OUVRE UNE FENÊTRE** « SkyShare — écran de … », après la connexion.
> La fermer par la croix arrête le visionnage.

```
cd /d/skyshare/spike && ./target/release/sky-probe.exe view "NomDuSecondCompte" --format h264 --seconds 60
```

**Attendu** : `format négocié : H.264` **dans les deux terminaux** (hôte et spectateur), et l'image
affichée dans la fenêtre.

### A2 — HEVC 4:2:0

Relancer l'hôte (même commande), puis :

> **⚠️ La commande suivante OUVRE UNE FENÊTRE** « SkyShare — écran de … ».

```
cd /d/skyshare/spike && ./target/release/sky-probe.exe view "NomDuSecondCompte" --format hevc420 --seconds 60
```

**Attendu** : `format négocié : HEVC 4:2:0` des deux côtés, l'image affichée.

Supprime ensuite `spike/recu.h265` (flux reçu, exclu par git).

---

## 2. Essai B — deux machines : la NVIDIA partage, le portable AMD regarde

C'est **le critère de fin du sous-jalon** (spec §8). Le portable regarde par la **version
portable**, avec son compte (le second compte Discord, **ami** du compte principal : celui du C2).

### Sur la machine NVIDIA

1. Quitte l'application de l'icône près de l'horloge (icône → **Quitter**), puis lance
   `D:\skyshare\dist\SkyShare-portable\SkyShare.exe` (compte principal).

   > **⚠️ Cela OUVRE LA FENÊTRE de l'application** et pose son icône près de l'horloge.

2. Pour la mesure de latence (§3), affiche à l'écran **partagé** un chronomètre **à la
   milliseconde** (une page de chronomètre dans le navigateur convient ; *non vérifié* : choisis
   une page qui tourne en continu et **ne fige pas** quand l'écran ne change pas).
3. Clique **« Partager mon écran »**, choisis l'écran qui porte le chronomètre.

### Sur le portable AMD

1. Lance `SkyShare.exe` du dossier décompressé.

   > **⚠️ Cela OUVRE LA FENÊTRE de l'application** et pose son icône près de l'horloge.

2. Première fois : **« Se connecter »** ouvre le **navigateur par défaut** sur la page
   d'autorisation Discord.

   > **⚠️ Cela OUVRE LE NAVIGATEUR.** Lis le nom du compte affiché par Discord : ce doit être le
   > **second** compte, ami du compte principal. Sinon, n'autorise pas.

3. Attends que l'ami apparaisse comme partageant (la synchronisation est de 30 s au premier plan,
   *argumenté, non mesuré*), puis clique **« Regarder »**.

   > **⚠️ Cela OUVRE UNE FENÊTRE** native « SkyShare — écran de … ».

### Ce qu'il faut voir (attendu)

- Le panneau de partage affiche **« Format : HEVC 4:2:0 »** — ou **« Format : H.264 »** si
  l'extension HEVC du Store manque sur le portable (c'est son rôle de repli, **non éprouvé** sur
  cette machine sans l'extension). **Pas** « HEVC 4:4:4 » : le portable ne le décode pas.
- L'image du chronomètre s'affiche dans la fenêtre native, **en couleurs justes**.
- Côté machine NVIDIA, le panneau affiche le même format.
- **Quand tu arrêtes** : « Le partage s'est arrêté » (côté portable), pas un échec.

---

## 3. Ce qu'il faut relever

**« Non mesuré » est une réponse acceptable. Une estimation présentée comme une mesure ne l'est
pas.**

| Grandeur | Où la lire | Remarque |
|---|---|---|
| **Format négocié** | panneau (« Format ») ou ligne `format négocié` du terminal | HEVC 4:2:0 ou H.264 |
| **Images reçues par seconde** | panneau (« Images reçues ») | images **arrivées**, pas affichées ; l'hôte vise 60 im/s |
| **Latence de décodage** | panneau (« Décodage ») | une **moyenne** par relevé, pas une médiane |
| **Images écartées** | panneau | quelques-unes au démarrage sont normales ; un compte qui **monte en continu** n'en est pas un |
| **Latence de bout en bout** | photo, voir ci-dessous | **le chiffre attendu de cet essai** (spec §2, inconnue 1) |
| Débit, gigue | panneau | pour mémoire |

**Latence de bout en bout.** Avec le chronomètre à la milliseconde affiché sur l'écran partagé,
**photographie ensemble l'écran de la machine NVIDIA et celui du portable**, dans le même cadre,
d'un seul déclenchement (téléphone). **L'écart entre les deux chronomètres est la latence.**
Prends **plusieurs photos** (au moins cinq) : à 60 im/s un chronomètre à la milliseconde se
brouille, note la valeur lisible et l'écart relevé sur chacune. Ce n'est **pas** le « transit »
du bilan de `sky-probe`, qui compare deux horloges d'origines différentes.

## 4. Ce qui doit faire arrêter l'essai

Arrête, ne cherche pas à corriger, et **garde le texte du terminal et des photos** si :

- **une image figée sans aucun message** (le flux continue d'arriver, l'image ne bouge plus) ;
- **les couleurs sont inversées** (rouge et bleu échangés) : une matrice ou un ordre de canaux
  faux sur le chemin NV12, **jamais éprouvé sur AMD** ;
- **une fenêtre noire muette** : ni image, ni état (« En attente de l'image… », « Le partage
  s'est arrêté », « Connexion perdue »), ni message d'erreur.

Un message d'erreur clair (par exemple « Cette machine ne sait décoder en matériel aucun des
formats vidéo de SkyShare… ») n'arrête pas l'essai : **recopie-le tel quel**, c'est un résultat.

## 5. Ce qu'il faut me rapporter

Les cases du §2 (« Ce qu'il faut voir »), le tableau du §3 rempli — « non mesuré » partout où rien
n'a été relevé —, les photos de latence, ce que tu as vu aux essais A1 et A2 (ou « non fait »), le
texte des terminaux — **déduit du code** : aucune adresse IP n'y figure tant qu'aucun collecteur
de traces n'est installé et que `RUST_LOG` n'est pas défini ; relis-les quand même avant de les
partager —, et tout message d'erreur, même passager, recopié tel quel.

**Si l'image est dégradée** (saccades, débit au plafond sans raison) : le régulateur de l'hôte n'a
**plus aucune mesure de RTT** et reste bloqué au plafond sur le chemin nominal (voir `tasks/todo.md`,
jalon 2) ; c'est la première chose à regarder, avant de soupçonner le décodeur.
