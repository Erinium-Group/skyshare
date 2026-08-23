# Mesures que les agents ne peuvent pas produire

Ces chiffres exigent un écran réel en mouvement, un jugement visuel, ou une seconde
machine. Un agent peut écrire et compiler le code qui les mesure ; il ne peut pas
fournir les valeurs. Deux d'entre elles — **M4** et **M1** — décident du passage du
jalon 0 d'un GO conditionnel à un GO ferme.

**Pourquoi ce fichier est dans le dépôt.** Les rapports de tâche du jalon 0 vivent
dans `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/`, répertoire délibérément exclu
du dépôt par `.gitignore` : ce sont des artefacts de travail, pas un livrable. La
conséquence n'avait pas été tirée : les commandes des deux mesures qui décident du
jalon n'existaient que là. Elles sont rapatriées ici, avec leurs avertissements. Ce
document est la source à jour ; toute copie hors dépôt est de seconde main.

Chaque section indique la commande exacte, ce qu'il faut relever, et le seuil.

---

## M4 — Test pair-à-pair avec un correspondant distant (Q5) · Tâche 7

**La mesure la plus importante du jalon : c'est le risque n°1 du projet.**

1. Envoyer `spike/target/release/sky-probe.exe` et `spike/README-AMI.md` à un
   correspondant, idéalement sur un autre fournisseur d'accès.
2. Lancer localement — **avec la source synthétique, ce n'est pas optionnel** :

```bash
cargo run --release -p sky-probe -- host --source synthetique --seconds 5
```

3. Suivre le protocole de copier-coller des blocs `SKY1:` décrit dans `README-AMI.md`.

> ### ⚠️ Ne pas lancer `sky-probe host` sans options
>
> Sans options, la commande prend ses valeurs par défaut :
> `--source ecran --seconds 30 --codec hevc444 --bitrate-mbps 30`. Elle capture
> alors **l'écran réel de l'opérateur** pendant 30 secondes, l'encode et l'envoie —
> et le programme du correspondant écrit ce flux **sur son disque**, dans un fichier
> `recu.h265` déposé dans son répertoire courant, pouvant atteindre ~112 Mo.
>
> **Q5 ne demande aucune vidéo** : la connexion s'établit ou elle ne s'établit pas.
> La source synthétique répond exactement à la même question, sans exposer le bureau
> de l'opérateur ni encombrer le disque du correspondant. Cinq secondes suffisent :
> le chronomètre ne démarre qu'une fois la connexion établie.
>
> **Origine de cet avertissement.** `README-AMI.md` a été écrit en Tâche 7, quand le
> programme n'envoyait que des octets de remplissage, et promettait donc de bonne foi
> qu'aucun écran n'était capturé et que rien ne s'écrivait sur le disque. La Tâche 8 a
> branché la vraie chaîne vidéo sans que ce fichier figure dans sa liste de fichiers à
> toucher. Aucune revue au périmètre d'une tâche ne pouvait voir la contradiction :
> elle est née *entre* les deux. Relevé par la revue de branche.

À relever :

- **Le texte exact affiché des deux côtés** — capture d'écran entière, pas un résumé.
- Délai d'établissement, compté **après le collage du second bloc**.
- Fournisseurs d'accès des deux côtés, et type de connexion (fibre / câble / 4G).

**Seuil : connexion établie en moins de 8 secondes après le collage du second bloc.**

> ### Le diagnostic distingue trois situations — les relever littéralement
>
> Les messages ont été corrigés pour n'affirmer que ce qu'ils observent. Ne pas les
> résumer, les recopier :
>
> - **« cause probable : NAT strict »** → le perçage a échoué. C'est la vraie réponse
>   négative à Q5.
> - **« le canal de données ne s'est pas ouvert… NAT n'est PAS en cause »** → le
>   perçage a réussi, c'est l'établissement du canal chiffré qui a échoué. Réponse
>   **positive** à Q5, avec un autre problème derrière.
> - **« aucun paquet ne nous est parvenu »** (côté spectateur) → ambigu par
>   construction : soit le correspondant n'a pas encore collé le bloc de son côté,
>   soit il l'a fait et ses paquets n'ont pas franchi le réseau. **Aucun des deux
>   bords ne peut choisir entre les deux** — ne rien conclure sur Q5 et recommencer,
>   c'est sans risque.
>
> Si un compteur d'erreurs de socket est mentionné, le noter aussi : il nuance le
> verdict et a déjà signalé, dans un test, une cause locale que le message aurait
> sinon imputée au réseau.
>
> **La fenêtre est de 10 minutes** entre l'affichage de la réponse du correspondant et
> le collage du bloc, avec un rappel toutes les 30 secondes. Elle était de 2 minutes,
> et une minuterie interne coupait en réalité à 30 secondes : sans ces deux
> corrections, ce test aurait échoué à chaque fois pour une raison sans aucun rapport
> avec le réseau.

> ### ⚠️ N'activer aucun journal réseau détaillé
>
> Une version antérieure de cette checklist proposait `RUST_LOG="str0m=debug"` en cas
> d'échec. **Ne pas le faire.** Le filtrage de données personnelles de la bibliothèque
> ne couvre pas son point de trace le plus volumineux : les adresses des deux machines
> sortiraient en clair dans la console, et donc dans toute capture d'écran partagée
> ensuite.
>
> Le diagnostic intégré ci-dessus est ce qu'on a de mieux, et il est honnête sur ses
> limites. **Il ne désigne aucun côté fautif** — aucun des deux bords ne peut savoir ce
> qui se passe chez l'autre. C'est la confrontation des deux écrans, et non l'un des
> deux seul, qui permet de conclure.

Un échec est une information aussi utile qu'un succès : il indiquerait qu'un relais est
nécessaire, ce qui rouvre une décision d'architecture.

---

## M1 — Capture en mouvement réel (Q1) · Tâche 2

```bash
cargo run --release -p sky-probe -- capture --seconds 30
```

Pendant les 30 secondes : **déplacer des fenêtres, faire défiler une page.** Sans
mouvement, Windows.Graphics.Capture ne livre presque aucune image et la mesure ne veut
rien dire — c'est exactement la raison pour laquelle aucun agent n'a pu produire ce
chiffre.

À relever : `FPS moyen` · `Images capturées` · `Délais dépassés` · UC du processus
(Gestionnaire des tâches → Détails → colonne UC).

**Seuil : ≥ 59 fps.**

> ### ⚠️ M1 ne peut clore que la moitié de Q1
>
> Le plan fixe Q1 à « ≥ 59 fps **et < 1 % d'images perdues** ». Cette commande ne
> mesure que la première moitié.
>
> Le compteur affiché sous `Délais dépassés` (`CaptureStats.dropped`) **ne compte pas
> des images perdues** : il compte les appels à `next_frame` sortis sur expiration du
> délai d'attente, ce que `sky-capture/src/wgc.rs` documente lui-même. Un écran qui ne
> change pas incrémente ce compteur sans qu'aucune image ait été perdue.
>
> **Rien dans cette branche ne mesure le taux réel de perte d'images.** L'instrument
> reste à écrire : il faudrait comparer les images livrées par WGC au nombre d'images
> qu'il aurait dû livrer sur la période, ce que l'API ne donne pas directement. À
> inscrire au jalon 2, avec la chaîne d'affichage. Tant qu'il n'existe pas, un M1 au
> seuil ferme la question du débit, pas celle de la perte.

---

## M2 — Validation visuelle du bitstream (Q2) · Tâche 3

```bash
ffplay test_ecran20.h265
```

À relever : la vidéo se lit-elle ? L'image correspond-elle à l'écran capturé ?
Artefacts visibles ?

Seuil : lecture correcte, pas de bloc corrompu.

> ### ⚠️ Piège de la première seconde — à connaître avant de juger la qualité
>
> **Ne jamais juger sur la première seconde.** Le plan impose un tampon de sortie
> d'une seule image, réglage délibéré pour supprimer les pics de débit et donc les
> micro-saccades. Conséquence mécanique : la toute première image — la seule
> entièrement autonome, en 1440p, tout intra, en 4:4:4 — est plafonnée à environ
> 500 kbit et sort franchement dégradée. Le groupe d'images étant infini, elle reste
> dans la chaîne de références jusqu'à ce que le rafraîchissement progressif l'ait
> entièrement remplacée, soit environ deux secondes.
>
> **Avancer de 3 à 5 secondes avant de porter un jugement.** Sinon on conclurait que
> le 4:4:4 ne tient pas ses promesses en regardant l'artefact d'un réglage de latence.
>
> Si la qualité paraît malgré tout mauvaise en régime établi, les deux suspects sont,
> dans l'ordre : les paramètres de signalisation couleur (matrice et plage) —
> normalement corrigés depuis —, puis ce plafond de tampon, qu'on peut relâcher au
> prix de pics de débit.

---

## M3 — Jugement de lisibilité comparée entre les 4 encodages (Q3) · Tâche 4

```bash
cargo run --release -p sky-probe -- codecs --seconds 15 --bitrate-mbps 10
```

Les images déjà extraites et recadrées sur le panneau de texte sont dans
`spike/mesures/` (`crop-*.png`, `frame120-*.png`) : le jugement peut se faire sur
elles, sans réexécuter la mesure. Comparer à 100 % de zoom.

À relever, pour chacun des 4 : texte lisible / flou / illisible · franges colorées sur
le texte · préférence subjective.

C'est le jugement humain qui tranche : le PSNR dit qu'il existe une distance numérique
entre la couleur décodée et l'originale, il ne dit pas si un caractère reste
reconnaissable à l'œil. **Le même piège de la première seconde vaut ici** — les images
extraites le sont déjà à l'index 120 pour cette raison.

---

## M5 — Charge processeur relevée sur écran réel (Q4) · Tâche 8

```bash
cargo run --release -p sky-probe -- host --seconds 60 --codec hevc444 --bitrate-mbps 30
```

Non bloquante : deux mesures instrumentées concordent déjà. Cette mesure ne sert qu'à
confirmer au Gestionnaire des tâches ce que l'instrumentation a mesuré.

À relever : UC du processus (Gestionnaire des tâches → Détails) et surtout le graphe
« Video Encode » du GPU (onglet Performance → GPU). Ce dernier doit être nettement non
nul : c'est la preuve que l'encodage se fait sur le matériel dédié et non sur le
processeur.

Seuil : UC < 5 %.

*Contrairement à M4, cette mesure-ci veut bien la source écran : c'est son objet. Elle
suppose donc un correspondant averti, ou un spectateur lancé sur la même machine.*

---

## M6 — Latence de bout en bout par photographie · Tâche 8

Méthode de référence, celle utilisée pour évaluer Parsec et Moonlight :

1. Ouvrir un chronomètre en millisecondes, en plein écran, sur la machine émettrice.
2. Sur la machine réceptrice : `ffplay -fflags nobuffer -flags low_delay recu.h265`.
3. Photographier les deux écrans côte à côte.
4. Lire l'écart entre les deux chronomètres.
5. Répéter 5 fois, retenir la médiane.

À relever : les 5 valeurs et leur médiane.

Vérification de cohérence : médiane ≈ encodage p50 + RTT réseau + tampon. La somme des
composantes mesurées en boucle locale donne ≈ 5,06 ms, qui n'est qu'un **plancher** : il
y manque la capture elle-même et le décodage plus l'affichage côté spectateur. Un écart
important vers le haut signalerait un tampon caché dans la chaîne.

---

## Ce qui reste hors du dépôt, et pourquoi

Le répertoire `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/` contient les briefs et
les rapports détaillés des neuf tâches, les diffs de revue et le journal de bord. Il est
exclu du dépôt par `.gitignore`, délibérément : ce sont des artefacts de travail, dont le
volume et la durée de vie n'ont rien à voir avec ceux d'un livrable.

**La conséquence à assumer :** les renvois « *(Tâche N)* » du rapport de faisabilité
nomment leur source mais ne permettent pas de la rouvrir depuis le dépôt seul. Ils
indiquent d'où vient un chiffre, ils ne le rendent pas vérifiable après fusion. Ce qui
est vérifiable depuis le dépôt : ce document, les journaux PSNR/SSIM et les images de
`spike/mesures/`, le script `spike/mesures.sh` qui les produit, et le code lui-même.
