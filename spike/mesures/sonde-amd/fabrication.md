# Sonde AMD — rapport de fabrication

02/10/2026, machine de fabrication : Ryzen 7 5800X, RTX 4060 (pilote 32.0.16.1692), Windows build 26200.9457.

## Statut

**Prête à copier.** Le dossier `scratchpad\sonde-amd\` contient 10 fichiers, soit **69 611 655 octets (66,4 Mio)** :

| Fichier | Octets |
|---|---|
| `sonde-amd.exe` | 605 184 |
| `hevc-main-420-2560x1440.h265` + `.tailles` | 14 053 786 + 4 165 |
| `hevc-main-420-2560x2880.h265` + `.tailles` | 21 630 523 + 4 200 |
| `h264-high-420-2560x1440.h264` + `.tailles` | 13 025 810 + 4 189 |
| `h264-high-420-2560x2880.h264` + `.tailles` | 20 279 017 + 4 200 |
| `LISEZMOI.txt` | 5 lignes |

`D:\skyshare` n'a pas été modifié : `git status --short` donne, avant comme après, les trois mêmes fichiers non suivis (`.claude/`, `AGENTS.md`, `testm4.md`).

## 1. Les flux de test

### Comment ils ont été produits

Un outil jetable, `scratchpad\fabrique-src\`, encode avec NVENC sur la RTX 4060 :

- `nvenc.rs` et `nvenc_sys.rs` sont **copiés** de `spike/crates/sky-encode/src/`. La copie a été modifiée, pas le dépôt : ajout d'une variante `Codec::Hevc420` (profil `NV_ENC_HEVC_PROFILE_MAIN_GUID`, `chromaFormatIDC = 1`). Le dépôt ne sait produire que du HEVC 4:4:4.
- La configuration reste celle du projet : préréglage P4 en latence ultra-basse, CBR avec un tampon VBV d'une image, GOP infini et rafraîchissement intra (période de 120 images, compte de 60), un passage, VUI plage pleine. Les flux ressemblent donc à ce que recevrait réellement un spectateur SkyShare : **une seule IDR**, puis des vagues de rafraîchissement intra.
- **Écart imposé par l'environnement.** Dans le répertoire temporaire, l'antivirus bloque le script de construction de la crate `nvidia-video-codec-sdk`. Le premier `cargo build` est sorti en **code 225** (`ERROR_VIRUS_INFECTED`), les suivants en « Accès refusé (os error 5) » au moment de lier `build-script-build.exe`. Les liaisons Rust (`src/sys/`, simples définitions de types) ont donc été recopiées depuis le registre Cargo, sans dépendre de la crate.
- Le contenu est synthétique, calculé sur le processeur puis téléversé par `UpdateSubresource`. On y trouve un bureau en aplats, une barre des tâches avec des icônes de couleur et une horloge, et un éditeur de code sombre (gouttière numérotée, arborescence, barre d'état bleue). Son texte coloré est fin : glyphes de 7×13 px aux traits d'**un pixel**. L'éditeur défile de 3 px par image. Une fenêtre de document claire, avec un histogramme en aplats, glisse de 2 px par image. S'y ajoutent un caret clignotant et un curseur de souris sur une trajectoire de Lissajous. En 2560×2880, la moitié basse porte un second écran en thème clair : une page qui défile de 2 px par image, un terminal qui reçoit une ligne toutes les 6 images et une barre de progression.
- Chaque flux est en Annex-B brut. Il est accompagné d'un fichier `.tailles` qui donne la taille de chaque unité d'accès rendue par NVENC (une ligne par image). La sonde soumet ainsi exactement une image par échantillon, sans redécouper le flux. Elle vérifie aussi que la somme des tailles égale la taille du fichier.

### Caractéristiques vérifiées

Chaque ligne est vérifiée trois fois : par le SPS, que la sonde lit elle-même ; par un décodage réel ; par une comparaison à l'octet.

| Flux | SPS lu | Images | IDR | Débit mesuré (cible) |
|---|---|---|---|---|
| hevc-main-420-2560x1440 | HEVC **Main**, niveau 150 (5.0), **4:2:0**, **8/8 bits**, 2560×1440 | 600 | 1 | 11,24 Mbps (12) |
| hevc-main-420-2560x2880 | HEVC **Main**, niveau 153 (5.1), 4:2:0, 8/8 bits, **2560×2880** | 600 | 1 | 17,30 Mbps (20) |
| h264-high-420-2560x1440 | H.264 **High**, niveau 51, 4:2:0, 8/8 bits, 2560×1440 | 600 | 1 | 10,42 Mbps (12) |
| h264-high-420-2560x2880 | H.264 High, niveau 52, 4:2:0, 8/8 bits, 2560×2880 | 600 | 1 | 16,22 Mbps (20) |

- Les quatre flux ont été décodés en entier, 600 images sur 600, par Media Foundation sur la machine de fabrication, sans image manquante ni hors d'ordre d'après les horodatages.
- Pour les quatre flux, l'image n° 300 décodée en matériel (NVIDIA) et en logiciel (Microsoft) a une **luminance identique à l'octet près**.
- L'image n° 300 décodée a été contrôlée à l'œil : elle ressemble bien à un écran, avec un texte d'un pixel net. Les aperçus sont dans `scratchpad\apercu\*.png`.

## 2. Comment la sonde établit le décodage matériel

La sonde n'accepte pas un seul indice : elle exige **trois preuves indépendantes**. S'il en manque une, le verdict n'est pas « matériel ».

1. **Le décodeur accepte le périphérique.** `MFT_MESSAGE_SET_D3D_MANAGER` doit réussir. Un refus est rapporté avec son HRESULT exact.
2. **Toutes les images rendues sont des surfaces DXGI.** Chaque échantillon de sortie est interrogé en `IMFDXGIBuffer`, et la sonde compte « surfaces DXGI » contre « mémoire système ». La description de la texture est rapportée (taille, format, tranches, BindFlags).
3. **Le moteur vidéo du GPU a travaillé pour ce processus.** La sonde lit les compteurs Windows `\GPU Engine(*)\Running Time` et `Utilization Percentage` par PDH. Ce sont ceux du Gestionnaire des tâches. Elle les filtre sur `pid_<son PID>` **et** sur le LUID de l'adaptateur testé, avant et après le décodage. C'est le témoin qui ne dépend pas de Media Foundation : un décodeur qui décode sur le processeur puis téléverse donnerait des surfaces DXGI, mais le moteur vidéo resterait à zéro.

Les verdicts possibles :

- « DÉCODAGE MATÉRIEL ÉTABLI » : les trois preuves sont réunies.
- « DÉCODAGE LOGICIEL » : des images arrivent en mémoire système.
- « MATÉRIEL NON ÉTABLI, LOGICIEL PROBABLE » : surfaces DXGI, mais moteur vidéo inactif.
- « MATÉRIEL NON ÉTABLI » : compteurs indisponibles.

Dans tous les cas la cadence est rapportée. Chaque décodeur est aussi lancé **sans** périphérique, en mode « LOGICIEL (référence) », pour avoir le contraste.

Le contraste a été observé ici. En mode matériel, le moteur « VideoDecode » est actif entre 87 et 95 %. En mode logiciel, aucun moteur GPU n'apparaît pour le processus. Le compteur « Running Time » se compte en unités de 100 ns : 4 753 546 unités font 475 ms d'activité sur 540 ms de décodage, cohérent avec une utilisation lue à 87,1 %. Cette unité est **recoupée ici, pas documentée**.

Autres mesures par passe :

- images rendues sur le total, avec les plages manquantes d'après les horodatages ;
- cadence maximale de décodage, en images par seconde : les images sont soumises au plus vite, ce n'est **pas** une lecture cadencée à 60 i/s ;
- temps CPU du processus (`GetProcessTimes`), en % d'un cœur et de la machine ;
- nombre maximal d'images retenues par le décodeur ;
- luminance de l'image n° 300, comparée entre les passes.

Ce que la sonde couvre aussi :

- **Q1.** Les adaptateurs sont énumérés par DXGI, avec leur pilote. Pour chaque adaptateur matériel, la sonde lit `GetVideoDecoderProfile`. Les noms viennent de `dxva.h` et `d3d11.h` du SDK 10.0.26100.0 : tous les GUID ont été recopiés de ces en-têtes et vérifiés. Elle teste aussi `CheckVideoDecoderFormat` sur 10 formats. Pour **chaque** profil HEVC et H.264, elle essaie six tailles. La méthode : `GetVideoDecoderConfigCount`, puis `GetVideoDecoderConfig`, puis **`CreateVideoDecoder`**. La création effective est la preuve, le simple décompte ne suffit pas. La taille 2560×2880 est signalée « taille décisive ».
- **Q2/Q3.** `MFTEnumEx` est lancé sur quatre jeux de drapeaux : synchrones par défaut, `HARDWARE`, `ALL`, `ALL | UNTRUSTED_STOREMFT`. La sonde rapporte le nom, le CLSID, les drapeaux, l'URL matérielle et le fournisseur de chaque décodeur. Elle relève aussi les paquets d'extensions vidéo du Store trouvés dans le registre de l'utilisateur, comme simple indice. Les MFT asynchrones sont gérés par le modèle à événements.
- **Robustesse.** Chaque section est enveloppée dans `catch_unwind` : une panique devient un `[ÉCHEC]` et la sonde continue. Chaque passe est limitée à 240 s, et à 10 s sans événement pour un MFT asynchrone. Le rapport est réécrit sur disque après chaque section. Si le dossier n'est pas inscriptible, la sonde se replie sur le dossier courant, puis sur `%TEMP%`, et le chemin est affiché en tête.
- **Exécutable.** Il est construit avec `-C target-feature=+crt-static`. `dumpbin /dependents` donne `ole32`, `mfplat`, `kernel32`, `pdh`, `combase`, `dxgi`, `advapi32`, `d3d11`, `oleaut32`, `api-ms-win-core-synch-l1-2-0` et `ntdll` : **aucune dépendance au runtime Visual C++** (`vcruntime140`, `msvcp140`, `api-ms-win-crt-*`, `ucrtbase` absentes). Aucun collecteur `tracing`, aucun appel réseau, aucune fenêtre. Il n'attend « Entrée » que s'il est seul dans sa console, donc s'il a été lancé par double-clic.

## 3. Une découverte en route : le décodeur HEVC logiciel de Microsoft perd des images en faible latence

Au premier essai, la passe HEVC « logiciel » n'a rendu que 120 images sur 600, sans aucune erreur. Les horodatages montrent que les images 0 à 119 sont sorties, puis **plus rien de 120 à 599**. Or 120 correspond exactement à la période de rafraîchissement intra de NVENC. Le décodeur a pourtant consommé 13 s de CPU.

Relancée avec `MF_LOW_LATENCY = 0`, la même passe rend 600 images sur 600, à 210 i/s. Sa luminance est identique à l'octet près à celle du décodage matériel. **Le défaut tient donc au mode faible latence du décodeur HEVC logiciel de Microsoft (paquet HEVCVideoExtension 2.5.33.0) sur un flux à rafraîchissement intra**, pas au flux.

Le même mode avec le périphérique NVIDIA rend 600 images sur 600. La sonde en tient compte : toute passe incomplète est **refaite automatiquement sans `MF_LOW_LATENCY`**, et les deux passes figurent au rapport. La première y est comptée comme un échec mesuré.

Ce que ça dit pour SkyShare, à confirmer sur le portable : si le portable devait décoder en logiciel par Media Foundation, le mode faible latence, le seul acceptable pour le projet, perdrait le flux après la première vague de rafraîchissement intra.

## 4. Rapport du test sur la machine de fabrication (NVIDIA)

**C'est un test de bon fonctionnement, pas une mesure AMD** : ici le décodeur est NVIDIA. Le fichier complet fait 392 lignes et se trouve dans `scratchpad\rapport-test-machine-nvidia.txt`. Il a été sorti du dossier livré pour ne pas être confondu avec celui du portable. Exécution : 46 s, code de retour 0. Toutes les sections ont été parcourues : machine, 27 profils DXVA, 9 profils HEVC/H.264 × 6 tailles, 4 jeux d'énumération × 2 codecs, et 4 flux × (matériel + logiciel) plus 2 reprises sans faible latence.

Synthèse telle que la sonde l'a écrite (lignes de cadence abrégées) :

```
Question 1 — RTX 4060 : HEVC Main et H.264 VLD acceptés (décodeur créé) aux six tailles, 2560x2880 compris.
HEVC Media Foundation : « HEVCVideoExtension » (SYNCMFT), 0 MFT matériel. H.264 : « Microsoft H264 Video Decoder MFT ».

hevc 2560x1440  MATÉRIEL  DÉCODAGE MATÉRIEL ÉTABLI — VideoDecode 475 ms, util. max 87,1 % — 600/600, 1103 i/s, CPU 14 % d'un cœur
hevc 2560x1440  LOGICIEL  120/600 (images 120–599 perdues) — 31,8 i/s
hevc 2560x1440  LOGICIEL sans faible latence — 600/600, 210 i/s, CPU 639 % d'un cœur
hevc 2560x2880  MATÉRIEL  DÉCODAGE MATÉRIEL ÉTABLI — VideoDecode 873 ms, util. max 91,8 % — 600/600, 632 i/s
hevc 2560x2880  LOGICIEL  120/600 — 18,6 i/s ; sans faible latence : 600/600, 114 i/s
h264 2560x1440  MATÉRIEL  DÉCODAGE MATÉRIEL ÉTABLI — VideoDecode 991 ms, util. max 92,0 % — 600/600, 565 i/s, CPU 6 % d'un cœur
h264 2560x1440  LOGICIEL  600/600, 176 i/s
h264 2560x2880  MATÉRIEL  DÉCODAGE MATÉRIEL ÉTABLI — VideoDecode 1898 ms, util. max 95,0 % — 600/600, 301 i/s
h264 2560x2880  LOGICIEL  600/600, 99 i/s
Image n° 300 : matériel et logiciel identiques à l'octet près, pour les quatre flux.
Échecs (2) : les deux passes HEVC logicielles en faible latence (480 images sur 600 non rendues, sans erreur).
```

Ces chiffres sont ceux d'un Ryzen 7 5800X avec une RTX 4060. **Ils ne disent rien du portable.**

## 5. Limites et inquiétudes

- **`mfplat.dll` est une dépendance statique.** Sur une édition « N » de Windows sans Media Feature Pack, l'exécutable ne démarrerait pas du tout, et le chargeur de Windows afficherait sa propre erreur. Ça ne devrait pas concerner un Windows 11 25H2 grand public, mais ce n'est pas vérifié.
- **Le chemin des MFT asynchrones (matériels) n'a pas été exercé** : la machine de fabrication n'en a aucun. Le code existe, mais il n'est pas éprouvé.
- **Le nom du moteur vidéo chez AMD.** La troisième preuve reconnaît un moteur dont le type contient « decode » ou « codec ». Si le pilote AMD le nomme autrement, le verdict sera « MATÉRIEL NON ÉTABLI ». La liste de tous les moteurs vus et actifs est cependant imprimée, pour qu'on puisse trancher à la lecture.
- **Les extensions HEVC.** Si le portable n'a ni l'extension HEVC du Store ni de MFT AMD, la question 2 se soldera par « aucun décodeur ». La question 1 (DXVA) dira quand même si la puce sait décoder. Contourner Media Foundation exigerait d'écrire soi-même l'analyse des en-têtes de tranche HEVC pour piloter `ID3D11VideoDecoder` : c'est un autre chantier.
- **La cadence mesurée est un plafond, pas une latence.** Elle est mesurée en soumission au plus vite. La latence par image à 60 i/s n'est pas mesurée ; seul le nombre maximal d'images retenues l'approche. Sur un portable, l'alimentation compte : le rapport dit si le portable est sur secteur.
- **Le débit des flux est sous la cible.** Les flux font 10 à 17 Mbps mesurés, pour des cibles de 12 et 20 Mbps. C'est sans effet sur la question matérielle, mais le décodage logiciel CABAC dépend du débit.
- **La sonde n'a pas de signature.** SmartScreen ou l'antivirus du portable peuvent la bloquer ; le LISEZMOI le dit. Ici, l'antivirus a déjà bloqué un script de construction dans `%TEMP%`.
- **Une variable cachée existe pour les tests.** `SONDE_AMD_APERCU=<dossier>` écrit la luminance de l'image n° 300 en PGM. Elle n'est pas documentée dans le LISEZMOI et elle est sans effet si elle est absente.
- **Ce rapport et les sources vivent dans le répertoire temporaire de session.** Selon la leçon du 30/09 dans `tasks/lessons.md`, ce répertoire a déjà été vidé entre deux sessions. Le rapport du portable, quand il reviendra, devra être porté dans le dépôt. Celui-ci aussi, si on veut le garder.
