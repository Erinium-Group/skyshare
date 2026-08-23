# Rapport de faisabilité — Jalon 0

Matériel : RTX 4060, driver 610.74, Windows 11 build 26200, 16 processeurs logiques.
Cible de compilation unique : `x86_64-pc-windows-msvc` (Rust 1.94.0, MSVC).
Réseaux testés : **aucun.** Toutes les mesures dites « réseau » ont été prises entre
deux processus d'une même machine, en boucle locale. Aucun paquet n'a traversé une
carte réseau, encore moins deux box. Le test entre deux fournisseurs d'accès reste à
faire — c'est la mesure **M4**.
Date des mesures : 22 et 23 août 2026. Rédaction : 23 août 2026.

---

## Comment lire ce rapport

Trois statuts, jamais mélangés :

- **Mesuré** — un chiffre produit par une exécution, avec sa commande et son nombre
  d'échantillons. C'est le seul statut qui autorise une affirmation.
- **Établi par lecture** — une propriété démontrée en lisant le code ou la
  documentation d'une bibliothèque, sans exécution qui la confirme. Plus faible qu'une
  mesure, plus fort qu'une intuition.
- **Non mesuré** — avec la raison. Une case vide honnête vaut mieux qu'une estimation
  déguisée en résultat, et ce rapport en contient plusieurs.

Tout chiffre cité ici renvoie à un rapport de tâche dans
`.superpowers/sdd/2026-08-22-jalon-0-faisabilite/`. Deux chiffres qui circulent dans
ces rapports sont volontairement **écartés** de celui-ci : voir « Deux chiffres que ce
rapport ne reprend pas », en fin de section latence.

---

## Réponses aux 6 questions

| Q | Question | Seuil | Mesuré | Verdict |
|---|----------|-------|--------|---------|
| Q1 | Capture 1440p60 sans copie CPU | ≥ 59 fps | **Débit d'images : non mesuré** (aucun agent ne peut produire un écran en mouvement réel — voir note 1). **Absence de copie processeur : établie par lecture** — aucun `Map`, aucun `CopyResource`, aucune texture intermédiaire ; `IDirect3DDxgiInterfaceAccess::GetInterface` rend directement l'`ID3D11Texture2D` du pool WGC. Corroborée indirectement par Q4. | **PARTIEL** |
| Q2 | NVENC accepte une texture D3D11 en 4:4:4 | bitstream valide | `ffprobe` : `codec_name=hevc`, `profile=Rext`, `pix_fmt=yuv444p`, 2560×1440. **1201 images encodées = 1201 images décodées**, `ffmpeg -f null` code de sortie 0, aucune ligne d'erreur. Les trois branches de codec exercées sur matériel réel. Repli FFmpeg du plan **non déclenché**. | **OUI** |
| Q3 | Meilleur codec pour le texte | comparatif | À cible commune 10 Mbps, régime établi (images 120-900, 781 images) : HEVC 4:4:4 (9,37 Mbps réels) rend **PSNR U 58,30 dB / V 48,58 dB**, contre H.264 4:2:0 (8,44 Mbps) **18,21 / 19,29 dB** et AV1 4:2:0 (10,04 Mbps) **18,12 / 19,27 dB**. Soit **+40,1 dB sur U et +29,3 dB sur V** à débit comparable. Luminance comparable dans les trois cas (50,2 à 74,9 dB). | **HEVC 4:4:4**, sur la chrominance. Lisibilité perçue **non jugée** (note 2) |
| Q4 | CPU de la chaîne complète | < 5 % | **0,53 % médian, 1,26 % au pic** (54 échantillons, 1/s sur 59 s, source synthétique, 1440p60 HEVC 4:4:4 à 30 Mbps). Encodeur matériel simultanément à **25 % médian, minimum observé 9 %, jamais nul**. Seconde mesure indépendante sur écran réel : 0,53 % médian / 1,01 % max, encodeur 24 % médian — concordante. | **OUI** |
| Q5 | Connexion entre deux box | établie < 8 s | **Non mesurée.** Aucun test n'a franchi un NAT : les deux processus tournent sur la même machine, la paire de candidats ICE retenue est hôte/hôte, le trafic ne quitte jamais la pile réseau de Windows. Le chemin de code que Q5 existe pour exercer n'a jamais été exécuté. | **OUVERTE** — risque n°1 |
| Q6 | Plancher de débit tenu | jamais franchi | 5 tests unitaires verts, **et** propriétés démontrées analytiquement pour *toute* entrée, pas seulement les cas testés : plancher tenu à n'importe quelle sévérité de perte, descente bornée à 15 %/tick par construction de la formule, remontée au plafond en 1 tick (100 ms) après un à-coup. **Jamais éprouvé sur un signal de congestion réseau réel** — le signal injecté en Tâche 8 est un taux d'échec d'envoi local, pas une perte de paquets. | **OUI en théorie** — mais voir écart n°5 |

**Note 1 — pourquoi Q1 reste partielle.** Windows.Graphics.Capture ne livre une image
que lorsque le contenu affiché change. Une mesure sans mouvement provoqué ne dit rien
du débit atteignable. Trois exécutions de 10 s ont bien produit des images (553, 553,
553) et un « FPS moyen » autour de 55, mais sur du mouvement **accidentel** — logs de
compilation qui défilent, curseur qui clignote — et non sur le déplacement de fenêtres
et le défilement de page que Q1 mesure. Ces chiffres ne valident ni n'invalident le
seuil de 59 fps, et ce rapport ne les porte pas au tableau. Ce que ces exécutions
établissent, elles : la chaîne D3D11 → WGC → `ID3D11Texture2D` fonctionne de bout en
bout, sans panique, sans blocage, code de sortie 0 à chaque fois.

**Ce qui rend Q1 néanmoins peu risquée.** Le pari architectural de Q1 n'est pas le
débit d'images — c'est l'absence de copie processeur. Or Q4 mesure 0,53 % de
processeur pendant que l'encodeur matériel travaille à 25 %, à 60,0 i/s soutenues en
2560×1440 sur 60 secondes. Un pipeline qui ferait redescendre chaque image en mémoire
centrale ne pourrait pas afficher ce profil. C'est une corroboration indirecte, pas la
mesure M1 ; elle ne dispense pas de la prendre.

**Note 2 — ce que Q3 n'établit pas.** Un PSNR de chrominance à 18-19 dB dit qu'il
existe une grande distance numérique entre la couleur décodée et l'originale. Il ne dit
pas si un caractère reste reconnaissable à l'œil, à distance de lecture, avec
l'antialiasing d'un vrai rendu de police — le motif de test est un pire cas à bords
durs, sans antialiasing. Aucune ligne de ce rapport n'affirme que le texte est
« lisible » ou « flou » : c'est le jugement humain **M3** qui tranchera, sur les images
extraites dans `spike/mesures/`.

**L'argument le plus solide de Q3**, et il ne dépend d'aucun jugement : H.264 4:2:0 et
AV1 4:2:0 — deux codecs différents, deux implémentations distinctes — convergent
étroitement sur U (18,21 contre 18,12 dB) et sur V (19,29 contre 19,27 dB), alors que
leur luminance diverge nettement (50,23 contre 55,28 dB : AV1 compresse Y sensiblement
mieux à ce débit). Ce plancher chroma commun malgré des efficacités différentes établit
qu'il s'agit d'un artefact du sous-échantillonnage 4:2:0 lui-même, pas du réglage d'un
encodeur particulier. C'est ce qui rend la conclusion robuste malgré l'exclusion de
H.264 4:4:4 du comparatif (écart n°2).

*Précision de traçabilité : l'écart cité (+40,1 dB / +29,3 dB) est celui du régime
établi, images 120 à 900. Une valeur de +38,6 dB / +28,5 dB circule encore dans les
notes du jalon : c'est le même écart calculé sur la séquence entière, avant correction
du script de mesure qui annonçait exclure la rampe du tampon VBV sans le faire.*

### La formulation qui compte pour Q5

Le spike **n'a pas répondu à Q5.** Il a rendu la réponse fiable quand elle sera
obtenue. C'est une valeur réelle, et il ne faut ni la surestimer ni la minimiser.

Trois mécanismes distincts pouvant produire une **fausse réponse négative** ont été
supprimés. Aucun des trois n'était visible dans une boucle locale réussie ; chacun,
isolément, aurait fait échouer le test réel ; et les trois auraient produit le même
symptôme indistinct — « ça ne marche pas ».

1. **La bibliothèque réseau ne découvre pas l'adresse publique.** Le plan l'affirmait ;
   `str0m` est une bibliothèque sans-IO et le dit dans son propre README. Sans le module
   de découverte ajouté, l'offre n'aurait contenu que l'adresse privée de la machine —
   traversée de NAT impossible à 100 %, sans qu'aucune ligne de code soit en cause.
   Vérifié après coup sur une offre réelle : `{'host': 1, 'srflx': 1}`, le candidat
   réfléchi est bien présent.
2. **Un champ mal renseigné aurait fait rejeter silencieusement tous les paquets
   entrants distants.** L'agent ICE n'apparie un paquet entrant qu'avec un candidat hôte
   dont l'adresse égale la destination déclarée ; y annoncer l'adresse publique fait
   jeter tous les sondages. Défaut commis, puis trouvé en lisant le code de la
   bibliothèque — aucun symptôme en local.
3. **Une minuterie interne coupait à 30 secondes.** La poignée de main chiffrée
   abandonnait après cinq réémissions à délai doublant (1+2+4+8+16 s), quelle que soit
   la fenêtre d'attente affichée. Un aller-retour humain de blocs de 3 500 à 3 800
   caractères par messagerie dépasse presque toujours trente secondes. Corrigé à la
   racine (l'horloge présentée à la bibliothèque ne démarre plus qu'au premier paquet
   reçu) ; vérifié par une exécution où le collage est retardé de 180 s, puis par une
   fenêtre d'attente complète de 600 s tenue sans dérive.

À quoi s'ajoute une quatrième famille de défauts, comptée séparément par le rapport de
la Tâche 7 : les deux bords
**affirmaient des causes qu'ils ne pouvaient pas connaître**. L'émetteur accusait le NAT
même quand le perçage avait réussi et que seul le canal chiffré avait échoué ; le
spectateur affirmait « personne n'a jamais essayé de nous joindre » là où il ne pouvait
observer qu'une chose — aucun datagramme reçu. Les deux messages sont désormais
différenciés, et le compteur d'erreurs de socket ajouté au diagnostic a immédiatement
prouvé son utilité en signalant, dans un test, une cause locale que le message aurait
sinon imputée au réseau.

---

## Latence de bout en bout

| Composante | Valeur | Statut |
|---|---|---|
| Encodage p50 / p99 | **4,88 ms / 5,96 ms** | **Mesuré** — 3 599 échantillons, HEVC 4:4:4 2560×1440 à 30 Mbps, source synthétique, code courant. Couvre l'appel complet : enregistrement, mappage, encodage, attente du flux, copie des octets, démappage, désenregistrement. |
| RTT réseau | **Non mesuré** | Les 16,90 ms de médiane relevés en Tâche 8 mesurent le chiffrement DTLS et le transport SCTP **en mémoire**, entre deux processus du même noyau. Ils ne prédisent rien d'un vrai lien. |
| Transit sur le lien | 0,18 ms médian | Même réserve : boucle locale, aucun réseau traversé. |
| Mesure photographique (médiane sur 5) | **Non mesurée** | Exige deux écrans physiques, un chronomètre plein écran, un appareil photo et deux lectures humaines. Hors de portée d'un agent — c'est la mesure **M6**. |

**Estimation basse, et rien de plus.** La somme des deux composantes mesurées donne
≈ 5,06 ms. Ce n'est **pas** une latence de bout en bout : il y manque la capture
elle-même (temps entre le changement d'écran et la texture disponible, jamais isolé) et
le décodage plus l'affichage côté spectateur (le spectateur du spike n'écrit qu'un
fichier, il ne décode ni n'affiche rien). Elle établit un plancher. Si M6 rend une
médiane nettement supérieure à ~5 ms de façon répétée, cela pointera vers un tampon
caché ailleurs dans la chaîne — c'est exactement l'écart à surveiller.

Deux réserves de mesure à conserver :

- **Un épisode isolé d'environ une seconde** est visible à la 48ᵉ seconde du run de
  60 s : une seconde quasi vide, puis une rafale de rattrapage à 124 images reçues.
  C'est lui, et lui seul, qui porte les p99 de RTT (1 081,56 ms) et de transit
  (880,20 ms). Deux hypothèses candidates sont documentées — un budget de relance de
  300 ms réarmé par morceau et non par image, et une contention réelle confirmée sur la
  machine de test — **aucune des deux n'a été isolée**. Ces p99 ne caractérisent pas la
  chaîne ; ils datent d'un incident non expliqué.
- **La chaîne s'est arrêtée deux fois sur deux essais sur écran réel**, après ~19 s,
  par un mécanisme de sécurité délibéré (« tampon d'émission saturé plus de 300 ms —
  arrêt pour ne pas produire un flux corrompu »), confirmé par la ligne verbatim dans
  les deux journaux. Le run de 60 s sur source synthétique n'a connu qu'un incident
  isolé. La robustesse face à un débit d'entrée irrégulier est donc **moins établie**
  que face à une charge régulière. Deux essais ne suffisent pas à trancher.

### Deux chiffres que ce rapport ne reprend pas

- **La médiane d'encodage H.264 4:4:4 de 5,46 ms** citée dans le tableau de remesure de
  la Tâche 3. Elle est identique à sa valeur d'avant correction alors que son p99
  associé chutait fortement (10,39 → 6,24 ms) et que les deux autres codecs voyaient
  leur médiane monter. Possible recopie. **Non vérifiable** : le motif de test et la
  configuration de l'encodeur ont changé depuis (motif refondu en Tâche 4, double
  passage désactivé), une réexécution ne reproduirait pas cette mesure mais en
  produirait une autre. Les temps d'encodage par codec à citer sont ceux de la Tâche 4,
  mesurés sur le motif courant : H.264 4:2:0 4,78 / 6,21 ms · H.264 4:4:4 4,94 /
  12,84 ms · HEVC 4:4:4 5,35 / 6,97 ms · AV1 4:2:0 3,95 / 4,82 ms.
- **Les latences de la Tâche 3** (6,29 ms médian, 7,47 ms p99 en HEVC 4:4:4). Elles ont
  été mesurées sur le motif de test d'avant sa refonte en Tâche 4 et ne sont plus
  reproductibles avec le code actuel. Elles restent exactes pour ce qu'elles étaient ;
  elles ne caractérisent plus le code d'aujourd'hui. Les chiffres courants sont ceux du
  tableau ci-dessus.

---

## Ce qui a changé par rapport au spec

Six écarts, classés par gravité. Pour chacun : ce que le document d'architecture
affirme, ce que la mesure ou la recherche établit, la conséquence concrète, et la
correction apportée au spec.

Deux d'entre eux ouvrent une **décision produit** que ce rapport n'a pas tranchée : elle
appartient au propriétaire, et la trancher ici engagerait le jalon 2 sur une orientation
qu'il n'a pas choisie. Elles sont reprises en fin de rapport.

### Écart 1 — AV1 ne fait pas de 4:4:4

**Le spec (§6.4)** présente AV1 comme le meilleur choix, « ~40 % de débit en moins à
qualité égale », en tête du tableau des codecs.

**La mesure.** NVENC ne produit pas de 4:4:4 en AV1, même sur architecture Ada. La
sonde matérielle énumère, pour chaque GUID de codec, les formats d'entrée acceptés :
sur cette RTX 4060 elle rend `H.264 4:2:0`, `H.264 4:4:4`, `HEVC 4:4:4`, `AV1 4:2:0`.
Confirmé en sortie par `ffprobe` sur un flux réellement produit : `av1` / `Main` /
`yuv420p`. Le 4:4:4 — qui rend le texte lisible — n'existe qu'en H.264 et en HEVC.

**Conséquence pour le produit.** AV1 et « texte net » s'excluent sur ce matériel. Le
gain de débit d'AV1 ne peut être encaissé qu'en renonçant à la différenciation
principale du produit. AV1 reste pertinent pour le partage de **vidéo**, pas pour le
partage d'un écran de travail.

**Statut.** Déjà intégré au spike, qui retient HEVC 4:4:4 pour le partage d'écran et
n'expose AV1 que pour la vidéo. Le spec est corrigé (§6.4).

### Écart 2 — H.264 4:4:4 ne respecte pas la cible de débit

**Le spec (§6.4)** désigne H.264 4:4:4 comme « socle universel », disponible sur le
« matériel des 12 dernières années ».

**La mesure.** 70,93 Mbps réels pour une cible de 10 Mbps. Testé à 3, 5, 10, 20 et
30 Mbps de cible : le débit réel reste bloqué entre 67 et 72 Mbps sur toute la plage 3
à 20 Mbps — donc indépendant de la cible, ce n'est pas une erreur d'échelle — et ne
redescend qu'à une cible de 30 Mbps, où il tombe à 14,8 Mbps. Son p99 d'encodage est le
double des autres (12,84 ms contre 4,82 à 6,97 ms pour les trois autres combinaisons),
cohérent avec ce débit. Les trois autres combinaisons, avec
exactement la même configuration par ailleurs, tiennent leur cible à 10-20 % près ; les
paramètres ont été journalisés juste avant l'initialisation de l'encodeur et sont
identiques entre H.264 4:2:0 (conforme) et H.264 4:4:4 (non conforme).

**Portée honnête de ce constat.** Il est établi sur **un** GPU et **un** pilote (RTX
4060, 610.74), sur **un** contenu. La conclusion prudente est : anomalie du profil High
4:4:4 Predictive sur cette combinaison matériel/pilote, non généralisée. La cause n'a
pas été creusée — hors profondeur raisonnable pour un spike.

**Conséquence pour le produit.** Un codec dont le débit ne se pilote pas est
inutilisable pour du partage d'écran : ni plancher garanti, ni plafond respecté, ni
couches de qualité multiples. Sur ce matériel, le repli pour machines anciennes devient
H.264 4:2:0, au prix de la netteté du texte.

**Décision ouverte.** Le spec promet-il encore un socle universel en 4:4:4 ? Voir la
décision D1, en fin de rapport — elle fusionne avec l'écart 6, qui la rend plus lourde.

### Écart 3 — Les en-têtes de séquence ne sont émis qu'une fois

**Le spec** ne l'affirme pas directement, mais le suppose partout : le mode public
(§5.3, §5.4) et le multi-spectateurs (§6.5) reposent sur le fait qu'un spectateur puisse
rejoindre un partage **déjà en cours**.

**La mesure.** Comptage des unités NAL sur un flux HEVC de 1 201 images : **1 VPS,
1 SPS, 1 PPS, 1 image IDR**, et 1 993 unités de tranche `TRAIL_R`. Un seul jeu d'en-têtes
sur toute la séquence, malgré le drapeau de répétition activé. Coupés en leur milieu, les
trois flux sont indécodables — sortie verbatim :

```
v2_synth.obu  coupé à 50% -> No sequence header available     -> av1,unknown,0,0
v2_synth.h265 coupé à 50% -> PPS id out of range: 0           -> hevc,unknown,0,0
v2_synth.h264 coupé à 50% -> non-existing PPS 0 referenced    -> h264,unknown,0,0
```

**Cause établie.** La sémantique de ces drapeaux est « émettre les en-têtes à chaque
image IDR ». Avec un groupe d'images infini — réglage délibéré du plan, qui supprime les
pics de débit périodiques — il n'y a qu'une seule image IDR, la première. Les drapeaux
sont structurellement sans effet dès qu'on choisit ce réglage. Ils sont conservés dans le
spike : corrects si la période IDR change un jour, et nécessaires à la cohérence entre
codecs pour le comparatif.

**Conséquence directe sur le produit.** Un spectateur qui rejoint un partage en cours ne
verrait **rien**. Écran noir. Ce n'est pas un cas limite : c'est le scénario nominal du
mode public et du multi-spectateurs. Le spike ne l'exerce jamais — son spectateur se
connecte toujours à un hôte qui vient de démarrer — donc rien ne l'aurait révélé sans
cette vérification.

**Correction identifiée**, à inscrire au jalon 2 : soit demander explicitement
l'émission des en-têtes sur une image choisie à l'arrivée de chaque spectateur, soit les
récupérer une fois et les transmettre hors du flux vidéo. La seconde option est la plus
économe en débit. Aucune des deux n'est faite ici.

### Écart 4 — L'offre de connexion voyage en clair

**Le spec (§5.2)** promet, pour l'adresse réseau : « Non — enveloppe scellée ».
**Le spec (§2, décision 5)** promet qu'« un inconnu qui clique sur un lien public
n'obtient aucune adresse tant que l'hôte n'a pas approuvé ».

**La mesure.** Le bloc d'offre contient **deux adresses de l'émetteur en clair** — celle
de son réseau local et sa publique — décodables par simple base64. Mesuré précisément :
cinq motifs bruts capturés dans le bloc, dont trois sont des zéros — deux constantes
écrites en dur par la bibliothèque, et un champ qu'elle force elle-même à zéro par souci
de confidentialité ; **deux** sont de vraies adresses. Seule la
**réponse** du spectateur est scellée. La raison est structurelle et connue : au moment
de produire l'offre, le destinataire n'est pas encore identifié, donc aucune clé
n'existe pour la sceller.

**Ce qui aggrave, et qui est le point le plus grave.** L'offre a une **forme de
diffusion**. Elle est destinée à être collée dans une messagerie tierce : tout lecteur
du canal — y compris ceux qui ne se connecteront jamais — apprend l'adresse publique de
l'émetteur. C'est strictement plus large que « mon pair connaît mon adresse », qui est
une conséquence inévitable du protocole IP et que le spec assume explicitement.

**Et la projection au jalon 1 aggrave au lieu de résoudre.** Si l'offre transite non
scellée par la boîte aux lettres, **l'opérateur du serveur voit l'adresse publique de
chaque émetteur** — ce qui contredit frontalement la prémisse « aucun serveur ne voit
quoi que ce soit » sur laquelle repose toute l'architecture (§1, principe 2 ; §3 ; §5.2).

**Conséquence de conception, à inscrire maintenant.** La boîte aux lettres ne peut pas
être un simple relais : elle doit être un **annuaire de clés interrogeable avant**
production de l'offre. On récupère la clé du destinataire, puis on scelle, puis on
envoie. La direction est identifiée ; **la conception appartient au jalon suivant** et
n'est pas tranchée ici. Voir la décision D2.

**Mesures d'atténuation déjà en place dans le spike**, pour le test M4 : la console de
l'opérateur affiche, juste avant le bloc, un avertissement disant que ce bloc contient
son adresse publique en clair et qu'il doit être envoyé en message privé, jamais dans un
salon ouvert.

*Note technique, pour que personne ne la reprenne comme un oubli :* une voie standard
existait pour retirer l'adresse publique du texte clair — s'appuyer uniquement sur les
candidats découverts par sondage mutuel. La refuser était le bon choix **ici**, car elle
réduirait le nombre de chemins testés et rendrait la réponse à Q5 pessimiste. Ce choix
est propre au spike, pas au produit.

### Écart 5 — Le régulateur pilote la cadence, pas le débit

**Le spec (§6.1)** décrit un « plancher garanti défini par l'utilisateur, jusqu'à
100 Mbps » et un « contrôle réécrit : descente lente, remontée rapide, jamais sous le
plancher ». Dans le contexte d'un contrôle de congestion vidéo, cette formulation se lit
comme un ajustement fin et continu du **débit par image**.

**La réalité du code.** Le régulateur ne peut que **sauter des images entières**.
L'encodeur n'expose que sa création (débit fixé à l'ouverture de la session) et
l'encodage d'une image — aucune reconfiguration à chaud. Le seul levier restant à
l'application est donc de ne pas produire l'image suivante quand le budget est épuisé.

**Pourquoi c'est un écart de fond et non un détail.** La promesse du produit est : « en
cas de congestion, on préfère perdre des images plutôt que de la netteté ». Mais si le
seul levier est la cadence, on ne peut **que** perdre des images — l'autre terme du choix
n'existe pas. La promesse n'est pas violée, elle est vide : ce n'est plus un arbitrage.

**Nuance à conserver.** Le comportement est fidèle au régulateur tel que documenté depuis
sa conception. L'écart est entre le **code** et le **spec**, pas entre deux tâches. Les
propriétés du régulateur lui-même (plancher inviolable, descente bornée, remontée rapide)
sont démontrées, elles ; c'est la grandeur qu'il pilote qui n'est pas celle annoncée.

**Correction identifiée** : exposer la reconfiguration de débit à chaud
(`nvEncReconfigureEncoder`), que le matériel sait faire. Tant qu'elle n'est pas exposée,
le jalon 2 ne peut pas tenir la promesse du §6.1. Le spec est corrigé pour le dire.

### Écart 6 — Le 4:4:4 n'existe pas sur AMD, et probablement pas sur Intel

**Statut de cette constatation, à énoncer avant tout le reste : recherche documentaire,
non testée sur matériel.** Aucune carte AMD ni Intel n'était disponible. Cet écart a été
soulevé par le propriétaire du projet en fin de jalon, puis vérifié en documentation.
C'est le seul des six qui ne repose pas sur une mesure de ce jalon.

**Le spec (§6.4)** présente H.264 4:4:4 comme « socle universel, matériel des 12
dernières années », et **le spec (§6.1)** fait du 4:4:4 le pilier de la netteté du texte.

**Ce que la recherche établit.**

- **AMD** — le SDK d'encodage AMF ne comporte **aucune surface 4:4:4**. Sur RDNA 3
  (7900 XTX), soumettre un format 4:4:4 renvoie `AMF_INVALID_FORMAT`. Ce n'est pas une
  limite de génération : la capacité est absente de la plateforme. Le 4:4:4 introduit en
  AMF 1.5.0 concerne le convertisseur de couleur, **pas l'encodeur**.
- **Intel** — la documentation atteste le 4:2:2 sur certaines configurations ; aucune
  source ne confirme le 4:4:4 en encodage. À considérer comme indisponible jusqu'à
  vérification sur matériel réel.
- **NVIDIA** — seule plateforme confirmée, et confirmée par mesure : H.264 4:4:4 et
  HEVC 4:4:4 énumérés par la sonde matérielle et produits réellement.

**Portée pour le produit : c'est l'écart le plus lourd des six.** Le gain de +40,1 dB sur
la chrominance est la différenciation la plus visible du produit face à Discord.
**Un utilisateur AMD ne l'aurait pas.** Il conserverait le débit libre, la résolution et
la cadence — trois des quatre limites levées — mais retomberait en 4:2:0 pour la couleur,
donc au niveau de Discord sur le point précis qui motive le projet. Et contrairement aux
cinq autres écarts, aucune quantité de travail ne l'ajoutera : c'est une contrainte
matérielle, pas un défaut d'implémentation.

**Trois voies, aucune indolore. Aucune n'est tranchée ici** — voir la décision D1.

1. Encodage **logiciel** en 4:4:4 pour AMD et Intel. Texte net préservé, mais la charge
   processeur passe de 0,5 % à plusieurs dizaines de pourcents et le 1440p60 devient
   difficile — donc on troque la différenciation « texte net » contre la différenciation
   « ne coûte rien à la machine ».
2. Accepter le 4:2:0 sur AMD et Intel, en le **disant dans l'interface** plutôt qu'en
   laissant l'utilisateur croire à un défaut du logiciel.
3. Hybride : 4:4:4 matériel sur NVIDIA, 4:4:4 logiciel sous un seuil de résolution
   ailleurs, 4:2:0 au-delà.

**Travail déjà engagé sur cette question, hors périmètre de ce jalon.** Une note de
référence du 23 août 2026,
`docs/superpowers/notes/2026-08-23-compatibilite-toutes-cartes-graphiques.md`, instruit
déjà le sujet — elle n'était pas commitée à la date de rédaction de ce rapport, et je ne
l'ai ni écrite ni vérifiée. Deux de ses apports méritent d'être connus avant d'arbitrer
D1, en les attribuant à leur source plutôt qu'à une mesure de ce jalon :

- **Le décodage 4:4:4 serait aussi rare que l'encodage.** Si c'est exact, la voie 1
  ci-dessus ne résout que la moitié du problème : produire du 4:4:4 depuis une carte
  NVIDIA ne sert à rien si le spectateur ne peut pas le décoder — ce qui exclurait le
  mobile et les navigateurs. Cela déplace le problème de l'émetteur vers la paire.
- **Deux voies supplémentaires existent**, que ce rapport n'avait pas envisagées :
  l'empaquetage de la couleur pleine résolution dans une seule image plus grande encodée
  en 4:2:0 ordinaire, et le 4:2:2 comme palier intermédiaire. La première conserve une
  seule session d'encodage et n'impose aucune exigence de décodage particulière.

Cette note recommande un spike dédié plutôt qu'une décision sur plan, et liste six
inconnues à mesurer — dont, en premier, la **confirmation sur matériel réel** des
constats AMD et Intel, qui restent documentaires. Rien de tout cela ne change le verdict
du jalon 0 ; cela change ce que D1 doit trancher, et quand.

**État du code, qui conditionne le coût de n'importe laquelle de ces voies.** Aucune
abstraction d'encodeur n'existe : `sky-encode` ne contient que l'implémentation NVENC en
dur, sans trait `VideoEncoder`. Brancher un second fabricant demandera d'abord d'extraire
cette abstraction. Le spec (§2, décision 8) promet déjà une « abstraction OS dès le
premier jour » pour la capture ; il n'a pas d'équivalent pour l'encodage.

**Portabilité d'architecture, pour mémoire.** Cible unique `x86_64-pc-windows-msvc`.
ARM64 Windows n'a jamais été abordé et constitue une inconnue complète, y compris sur la
disponibilité d'un encodeur exploitable. Apple Silicon est prévu au jalon 7, non vérifié.
Cohérent avec la décision « Windows d'abord », mais à inscrire comme non traité plutôt
que comme acquis.

### Écart annexe, corrigé dans la même passe — §5.5

Le spec promettait un diagnostic d'échec « indiquant lequel des deux réseaux pose
problème ». Ce n'est **pas observable depuis un bord** : chacun ne constate que l'absence
de paquets, jamais la raison de cette absence. Le §5.5 est corrigé pour décrire les trois
situations que le diagnostic distingue réellement — perçage échoué, perçage réussi mais
canal chiffré en échec, aucun paquet parvenu (deux causes laissées ouvertes) — plus le
compteur d'erreurs sur le port local qui nuance le verdict. Ce n'est pas un
appauvrissement de la promesse : c'est la différence entre un diagnostic exact et un
diagnostic qui affirmerait une cause qu'il ne peut pas connaître, dans le cas précis que
Q5 existe pour tester.

### Correction déjà apportée au spec pendant le jalon

**§8.1, signature de code macOS.** La signature ad-hoc est *obligatoire* sur Apple
Silicon — un binaire arm64 dépourvu de toute signature y est tué au démarrage — mais elle
est **gratuite**. Les 99 $/an ne suppriment que l'avertissement du premier lancement.
macOS reste à 0 €. Déjà intégré au document.

---

## Décision

- [ ] GO — les paris tiennent, on enchaîne sur le jalon 1
- [x] **GO CONDITIONNEL** — tient sauf sur **Q5, la connexion entre deux box**, et
      **Q1, le débit de capture sur écran réel** : deux mesures qu'aucun agent ne pouvait
      prendre et qui restent à la charge du propriétaire.
- [ ] NO-GO

### L'argument

**Quatre des six questions sont closes positivement, et aucune n'a produit de réponse
négative.** Le cœur technique du projet — capture GPU, encodage matériel en 4:4:4 sans
copie processeur, gain de qualité mesurable sur la chrominance, charge processeur à
0,53 % contre un seuil de 5 % — tient sur du matériel réel, avec des preuves
reproductibles. Le repli FFmpeg prévu au plan n'a jamais été nécessaire. Q6 est close
sous une réserve nommée : ses propriétés sont démontrées, c'est la **grandeur** qu'elle
pilote qui n'est pas celle annoncée (écart 5), et elle n'a jamais vu de congestion
réseau réelle.

**Aucun des six écarts n'invalide le projet.** Trois sont des corrections de choix
techniques dans un espace où d'autres choix existent (écarts 1, 2, 3). Un est une dette
de conception identifiée avec sa direction de résolution (écart 4). Un est une API à
étendre sur du matériel qui sait déjà le faire (écart 5). Le sixième, le plus lourd,
restreint la différenciation du produit à une famille de matériel sans remettre en cause
sa faisabilité (écart 6).

**Mais deux questions restent ouvertes, et l'une porte le risque n°1.** Q5 n'a pas été
approchée : aucun paquet n'a franchi un NAT. Le spike a rendu le test réel capable de
dire la vérité — c'est réel et c'était nécessaire — mais il ne l'a pas remplacé. Si ce
test échoue de façon répétée, la cause probable est un NAT symétrique ou un CGNAT chez
l'un des pairs, situation où aucune quantité de STUN ne suffit et où seul un relais
débloquerait — ce que le spec écarte par principe (§10). Ce ne serait pas un NO-GO du
projet, mais un NO-GO de la promesse « zéro serveur » telle qu'elle est écrite, et donc
une décision d'architecture à rouvrir. Le spec chiffre déjà ce risque à ~5-10 % des
paires (§2, décision 2 ; §11) — **ce chiffre n'a été ni vérifié ni infirmé par ce
jalon.**

C'est cette asymétrie qui interdit un GO ferme : tout ce qui a été mesuré est bon, et ce
qui n'a pas été mesuré est précisément ce qui porte le risque.

### De quoi dépend le passage à un GO ferme

Deux mesures, et rien d'autre. Aucune ne dépend d'un travail de développement
supplémentaire : le code qui les produit est écrit, compilé et testé.

1. **M4 — test pair-à-pair avec un correspondant distant** (`spike/README-AMI.md`,
   binaire autonome déjà produit). Critère : connexion établie en moins de 8 secondes
   **après le collage du second bloc**. Relever le texte exact affiché des deux côtés,
   sans le résumer — le diagnostic distingue désormais trois situations, et c'est cette
   distinction qui porte la réponse à Q5.
2. **M1 — débit de capture sur écran en mouvement réel.** Critère : ≥ 59 fps, avec
   déplacement de fenêtres et défilement de page pendant les 30 secondes.

- **M4 réussit et M1 atteint le seuil** → **GO ferme**, le jalon 1 s'ouvre sans réserve
  technique.
- **M4 échoue avec « cause probable : NAT strict »**, sur plusieurs correspondants et
  plusieurs fournisseurs d'accès → la décision remonte au niveau architectural (relais,
  ou périmètre restreint aux paires compatibles), et ce rapport doit être rouvert.
- **M4 échoue avec « le canal de données ne s'est pas ouvert… NAT n'est PAS en cause »**
  → Q5 est **positive**, et le problème est ailleurs : un défaut à corriger, pas un pari
  perdu.
- **M1 rend un débit d'images nettement sous 59 fps sur un écran en mouvement réel** →
  ce serait le seul résultat de ce jalon qui contredirait une mesure déjà prise, puisque
  la chaîne complète tient 60,0 i/s sur 60 s à partir d'une source synthétique. La cause
  serait alors à chercher du côté de la capture, pas de l'encodage — donc un défaut
  localisé, pas un pari invalidé.

Les quatre autres mesures humaines (**M2**, **M3**, **M5**, **M6**) affinent le tableau
sans bloquer la décision.

---

## Ce qui reste à la charge du propriétaire

### Les mesures

Détail complet et commandes dans
`.superpowers/sdd/2026-08-22-jalon-0-faisabilite/mesures-humaines.md`.

| # | Mesure | Question | Bloquante ? |
|---|--------|----------|-------------|
| **M4** | Test pair-à-pair avec un correspondant distant | **Q5** | **Oui** — risque n°1 |
| **M1** | Débit d'images en capture sur écran en mouvement réel | **Q1** | **Oui** |
| M2 | Lecture visuelle du flux encodé | Q2 | Non |
| M3 | Jugement de lisibilité comparée entre les 4 encodages | Q3 | Non |
| M5 | Charge processeur relevée sur écran réel | Q4 | Non — deux mesures instrumentées concordent déjà |
| M6 | Latence de bout en bout par photographie | latence | Non |

**Un avertissement à ne pas perdre, repris de cette checklist.** N'activer **aucun**
journal réseau détaillé pendant M4. Le filtrage de données personnelles de la
bibliothèque réseau ne couvre pas son point de trace le plus volumineux : les adresses
des deux machines sortiraient en clair dans la console, et donc dans toute capture
d'écran partagée ensuite. Le diagnostic intégré suffit à identifier le côté fautif sans
exposer d'adresse.

**Deux pièges de jugement**, pour M2 et M3. Ne jamais juger la qualité sur la première
seconde d'un flux : le tampon de sortie est réglé à une seule image, donc la toute
première image — la seule entièrement autonome, en 1440p, tout intra, en 4:4:4 — est
plafonnée à environ 500 kbit et sort franchement dégradée, et le groupe d'images infini
la maintient en référence pendant environ deux secondes. Avancer de 3 à 5 secondes avant
de porter un jugement.

### Les décisions que ce rapport n'a pas tranchées

Elles sont inscrites dans le spec comme décisions à prendre, sans orientation imposée.

**D1 — Que promet-on aux utilisateurs non-NVIDIA ?** (écarts 2 et 6)
L'arbitrage est entre « sans compromis partout » et « sans compromis sur NVIDIA ». Les
voies et leur coût respectif sont décrites à l'écart 6. Le choix détermine si le jalon 2
doit extraire un trait `VideoEncoder` et intégrer un second chemin d'encodage, ou s'il
peut rester sur l'implémentation NVENC en dur en signalant la limite dans l'interface.
Il détermine aussi si le §6.4 continue de promettre un socle universel en 4:4:4 — ce que
la mesure et la recherche contredisent toutes deux.

*Cette décision est déjà partiellement engagée hors de ce jalon* : la note de référence
`docs/superpowers/notes/2026-08-23-compatibilite-toutes-cartes-graphiques.md` (non
commitée à ce jour, non vérifiée par moi) consigne quatre décisions de périmètre — 4:4:4
natif conservé sur NVIDIA, objectif de faire mieux que 4:2:0 ailleurs plutôt que de s'y
résigner, ordinateur avant mobile, application native sur ordinateur — et recommande un
spike dédié après le jalon 3. Ce que ce rapport maintient malgré cela : **rien ne peut
être engagé avant une confirmation sur matériel AMD et Intel réel**, puisque tout le
constat de l'écart 6 est documentaire.

**D2 — Comment restructurer le signaling pour que l'offre soit scellée ?** (écart 4)
La direction est identifiée — un annuaire de clés interrogeable **avant** production de
l'offre, et non un simple relais — mais la conception appartient au jalon 1, qui construit
précisément cette boîte aux lettres. Ce qui est acquis et ne doit pas être reperdu : tant
que l'offre n'est pas scellée, l'opérateur du serveur voit l'adresse publique de chaque
émetteur, ce qui contredit la prémisse fondatrice de l'architecture.

**Point de vigilance associé, non chiffré par ce jalon.** Le spec annonce ~5-10 % des
paires incapables de se joindre (§2, décision 2). Ce chiffre vient de la littérature, pas
d'une mesure de ce projet. M4 en donnera un premier point de donnée, un seul.

---

## Code à promouvoir

Le code du spike est jetable **par défaut**. Les éléments ci-dessous sont l'exception :
les reprendre coûte moins cher que les redécouvrir, et plusieurs ont coûté un cycle de
mise au point complet.

### À reprendre tel quel, ou presque

| Élément | Pourquoi |
|---|---|
| `sky-encode/src/nvenc_sys.rs` — `NvencApi` | Charge `nvEncodeAPI64.dll` dynamiquement et résout la table de fonctions NVENC. Supprime la dépendance au NVIDIA Video Codec SDK, qui avait été rapporté à tort comme un blocage. Durci contre le détournement de recherche de DLL. Le constructeur ne prend aucun device : le point de variation reste chez l'appelant. |
| `sky-crypto` (`Identity`, `seal`, `open`) | Une centaine de lignes avec ses tests, dont 5 couvrant chacun une garantie distincte (confidentialité vis-à-vis d'un tiers, intégrité, non-liabilité de deux scellages, borne de taille). Surcoût constant de 48 octets, vérifié. Directement réutilisable ; seul son **usage** change (voir D2). |
| `sky-net/src/stun.rs` | N'était pas au plan et sans lui aucune traversée de NAT n'est possible. Trente lignes de décodage manuel, parce que le parseur de la bibliothèque refuse les réponses des serveurs publics. Identifiant de transaction tiré du générateur du système, réponse rejetée si elle ne vient pas du serveur interrogé — les deux garde-fous sont testés. |
| `spike/.cargo/config.toml` (liaison statique de la bibliothèque d'exécution C) | Trois lignes qui font la différence entre un binaire qui démarre chez un tiers et un binaire qui affiche une erreur incompréhensible. Table d'importation vérifiée avant/après : 23 DLL dont une appartenant au redistribuable Visual C++ et non à Windows, puis 13 toutes livrées avec le système. **Réserve : le binaire n'a jamais été *exécuté* sur une machine sans Rust** — seule sa table d'importation a été vérifiée. |
| `spike/docs/api-nvenc.md` | Relevé des noms réellement générés par les bindings, avec les pièges de nommage. Fait gagner une demi-journée à qui reprendra le FFI. |

### À reprendre comme référence, en refondant

| Élément | Ce qui vaut d'être repris | Ce qui doit changer |
|---|---|---|
| `sky-encode/src/nvenc.rs` — `NvencEncoder` | La séquence d'appels complète, la configuration issue du préréglage plutôt que d'un remplissage à zéro, la signalisation couleur vérifiée par aller-retour, la désactivation du double passage, la libération sur tous les chemins d'erreur (vérifiée : aucune session orpheline). | Doit passer derrière un trait `VideoEncoder` (D1) et exposer la reconfiguration de débit à chaud (écart 5). Les deux stubs de symboles fournis pour satisfaire le linker sont fragiles et à réévaluer. |
| `sky-capture/src/wgc.rs` — `WgcCapture` | Le chemin sans copie de bout en bout, les trois écarts d'API corrigés, la fermeture ordonnée de la session et du pool. | Un point identifié et non corrigé : l'objet image est relâché avant que la texture ne soit rendue, donc elle peut retourner au pool et être réécrite. Sûr dans le flux synchrone actuel, faux dès qu'un pipeline asynchrone apparaît. |
| `sky-net/src/pacer.rs` — `Pacer` | La formule et ses trois propriétés démontrées analytiquement, pas seulement testées. | Le levier qu'il pilote (écart 5), et un vrai signal de congestion à la place du taux d'échec d'envoi local. |
| `sky-net/src/link.rs` — `PeerLink` | **Le savoir vaut plus que le code.** Quatre pièges de la bibliothèque réseau, tous invisibles en boucle locale : la destination doit être l'adresse du candidat hôte ; l'offre en attente doit être conservée et non recréée ; un fournisseur cryptographique doit être installé avant toute session ; l'horloge présentée à la bibliothèque ne doit démarrer qu'au premier paquet reçu. | La structure elle-même est un banc de test, à réécrire pour le produit. |

### À ne pas promouvoir

Les sous-commandes `cmd_*.rs` de `sky-probe` sont des bancs de mesure, pas du code de
produit : elles portent des choix propres au spike (format de morceau à 9 octets, arrêt
du run sur congestion soutenue, affichage des blocs à l'écran). Le motif de test
synthétique et le script de mesure PSNR/SSIM méritent en revanche d'être conservés comme
outillage de non-régression pour le jalon 3.

---

*Ce rapport porte la décision d'ouvrir ou non le jalon 1. Il ne clôt pas le jalon 0 :
deux mesures manquent, elles sont nommées, et le code qui les produit est prêt.*
