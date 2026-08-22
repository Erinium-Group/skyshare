# Comparatif des 4 codecs à débit égal (Q3)

*Corrigé après revue — voir « Note de correction » en fin de document pour
le détail des 3 correctifs et les valeurs d'avant/après.*

Vérifie par la mesure la thèse du projet : le 4:2:0 stocke la couleur au
quart de la résolution et dégrade le texte fin, le 4:4:4 la garde pleine
résolution et ne devrait pas avoir ce problème. Résultat : **oui, l'écart
est mesurable, et il est net** — voir le tableau plus bas. Ce que la mesure
ne dit pas : si le texte reste *lisible* à l'écran (voir « Ce que ces
chiffres n'établissent pas »).

## Écart avec le brief d'origine

Le brief prévoyait de préparer la même scène (éditeur de texte + vidéo)
quatre fois à la main sur l'écran réel, en appuyant sur Entrée entre chaque
encodage. Deux problèmes : ça ne peut pas tourner sans supervision, et une
scène reconstituée à la main quatre fois de suite n'est jamais rigoureusement
identique — ce que le brief lui-même exige pour que la comparaison ait un
sens. Remplacé par la texture synthétique de la Tâche 3 (Q2) : son contenu ne
dépend que du nombre d'images déjà produites, donc en recréant la texture
(compteur à 0) pour chaque codec, les quatre flux voient exactement la même
séquence d'images, pixel pour pixel. `cmd_codecs.rs` s'exécute d'un bout à
l'autre sans aucune intervention clavier.

## Motif de test : pourquoi il a changé en cours de route

Le motif hérité de la Tâche 3 (glyphes gris clair sur fond clair, un peu de
bruit) a une propriété qui invalide tout le comparatif : `(b,g,r)` y variaient
quasiment ensemble, donc U et V étaient déjà proches de zéro **avant tout
encodage**. Le sous-échantillonnage chroma n'avait donc rien à perdre — le
comparatif aurait « confirmé » la thèse par construction, sans l'avoir
réellement testée.

Motif corrigé (`motif_detaille` dans `cmd_encode.rs`) : un panneau de
1200×800 px simulant un éditeur de code, fond sombre (BGR 24,20,18), lignes
de texte de ~12 px de haut (11 px d'encre + 6 px d'interligne), glyphes
rendus **1 pixel de large, alternant rouge pur (255,0,0) et bleu pur
(0,0,255) par colonne**. C'est le pire cas explicite que la consigne de
tâche demandait : le 4:2:0 divise la résolution de U/V par 2 dans les deux
axes, donc une alternance à la colonne près est exactement ce qu'il ne peut
pas représenter sans moyenner les deux couleurs saturées.

Le reste de l'image (bureau autour du panneau) reste en fond uni — voir plus
bas pourquoi (contrôle de débit).

## Méthode de comparaison retenue, référence comprise

**Référence.** Deux options possibles : relire les textures GPU (aller-retour
GPU→CPU), ou recalculer la séquence côté CPU. La seconde ne coûte rien de
plus (le motif est déjà une fonction pure de `(x, y)` et du numéro d'image) et
garantit une correspondance bit à bit avec ce que chaque encodeur a reçu,
puisque c'est littéralement le même code (`motif_detaille` +
`ecrire_reference_brute`, tous deux dans `cmd_encode.rs`) qui produit l'atlas
et sa fenêtre par image. Choisie pour cette raison. `cmd_codecs` écrit donc
`cmp-reference.bgra` : rawvideo BGRA, 2560×1440, 1020 images (15 s × 60 i/s +
120 images de marge), **avant** de lancer les quatre encodages.

Vérifié à l'octet près : extraction du pixel (210, 94) de l'image 120 de la
référence → `00 00 ff ff` (rouge pur), pixel (211, 94) → `ff 00 00 ff` (bleu
pur), pixel (204, 94, fond) → `18 14 12 ff` (24, 20, 18) — conforme à la
formule du motif.

**PSNR / SSIM par plan.** `ffmpeg`, filtres `psnr` et `ssim`, comparaison en
`yuv444p` (résolution chroma commune — un flux 4:2:0 y arrive
sur-échantillonné par le décodeur, ce qui est *le même* traitement qu'un
afficheur applique avant de poser les pixels à l'écran, donc une comparaison
honnête plutôt qu'un artefact de méthode). Conversion RGB→YUV444p de la
référence : `scale=out_range=full` (matrice par défaut = BT.601, coefficients
identiques à BT.470BG que NVENC est censée appliquer côté encodeur — c'est
la matrice déclarée dans le VUI, `appliquer_vui()` dans `nvenc.rs`.) Plage
vérifiée : `videoFullRangeFlag=1`, cohérent avec le test empirique
d'aller-retour de la Tâche 3. Vérifié sur un pixel rouge pur : Y=76, U=84,
V=255, conforme à la formule BT.601 pleine plage (Y=76,2, U=84,9, V=255
après écrêtage) — **ce que ce test vérifie, c'est que `scale=out_range=full`
fait bien la conversion BT.601 pleine plage qu'on lui demande, pas que
NVENC utilise en interne exactement ces mêmes coefficients.**

**Ce qui est vérifié et ce qui ne l'est pas, sur la matrice couleur.**
`nvenc.rs` porte, depuis la Tâche 3, la réserve suivante : « la plage est
vérifiée expérimentalement par aller-retour ; la matrice ne l'est pas ». Elle
tient toujours ici — je n'ai pas revérifié empiriquement que le convertisseur
RGB→YUV *interne* de NVENC utilise exactement BT.470BG plutôt qu'une autre
matrice proche (BT.601/SMPTE170M partagent les mêmes coefficients, mais nos
sources ne l'excluent pas absolument). Deux niveaux à distinguer :
- **Valeurs absolues de PSNR/SSIM** (celles du tableau plus bas) : portent
  cette réserve. Si le convertisseur interne de NVENC utilisait une matrice
  différente de celle assumée pour reconstruire la référence, les valeurs
  absolues seraient décalées.
- **Écart relatif entre 4:2:0 et 4:4:4** (le résultat qui répond à Q3) :
  n'en dépend pas. Le convertisseur RGB→YUV est le même étage, en amont du
  choix de codec et du sous-échantillonnage — commun aux quatre flux. Un
  biais de matrice, s'il existe, serait donc partagé par les quatre
  mesures également, et se retrancherait dans la comparaison relative plutôt
  que de la fausser. Une vérification empirique de la matrice interne
  coûterait disproportionnellement cher pour ce que Q3 demande — non
  entreprise pour cette raison, pas par négligence.

**Piège rencontré et corrigé : l'alignement image par image n'est pas
garanti par défaut.** `ffprobe` sur les quatre flux élémentaires (sans
conteneur) montre des cadences *devinées* incohérentes : H.264 → 120 i/s,
HEVC → 60 i/s, AV1 → 25 i/s — aucun des trois ne porte de métadonnée de
cadence fiable (le VUI H.264/HEVC ne signale pas `timingInfoPresentFlag`,
l'IVF AV1 non plus de façon exploitée par le démuxeur). Sans correction, le
filtre `psnr`/`ssim` de ffmpeg aligne les paires d'images par horodatage
(`framesync`), et des horodatages incohérents désynchronisent l'appariement
image par image. Corrigé par `setpts=N/(60*TB)` sur les deux flux avant
comparaison : force un horodatage `i/60` reconstruit uniquement à partir du
numéro d'image décodée, ignorant l'horodatage (mauvais) du conteneur. Un
second piège lié : `framesync` répète par défaut la dernière image du flux le
plus court au-delà de son EOF (`eof_action=repeat`) plutôt que de s'arrêter —
sans `trim=end_frame=901` sur la référence (les quatre flux codent tous
exactement 901 images, vérifié), les 119 dernières paires de la référence à
1020 images auraient été comparées à une image de test figée, biaisant la
moyenne. Script complet : `spike/mesures.sh` (copié dans le scratchpad de
cette tâche, décrit ci-dessous).

**Image de comparaison.** Image d'indice 120 (piège documenté du tampon VBV
à 1 image : la première image, seule intra-only, est plafonnée à ~500 kbit
et reste image de référence du flux pendant environ 2 s — l'extraire
mesurerait un artefact de réglage de latence, pas le codec).

**Débit.** 10 Mbps pour les quatre combinaisons, comme demandé — voir plus
bas pourquoi une seule des quatre ne le respecte pas.

## Deux écarts rencontrés pendant la mise au point

1. **Motif plein cadre trop dense pour le contrôle de débit CBR.** La
   première version du motif (alternance rouge/bleu sur tout l'écran)
   produisait un débit réel de 15-18 Mbps pour une cible à 10 Mbps, quel que
   soit le codec — le tampon VBV à 1 image (`vbvBufferSize = bitrate/fps`,
   posé en Tâche 2/3 pour borner la latence) ne peut pas absorber un contenu
   aussi uniformément incompressible. Confiner le motif à un panneau de
   1200×800 px (le reste de l'image en fond uni, ~0 bit après la première
   image) a résolu le problème pour trois des quatre codecs.

2. **`multiPass` du préréglage P4 cassait la conformité au débit,
   différemment selon le profil.** Non lié au motif : le préréglage
   `NV_ENC_PRESET_P4_GUID` active par défaut `NV_ENC_TWO_PASS_QUARTER_RESOLUTION`.
   Sur le motif dense (plein cadre), ça produisait un débit réel
   erratique et dépendant du profil (jusqu'à 7,4× la cible en H.264 4:4:4,
   alors que HEVC 4:4:4 sur le même contenu restait proche de la cible).
   Corrigé dans `sky-encode/src/nvenc.rs` :
   `config.rcParams.multiPass = NV_ENC_MULTI_PASS_DISABLED`. Ce changement
   s'applique aussi à la sous-commande `encode` (Q2) — revalidé après coup
   (voir Auto-relecture) : verdict p99 toujours SUCCÈS, aucune régression
   observée.

   **Résidu non résolu : H.264 4:4:4 dépasse quand même la cible, de façon
   indépendante de la cible elle-même.** Testé à 3, 5, 10, 20 et 30 Mbps de
   cible sur le motif confiné au panneau (donc après la correction #1
   ci-dessus) : le débit réel reste stable à 67-72 Mbps sur la plage
   3-20 Mbps (indépendant de la cible — pas une simple erreur d'échelle),
   puis retombe à 14,8 Mbps à 30 Mbps de cible. HEVC 4:4:4, AV1 4:2:0 et
   H.264 4:2:0, avec exactement la même configuration par ailleurs (même
   fonction `initialiser`, mêmes `rcParams`, seul le profil/`chromaFormatIDC`
   change), respectent leur cible à 10-20 % près. C'est donc spécifique au
   profil H.264 High 4:4:4 Predictive sur ce pilote/matériel (RTX 4060,
   pilote de la machine de test), pas un oubli de configuration de notre
   côté — vérifié en journalisant `entropyCodingMode`, `chromaFormatIDC`,
   `vbvBufferSize/InitialDelay` et `averageBitRate/maxBitRate` juste avant
   `nvEncInitializeEncoder` : identiques entre H.264 4:2:0 (conforme) et
   H.264 4:4:4 (non conforme), à l'exception du profil et de
   `chromaFormatIDC`. Non creusé plus loin (hors profondeur raisonnable pour
   ce spike) — voir « Ce que ces chiffres n'établissent pas ».

## Résultats

Quatre flux, 901 images chacun (15 s à 60 i/s, vérifié identique sur les
quatre — condition de comparabilité), 10 Mbps de cible.

| Codec | Débit réel | Octets | PSNR Y | PSNR U | PSNR V | SSIM Y | SSIM U | SSIM V | SSIM All |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| H.264 4:2:0 | 8,44 Mbps | 15 831 034 | 50,23 dB | **18,21 dB** | **19,29 dB** | 0,9989 | 0,7674 | 0,8328 | 0,8664 |
| AV1 4:2:0 | 10,04 Mbps | 18 826 580 | 55,28 dB | **18,12 dB** | **19,27 dB** | 0,9996 | 0,7684 | 0,8347 | 0,8676 |
| HEVC 4:4:4 | 9,37 Mbps | 17 577 575 | 74,91 dB | **58,30 dB** | **48,58 dB** | 1,0000 | 1,0000 | 0,9999 | 1,0000 |
| H.264 4:4:4 ⚠ | 70,93 Mbps | 133 029 436 | 29,36 dB | 29,29 dB | 29,24 dB | 0,8895 | 0,9680 | 0,9649 | 0,9408 |

⚠ H.264 4:4:4 n'a **pas** tenu le débit cible (70,93 Mbps réels contre 10 Mbps
visés, 7,1×) — voir « Résidu non résolu » ci-dessus. Sa ligne n'est **pas**
une comparaison à débit égal et ne doit pas être lue comme telle ; elle est
incluse pour complétude et parce qu'elle est elle-même un résultat notable
(voir plus bas).

Les trois autres lignes (H.264 4:2:0, AV1 4:2:0, HEVC 4:4:4) sont à débit
comparable : 8,44 à 10,04 Mbps, un écart de 19 % max autour de la cible à
10 Mbps — comparaison à débit égal valide entre elles.

**Fenêtre de mesure : images 120 à 900 (781 images), pas la séquence
entière.** Le piège documenté du tampon VBV à 1 image produit une rampe de
qualité sur les ~2 premières secondes (la première image, seule intra-only,
est plafonnée à ~500 kbit ; le PSNR Y met environ 95 images à atteindre son
régime établi — vérifié sur les logs `mesures/psnr-*.log`). Une première
version de ce document et du script `mesures.sh` annonçait exclure cette
rampe sans le faire réellement (le filtre ne coupait que la fin de la
référence, pas le début des deux flux) ; corrigé — voir « Note de correction »
en fin de document. Effet du correctif : Y monte de 0,3 à 2,9 dB selon le
codec (la rampe pèse plus sur les flux qui démarrent moins bien), U/V bougent
de 0,01 à 1,5 dB — la rampe touche presque exclusivement la luminance
(image intra-only, U/V y sont déjà proches de leur régime établi dès la
première image). Aucune conclusion de ce rapport ne change de sens.

**Temps d'encodage (mesuré côté `sky-probe`, indépendant de la fenêtre
PSNR/SSIM ci-dessus — ce sont les 901 images, tampon de sortie 1 image) :**

| Codec | Encodage médian | Encodage p99 |
|---|---:|---:|
| H.264 4:2:0 | 4,78 ms | 6,21 ms |
| H.264 4:4:4 ⚠ | 4,94 ms | 12,84 ms |
| HEVC 4:4:4 | 5,35 ms | 6,97 ms |
| AV1 4:2:0 | 3,95 ms | 4,82 ms |

Les quatre restent largement sous le seuil de 16 ms (60 i/s) posé en Tâche 2 ;
H.264 4:4:4 a le p99 le plus élevé des quatre, cohérent avec son débit réel
7× supérieur (plus d'octets à produire et à copier par image).

## Ce que ces chiffres établissent

**L'écart entre 4:2:0 et 4:4:4 sur les plans de chrominance est mesurable et
large, à débit comparable.** HEVC 4:4:4 (9,37 Mbps) contre la moyenne de
H.264 4:2:0 (8,44 Mbps) et AV1 4:2:0 (10,04 Mbps) : **+40,1 dB sur U, +29,3 dB
sur V** — un facteur d'erreur quadratique moyenne (MSE) de l'ordre de 850×
(V) à 10 200× (U) plus faible sur ces deux plans. La luminance (Y), elle, est
bonne dans les trois cas (50,2 à 74,9 dB) — c'est exactement ce que prédit la
thèse du projet : le 4:2:0 ne dégrade pas la luminance, il dégrade
spécifiquement la couleur, et l'écart se voit sur U/V, pas sur Y. SSIM va
dans le même sens : U/V autour de 0,77-0,83 en 4:2:0 contre ~1,000 en HEVC
4:4:4.

**L'argument le plus solide de cette mesure : H.264 4:2:0 et AV1 4:2:0
convergent étroitement sur U et V, alors que ce sont deux codecs différents
avec des efficacités de compression très différentes.** Sur U : 18,21 dB
contre 18,12 dB (écart 0,09 dB). Sur V : 19,29 dB contre 19,27 dB (écart
0,02 dB). Et pourtant leur luminance diverge nettement : 50,23 dB contre
55,28 dB (écart 5,05 dB) — AV1 compresse Y sensiblement mieux à ce débit,
mais pas U/V. Si le plancher chroma vers 18-19 dB était une particularité
d'un encodeur donné (un réglage NVENC spécifique à H.264, par exemple), on
n'aurait aucune raison de le retrouver quasi identique chez AV1, un codec
distinct partageant seulement le sous-échantillonnage 4:2:0. Cette
convergence est la preuve que le plancher observé est un artefact du
**sous-échantillonnage lui-même**, pas d'un choix d'implémentation d'un
codec en particulier — ce qui rend la conclusion robuste même en excluant
H.264 4:4:4 du calcul (voir plus bas, « limite de l'étude »).

**Le motif de test (alternance rouge/bleu pixel à pixel) est un pire cas
délibéré**, pas une moyenne représentative d'un écran de travail réel — les
chiffres ci-dessus bornent l'écart maximal observable sur ce type de
contenu, pas l'écart moyen sur un usage courant.

**H.264 4:4:4 sur ce pilote/matériel ne tient pas une cible de débit basse en
CBR sur ce contenu**, quelle que soit la cible testée (3 à 30 Mbps) — un
constat opérationnel indépendant de la question chroma, mais qui pèse dans
le choix du codec : même en lui laissant 7× le débit, sa qualité reste
inférieure à HEVC 4:4:4 à débit nominal (PSNR Y 29,36 dB contre 74,91 dB) et
étrangement plate entre plans (Y≈U≈V≈29,2-29,4 dB, alors qu'on attendrait Y
nettement meilleur que U/V comme sur les trois autres flux) — signe d'un
contrôle de débit qui alloue mal les bits pour ce profil sur ce contenu,
plutôt que d'un manque réel de bande passante.

**Limite de l'étude : la branche 4:4:4 ne repose que sur un seul point de
données (HEVC).** H.264 4:4:4 étant exclu de la comparaison à débit égal
(résidu non résolu ci-dessus), tout ce que ce rapport dit sur « le 4:4:4 »
vient d'un seul codec, HEVC. Ce n'est pas un défaut de méthode — HEVC 4:4:4
est la seule combinaison 4:4:4 qui a effectivement tenu son débit cible sur
ce contenu — mais c'est une limite honnête à connaître avant de généraliser
la conclusion : la mesure établit que *HEVC 4:4:4* bat *H.264/AV1 4:2:0* sur
la chrominance à débit comparable, pas que « le 4:4:4 » en général le fait
quel que soit le codec qui le porte.

## Ce que ces chiffres n'établissent pas

**La lisibilité perçue du texte.** Un PSNR chroma de 18-19 dB en 4:2:0 dit
qu'il y a une grande distance numérique entre la couleur décodée et la
couleur d'origine ; il ne dit pas si un caractère reste reconnaissable à
l'œil sur un écran réel, à une distance de lecture réelle, avec un
antialiasing de rendu de police réel (notre motif n'a pas d'antialiasing —
c'est un pire cas de bord dur, un vrai rendu de police en a). **Ce jugement
revient au propriétaire du projet** — voir les images extraites ci-dessous.

**Un usage représentatif.** Le motif est un pire cas construit, pas une
capture d'écran réelle. L'écart mesuré ne doit pas être lu comme « le partage
d'écran en 4:2:0 sera systématiquement 30 dB pire en couleur » — seulement
comme « sur du texte fin à couleurs saturées, l'écart existe et il est de cet
ordre ».

**La qualité de H.264 4:4:4 en général.** Le résidu de contrôle de débit
documenté plus haut est spécifique à ce contenu, ce pilote et cette carte ;
il n'a pas été testé sur du contenu moins extrême ni sur un autre GPU/pilote.
Ne pas en conclure que le profil H.264 High 4:4:4 est inutilisable en
général — seulement qu'il s'est comporté ainsi ici, dans ces conditions
précises.

## Images extraites, pour l'examen visuel du propriétaire

Image d'indice 120 (15 s de flux, hors piège VBV des 2 premières secondes),
image complète 2560×1440 et recadrage sur le seul panneau de texte (colonnes
200-1400, lignes 94-864 de l'image affichée) :

| | Image complète | Recadrage panneau |
|---|---|---|
| Référence (non compressée) | `spike/mesures/frame120-reference.png` | `spike/mesures/crop-reference.png` |
| H.264 4:2:0 | `spike/mesures/frame120-h264-420.png` | `spike/mesures/crop-h264-420.png` |
| H.264 4:4:4 ⚠ débit non tenu | `spike/mesures/frame120-h264-444.png` | `spike/mesures/crop-h264-444.png` |
| HEVC 4:4:4 | `spike/mesures/frame120-hevc-444.png` | `spike/mesures/crop-hevc-444.png` |
| AV1 4:2:0 | `spike/mesures/frame120-av1-420.png` | `spike/mesures/crop-av1-420.png` |

Chemins absolus : `D:\skyshare\spike\mesures\*.png`.

Note d'observation (pas une conclusion de lisibilité) : à l'œil, le
recadrage H.264 4:2:0 a une teinte visiblement plus rosée/magenta que la
référence et HEVC 4:4:4, qui restent d'un rouge plus pur — cohérent avec le
moyennage rouge/bleu qu'impose le sous-échantillonnage chroma, et avec
l'écart PSNR U/V mesuré. Ce n'est qu'une observation de teinte, pas un
jugement de lisibilité.

## Fichiers produits (non commités — voir `spike/.gitignore`)

Trop volumineux pour git, régénérables à volonté via
`cargo run --release -p sky-probe -- codecs --seconds 15 --bitrate-mbps 10`
(environ 1 min avec la référence CPU + quatre encodages GPU) :

- `spike/cmp-reference.bgra` — rawvideo BGRA, 15 GB (1020 images × 2560×1440×4)
- `spike/cmp-h264-420.h264`, `cmp-h264-444.h264` (H.264 Annex B)
- `spike/cmp-hevc-444.h265` (HEVC Annex B)
- `spike/cmp-av1-420.ivf` (AV1, conteneur IVF)

Les images PNG (`spike/mesures/*.png`) et le script de mesure
(`spike/mesures.sh`, ci-dessous) sont commités.

## Reproduire la mesure

```bash
cargo run --release -p sky-probe -- codecs --seconds 15 --bitrate-mbps 10
bash spike/mesures.sh   # PSNR/SSIM par plan + extraction PNG, ~2-3 min
```

## Fichiers modifiés

- `spike/crates/sky-probe/src/cmd_codecs.rs` (créé) — sous-commande `codecs` :
  encode les 4 combinaisons sur la texture synthétique (compteur remis à 0
  par codec), écrit la référence brute côté CPU, résumé chiffré en console.
- `spike/crates/sky-probe/src/cmd_encode.rs` — motif du texte de test refait
  (rouge/bleu saturés, panneau confiné — voir « Motif de test » ci-dessus) ;
  boucle d'encodage extraite en deux fonctions partagées
  (`encoder_ecran`, `encoder_synthetique`) pour que `cmd_codecs` réutilise
  exactement le même code que Q2 plutôt que de le dupliquer ; ajout de
  `ecrire_reference_brute` (référence CPU, sans aller-retour GPU) ;
  visibilité de `TextureSynthetique`/`FPS` élargie à `pub(crate)`.
- `spike/crates/sky-probe/src/main.rs` — sous-commande `codecs` (seconds,
  bitrate-mbps, monitor).
- `spike/crates/sky-encode/src/nvenc.rs` — `multiPass` du préréglage P4
  désactivé explicitement (voir « Deux écarts rencontrés », point 2).
- `spike/.gitignore` (créé) — exclut les flux vidéo générés et les logs de
  mesure, jamais commités.
- `spike/mesures/*.png` (créé) — images extraites, commitées.
- `spike/mesures.sh` (créé) — script de mesure PSNR/SSIM/extraction,
  commité pour reproductibilité.

## Auto-relecture

- **Bug de documentation trouvé et corrigé** : un premier jet avait collé le
  commentaire doc de `motif_detaille` directement au-dessus des constantes
  `PANNEAU_*` sans ligne vide séparatrice — en Rust, ça rattache tout le bloc
  à l'item suivant (`PANNEAU_X0`) et prive `motif_detaille` de sa
  documentation. Réorganisé : chaque bloc doc est maintenant immédiatement
  au-dessus de l'item qu'il documente.
- `cargo build --release -p sky-probe` : compile sans avertissement.
- `cargo clippy --release -p sky-probe -p sky-encode` : 3 avertissements de
  style (`manual_range_contains`, `manual_is_multiple_of`) corrigés ; clean
  après correction.
- Non-régression Q2 : `sky-probe encode --source synthetique` retesté après
  le correctif `multiPass` (issu de ce ticket mais partagé avec Q2) —
  toujours SUCCÈS (p99 < 16 ms), débit réel plus proche de la cible qu'avant
  le correctif (amélioration, pas une régression).
- Déterminisme vérifié : les quatre flux comptent exactement 901 images
  chacun (`ffprobe -count_frames`) — condition nécessaire pour que la
  comparaison porte sur la même séquence d'un bout à l'autre.
- Référence vérifiée à l'octet près sur 3 pixels connus (rouge, bleu, fond)
  avant de faire confiance au reste du calcul PSNR/SSIM.

## Note de correction (revue)

Trois défauts remontés par la revue, corrigés ici. Le script committé et les
chiffres du document ci-dessus sont déjà à jour ; cette note documente ce qui
a changé et comment le reproduire.

**1. Le p99 par codec manquait dans ce document** (brief Step 5 : « pour
chaque codec... temps d'encodage p99 »). Calculé côté `sky-probe`
(`cmd_codecs.rs`) mais seulement imprimé en console, jamais transcrit ici.
Corrigé : table « Temps d'encodage » ajoutée dans la section Résultats.

**2. `mesures.sh` annonçait exclure la rampe VBV sans le faire.** Le libellé
affiché disait « hors 2 premières secondes du piège VBV », mais le filtre
`ffmpeg` réel ne coupait que la fin de la référence (`trim=end_frame=901`),
jamais le début — ni sur la référence, ni sur le flux testé. Toute la
séquence, rampe comprise, entrait dans la moyenne. Corrigé en appliquant
`trim=start_frame=120:end_frame=901` **aux deux flux** (référence et flux
testé), pour que l'image k du flux testé continue de correspondre à l'image
k de la référence après la coupe — sans quoi couper un seul des deux flux
aurait désynchronisé l'appariement plutôt que de le corriger. Commande
verbatim (exécutée depuis `spike/`, sur les fichiers déjà encodés, sans
réencodage) :

```bash
bash mesures.sh
```

qui lance, pour chaque codec, exactement :

```bash
ffmpeg -v error -y -f rawvideo -pix_fmt bgra -video_size 2560x1440 -framerate 60 \
  -i cmp-reference.bgra -i cmp-<codec>.<ext> \
  -lavfi "[0:v]trim=start_frame=120:end_frame=901,setpts=N/(60*TB),scale=out_range=full,format=yuv444p,split=2[r1][r2];
          [1:v]trim=start_frame=120:end_frame=901,setpts=N/(60*TB),format=yuv444p,split=2[t1][t2];
          [r1][t1]psnr=stats_file=mesures/psnr-<codec>.log;
          [r2][t2]ssim=stats_file=mesures/ssim-<codec>.log" \
  -f null -
```

(filtre reformaté sur plusieurs lignes ici pour la lisibilité ; une seule
ligne dans le script réel.)

Effet mesuré — ancien tableau (séquence entière, 901 images) vs nouveau
(régime établi, images 120-900, 781 images) :

| Codec | Y avant | Y après | U avant | U après | V avant | V après |
|---|---:|---:|---:|---:|---:|---:|
| H.264 4:2:0 | 49,75 | 50,23 (+0,48) | 18,20 | 18,21 (+0,01) | 19,28 | 19,29 (+0,01) |
| AV1 4:2:0 | 54,95 | 55,28 (+0,33) | 18,12 | 18,12 (+0,00) | 19,27 | 19,27 (+0,00) |
| HEVC 4:4:4 | 71,97 | 74,91 (+2,94) | 56,81 | 58,30 (+1,49) | 47,82 | 48,58 (+0,76) |
| H.264 4:4:4 ⚠ | 29,56 | 29,36 (−0,20) | 29,36 | 29,29 (−0,07) | 29,31 | 29,24 (−0,07) |

Conforme à l'attendu de la revue : Y bouge de 0,2 à 2,9 dB (la rampe touche
la luminance, pas la couleur — cohérent avec une image intra-only en tête de
flux), U/V bougent de 0,00 à 1,49 dB (déjà proches de leur régime établi dès
la première image). Aucune conclusion qualitative de ce rapport ne change ;
les écarts 4:2:0 vs 4:4:4 se creusent même légèrement (HEVC gagne plus que
les deux 4:2:0 sur Y).

**3. La correspondance de matrice couleur était présentée comme acquise.**
Corrigé dans la section Méthode (« Ce qui est vérifié et ce qui ne l'est
pas, sur la matrice couleur ») : distinction explicite entre valeurs
absolues (portent la réserve, matrice interne de NVENC non revérifiée
empiriquement — seule la plage l'a été, en Tâche 3) et écart relatif
4:2:0/4:4:4 (n'en dépend pas, le convertisseur RGB→YUV étant un étage commun
aux quatre flux, en amont du choix de codec).

**Ajout demandé, non un défaut** : la section « Ce que ces chiffres
établissent » met maintenant en avant l'argument de convergence H.264
4:2:0 / AV1 4:2:0 sur U et V (écart de 0,01-0,09 dB entre deux codecs dont la
luminance diverge de 5 dB) comme preuve que le plancher chroma mesuré est un
artefact du sous-échantillonnage, pas d'un réglage propre à un encodeur — et
signale explicitement que la branche 4:4:4 ne repose que sur HEVC (un seul
point de données), H.264 4:4:4 étant exclu de la comparaison à débit égal.
