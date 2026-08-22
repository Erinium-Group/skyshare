# Comparatif des 4 codecs à débit égal (Q3)

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
identiques à BT.470BG que NVENC applique côté encodeur, plage complète —
`videoFullRangeFlag=1` dans `nvenc.rs`). Vérifié sur un pixel rouge pur :
Y=76, U=84, V=255, conforme à la formule BT.601 pleine plage (Y=76,2,
U=84,9, V=255 après écrêtage).

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
| H.264 4:2:0 | 8,44 Mbps | 15 831 034 | 49,75 dB | **18,20 dB** | **19,28 dB** | 0,9976 | 0,7672 | 0,8326 | 0,8658 |
| AV1 4:2:0 | 10,04 Mbps | 18 826 580 | 54,95 dB | **18,12 dB** | **19,27 dB** | 0,9994 | 0,7683 | 0,8346 | 0,8674 |
| HEVC 4:4:4 | 9,37 Mbps | 17 577 575 | 71,97 dB | **56,81 dB** | **47,82 dB** | 0,9989 | 0,9988 | 0,9987 | 0,9988 |
| H.264 4:4:4 ⚠ | 70,93 Mbps | 133 029 436 | 29,56 dB | 29,36 dB | 29,31 dB | 0,8935 | 0,9685 | 0,9655 | 0,9425 |

⚠ H.264 4:4:4 n'a **pas** tenu le débit cible (70,93 Mbps réels contre 10 Mbps
visés, 7,1×) — voir « Résidu non résolu » ci-dessus. Sa ligne n'est **pas**
une comparaison à débit égal et ne doit pas être lue comme telle ; elle est
incluse pour complétude et parce qu'elle est elle-même un résultat notable
(voir plus bas).

Les trois autres lignes (H.264 4:2:0, AV1 4:2:0, HEVC 4:4:4) sont à débit
comparable : 8,44 à 10,04 Mbps, un écart de 19 % max autour de la cible à
10 Mbps — comparaison à débit égal valide entre elles.

PSNR/SSIM calculés sur les 901 images (soit 15 s), pas seulement sur l'image
120 — la mesure porte sur toute la séquence, l'image 120 sert uniquement à
l'inspection visuelle.

## Ce que ces chiffres établissent

**L'écart entre 4:2:0 et 4:4:4 sur les plans de chrominance est mesurable et
large, à débit comparable.** HEVC 4:4:4 (9,37 Mbps) contre H.264 4:2:0
(8,44 Mbps) et AV1 4:2:0 (10,04 Mbps) : **+38,6 dB sur U, +28,5 à +28,6 dB
sur V** — un facteur d'erreur quadratique moyenne (MSE) de l'ordre de
70-700× plus faible sur ces deux plans. La luminance (Y), elle, est bonne
dans les trois cas (49,7 à 72,0 dB) — c'est exactement ce que prédit la
thèse du projet : le 4:2:0 ne dégrade pas la luminance, il dégrade
spécifiquement la couleur, et l'écart se voit sur U/V, pas sur Y. SSIM va
dans le même sens : U/V autour de 0,77-0,83 en 4:2:0 contre 0,999 en HEVC
4:4:4.

**Le motif de test (alternance rouge/bleu pixel à pixel) est un pire cas
délibéré**, pas une moyenne représentative d'un écran de travail réel — les
chiffres ci-dessus bornent l'écart maximal observable sur ce type de
contenu, pas l'écart moyen sur un usage courant.

**H.264 4:4:4 sur ce pilote/matériel ne tient pas une cible de débit basse en
CBR sur ce contenu**, quelle que soit la cible testée (3 à 30 Mbps) — un
constat opérationnel indépendant de la question chroma, mais qui pèse dans
le choix du codec : même en lui laissant 7× le débit, sa qualité reste
inférieure à HEVC 4:4:4 à débit nominal (PSNR Y 29,56 dB contre 71,97 dB) et
étrangement plate entre plans (Y≈U≈V≈29,3-29,6 dB, alors qu'on attendrait Y
nettement meilleur que U/V comme sur les trois autres flux) — signe d'un
contrôle de débit qui alloue mal les bits pour ce profil sur ce contenu,
plutôt que d'un manque réel de bande passante.

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
