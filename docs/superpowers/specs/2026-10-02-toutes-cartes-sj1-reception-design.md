# Toutes cartes graphiques, sous-jalon 1 — recevoir sur n'importe quelle carte — conception

**Date : 02/10/2026.** Premier des trois sous-jalons du jalon « toutes cartes graphiques » :
un spectateur **sans NVIDIA récente** (AMD, Intel, NVIDIA antérieure à Turing) reçoit et affiche
l'écran d'un hôte.

Documents qui fondent celui-ci :

- Note fondatrice : `docs/superpowers/notes/2026-08-23-compatibilite-toutes-cartes-graphiques.md`.
- Sonde sur le portable AMD : `spike/mesures/sonde-amd/` (`fabrication.md`,
  `rapport-test-machine-nvidia.txt`, `rapport-portable-amd.txt`).
- Jalon 2, dont ce sous-jalon part : `docs/superpowers/specs/2026-09-30-jalon-2-premier-pixel-design.md`.
- Décision de passer ce jalon avant l'essai du jalon 2 : `tasks/todo.md`, section « Décision du
  03/10/2026 ».

Branche : `jalon-toutes-cartes`, partie du jalon 2 (`4db8851`, non fusionné).

---

## §1 Ce que ce sous-jalon livre, et ce qu'il ne livre pas

**Il livre** : un spectateur qui n'a pas de décodeur HEVC 4:4:4 regarde quand même, en
**4:2:0**, HEVC si sa machine le décode, **H.264** sinon. Le format est **négocié** à chaque
connexion. Critère de fin : **la machine NVIDIA du propriétaire partage, le portable AMD
regarde** (§8).

**Sortent du périmètre, avec la raison :**

- **L'empaquetage** (voie A de la décision D1 : chrominance pleine résolution rangée dans une
  porteuse 4:2:0 de W×2H, recomposée par nuanceur). C'est le **sous-jalon 2**. En attendant, un
  spectateur 4:2:0 voit les petits textes colorés moins nets ; c'est assumé.
- **L'encodage hors NVIDIA** (AMF, Quick Sync, encodeur Media Foundation). C'est le
  **sous-jalon 3**. L'hôte reste NVIDIA dans ce sous-jalon.
- **Un décodage logiciel.** Aucun repli logiciel, comme au jalon 2 : un spectateur sans aucun
  décodeur matériel est refusé avant tout réseau (§6).

## §2 Ce qui est mesuré, ce qui ne l'est pas

**Mesuré** (sonde du 02/10/2026, portable Ryzen 7 5700U, Vega 8, pilote 31.0.21923.1000,
extension HEVCVideoExtension 2.5.33.0 ; rapports dans `spike/mesures/sonde-amd/`) :

- Media Foundation décode **en matériel**, sur un périphérique D3D11, les quatre flux d'essai
  (HEVC et H.264, 2560×1440 et 2560×2880) : **600 images sur 600** chacun, **identiques à l'octet**
  au décodage logiciel. Débits maximaux : HEVC 2560×1440 156,2 im/s ; H.264 2560×1440 275 im/s.
- D3D11 y déclare HEVC Main et Main10 en 4:2:0, H.264 et VP9 ; **aucun profil HEVC 4:4:4**.
  Tailles acceptées : HEVC jusqu'à 4096×4096, H.264 jusqu'à 4096×2304.
- **Le décodeur HEVC *logiciel* de Microsoft, avec `MF_LOW_LATENCY=1`, perd silencieusement toutes
  les images après la 120e** sur un flux en rafraîchissement intra (images 120 à 599 jamais
  rendues, aucune erreur). Constaté sur les deux machines et les deux tailles. **Le chemin
  matériel n'est pas touché** : avec un gestionnaire D3D11 et `MF_LOW_LATENCY=1`, il rend
  **600 images sur 600** et en retient **au plus une** entre l'entrée et la sortie (soumission au
  plus vite), pour les quatre flux et les deux machines. *Corrigé le 02/10/2026 : la première
  version de ce paragraphe attribuait la perte à Media Foundation en général ; le relevé des
  rapports la cantonne au décodage logiciel.*
- Aucun décodeur n'est énuméré avec `MFT_ENUM_FLAG_HARDWARE`, ni sur AMD ni sur NVIDIA. Le décodage
  matériel passe par les MFT **synchrones** de Microsoft (HEVCVideoExtension, « Microsoft H264
  Video Decoder MFT »), qui font leur DXVA en interne quand on leur confie un gestionnaire D3D11.
- L'image décodée sort dans une **tranche d'un tableau de textures** NV12 (13 tranches sur AMD,
  1 ou 8 sur NVIDIA), liée `D3D11_BIND_DECODER` seulement : **elle n'est pas lisible par un
  nuanceur**, il faut la copier sur le GPU.
- L'exécutable actuel de l'application n'importe **aucune DLL NVIDIA** (`dumpbin /dependents`,
  02/10/2026) : il démarre sur une machine sans NVIDIA.

**Non mesuré** :

1. La **latence en millisecondes** à cadence réelle : seul le nombre d'images retenues l'est.
   Elle se relève pendant l'essai (§8).
2. La **taille de l'offre** à trois codecs, une fois comprimée : mesurée par un test de la
   tâche qui touche à la négociation (§7).
3. Le comportement du MFT quand la taille annoncée diffère de celle du flux
   (`MF_E_TRANSFORM_STREAM_CHANGE`) : codé dans la sonde, jamais déclenché. Un test le prouve (§7).
4. Aucun flux 4:4:4 n'est passé par Media Foundation (hors périmètre : le 4:4:4 reste à NVDEC).

**Établi par lecture du source de `str0m` 0.23.1** (pas par un test) : la réponse liste **tous**
les codecs concordants, dans l'ordre de la configuration **de l'hôte** ; l'ordre de l'offre n'y
joue aucun rôle. *Corrigé le 02/10/2026 : c'était la troisième inconnue de la première version.*

## §3 Décisions

- **D1 — 4:2:0 d'abord.** L'empaquetage attend le sous-jalon 2 (choix du propriétaire, 02/10).
- **D2 — Repli H.264 automatique.** Sans décodeur HEVC (extension absente), le spectateur
  reçoit du H.264 4:2:0 (choix du propriétaire, 02/10).
- **D3 — Les capacités voyagent dans l'offre SDP** (approche A, choix du propriétaire, 02/10).
  Le spectateur, qui rédige déjà l'offre (décision D2 du jalon C), y liste ce qu'il sait
  décoder ; l'hôte choisit dans la réponse. **Aucun changement du site ni de son API.** Écartées :
  un message de contrôle après connexion (exige une renégociation SDP en cours de session,
  retarde la première image) ; un champ de l'annuaire (déploiement du site, information
  périmée si un pilote ou l'extension change).
- **D4 — Un seul chemin de décodage par format.** NVDEC pour HEVC 4:4:4 seulement ;
  **Media Foundation pour tout le 4:2:0, y compris sur une carte NVIDIA** (choix du
  propriétaire, 02/10). Moitié moins de combinaisons à tester ; le décodage Media Foundation y
  reste matériel.
- **D5 — `MF_LOW_LATENCY=1`, et jamais de décodage logiciel.** Le réglage est sûr sur le chemin
  matériel et c'est lui qui retient au plus une image (§2). Le défaut mesuré ne touche que le
  décodage logiciel : le moteur **refuse donc toute image rendue en mémoire centrale** (une sortie
  qui n'est pas un `IMFDXGIBuffer` signe un repli logiciel) au lieu de l'afficher. *Corrigé le
  02/10/2026 : la première version disait « pas de `MF_LOW_LATENCY` », sur la foi d'une perte
  mal attribuée.*
- **D6 — Une seule matrice de couleur.** Le nuanceur NV12 applique la même conversion
  **BT.601 pleine plage** que le nuanceur 4:4:4 ; l'encodeur 4:2:0 est configuré pour la même
  signalisation.

## §4 Architecture

### `sky-net` — la négociation

`nouveau_rtc` ne pose plus un catalogue fixe réduit à HEVC 4:4:4 (`link.rs:1033-1055`). Il reçoit
une **liste ordonnée de formats vidéo**, par exemple un type `FormatVideo` à trois valeurs :

| Format | SDP |
|---|---|
| `Hevc444` | H265, `profile-id=4` (comme aujourd'hui) |
| `Hevc420` | H265, `profile-id=1` (Main) |
| `H264` | H264, `packetization-mode=1` |

Chaque format reçoit son propre type de charge et son RTX. Le spectateur y met ce qu'il décode,
l'hôte ce qu'il encode ; `str0m` fait l'intersection. **Les deux côtés calculent le même format
par la même fonction** : le premier, dans l'ordre fixe 4:4:4, HEVC 4:2:0, H.264, dont le type de
charge figure parmi ceux que la négociation a retenus (`Media::remote_pts`). L'hôte écrit sur ce
type de charge au lieu de chercher `Codec::H265` en dur (`link.rs:827`) ; le spectateur ouvre le
décodeur de ce format. Aucun des deux ne dépend de l'ordre de la réponse. Chaque
`LinkEvent::Image` porte aussi le format lu sur le paquet, ce qui permet à un test de prouver que
les deux côtés s'accordent. Quand rien ne concorde, `str0m` écarte la ligne média entière
(mesuré, jalon 2) : le format négocié est alors absent, et l'hôte s'arrête avant d'encoder.

La détection d'image clé à la réception couvre `CodecExtra::H264` en plus de `CodecExtra::H265`.

### `sky-encode` — l'hôte

NVENC sait produire HEVC 4:2:0 et H.264. L'encodeur reçoit le **format négocié** en paramètre ;
les réglages du jalon 2 (rafraîchissement intra, en-têtes de séquence répétés,
`forcer_image_cle`) valent pour les trois formats. L'hôte annonce les formats que la sonde de
capacités de NVENC (`caps.rs`) déclare encodables : **le refus actuel « pas d'encodeur HEVC
4:4:4 » devient « aucun format encodable »**. Une NVIDIA antérieure à Turing peut donc partager
en 4:2:0.

### `sky-decode` — deux moteurs derrière un trait

Le trait `Decodage` de `sky-partage/src/spectateur.rs:160` reste l'interface de la boucle.

- **NVDEC** : inchangé, HEVC 4:4:4.
- **Media Foundation** (nouveau) : HEVC 4:2:0 et H.264, MFT synchrone de Microsoft
  (énumération `SYNCMFT | LOCALMFT | SORTANDFILTER`, §2) sur le **périphérique D3D11 de la
  fenêtre**, confié par `IMFDXGIDeviceManager`. Sortie : une tranche de tableau de textures NV12
  (§2), que l'affichage copie sur le GPU. Comme NVDEC depuis le correctif C1 du jalon 2, il reçoit
  **une unité d'accès entière par appel** et rend l'image de cette unité au même appel.

**Le périphérique D3D11 est créé en un seul endroit**, `sky-decode`, pour la sonde comme pour la
fenêtre : même règle de choix d'adaptateur (NVIDIA d'abord, sinon l'adaptateur par défaut), avec
`D3D11_CREATE_DEVICE_VIDEO_SUPPORT` et la protection multi-fil qu'exige le gestionnaire de Media
Foundation. Deux règles de choix d'adaptateur divergeraient : la sonde validerait une carte et
la fenêtre en ouvrirait une autre.

**« Matériel » se prouve, il ne se suppose pas.** Un format 4:2:0 n'est déclaré décodable que si
trois conditions tiennent : le profil DXVA accepte 2560×1440 sur l'adaptateur
(`ID3D11VideoDevice`), un MFT est énuméré pour le codec, et il accepte le gestionnaire D3D11.
Puis, en cours de flux, toute sortie hors GPU est refusée (D5).

Les erreurs : `ErreurDecodeur` gagne **`AucunDecodeur`** (aucun format décodable) et
**`MediaFoundation(String)`** (un appel Media Foundation a échoué, ou une image est sortie en
mémoire centrale). La taille annoncée est éprouvée par la question DXVA, mais un refus y devient
`MediaFoundation` et non `ResolutionTropGrande` : DXVA répond oui ou non pour une taille, sans
dire son maximum, que `ResolutionTropGrande` exige.

### `sky-rendu` — deux surfaces, deux nuanceurs

`ImageAAfficher` rend l'une de deux sources : trois plans CUDA 4:4:4 (aujourd'hui), ou une
**tranche de texture D3D11 NV12**. Pour la seconde, la fenêtre copie la tranche (à la taille
d'affichage, ce qui écarte les lignes de remplissage, cf. 1080 contre 1088 au jalon 2) dans sa
propre texture NV12 lisible par nuanceur, puis un second nuanceur convertit NV12 en RGB (D6). Le
chemin NV12 n'utilise pas l'interopérabilité CUDA.

## §5 Déroulé d'une connexion

1. **Le spectateur sonde** avant d'offrir : NVDEC pour HEVC 4:4:4 (`sonder_materiel`) ; pour
   HEVC 4:2:0 et H.264, il **crée réellement** un décodeur Media Foundation matériel. La seule
   présence de l'extension ne vaut pas preuve.
2. Rien ne répond : **refus avant tout réseau** (`AucunDecodeur`), comme le partage sans
   encodeur.
3. **L'offre** liste les formats sondés, ordre 4:4:4, HEVC 4:2:0, H.264 ; elle est comprimée
   puis scellée comme aujourd'hui.
4. **L'hôte choisit** le premier format retenu qu'il sait encoder, dans le même ordre, et
   configure NVENC. Le format choisi remonte à l'interface, qui l'affiche parmi les mesures.
5. **Le spectateur** ouvre le décodeur du format retenu. **La garde est inchangée** : jamais
   d'affichage avant une image clé, plus du tout après un trou (`spectateur.rs`, à ne pas
   simplifier).

## §6 Textes et erreurs visibles

Nouveaux textes, **soumis à la validation du propriétaire** comme les six du jalon 2 :

- spectateur sans aucun décodeur matériel (`AucunDecodeur`) ;
- hôte sans aucun format encodable (remplace le refus « sans HEVC 4:4:4 ») ;
- aucun format commun (`CodecNonNegocie`, aujourd'hui « aucun codec vidéo commun avec le
  correspondant ») ;
- l'étiquette du format dans les mesures (« HEVC 4:4:4 », « HEVC 4:2:0 », « H.264 »).

Les trois refus nouveaux passent par `FinVue::Autre { message }`, le texte étant rédigé par le
cœur comme les refus de l'hôte d'aujourd'hui : aucune cause nouvelle côté interface.

**Textes devenus faux, à réécrire dans ce sous-jalon** (un fait corrigé qui survit ailleurs est
la classe de défaut la plus fréquente du jalon 2) : « SkyShare ne peut ni partager son écran ni
en recevoir un sur cette machine » (absence de NVIDIA, `sky-encode/src/caps.rs` et
`app/src/messages.ts`), et le refus « sans décodage 4:4:4 » du spectateur, qui ne survient plus
à l'ouverture. Ainsi que le paragraphe « Pas de repli logiciel » de `CLAUDE.md`.

## §7 Tests

*Corrigé le 02/10/2026 : la première version ouvrait le plan par une sonde dédiée aux trois
inconnues du §2. Deux sont levées (l'ordre de `str0m` par lecture du source, la latence en images
par les rapports) ; les deux autres (taille de l'offre, changement de taille en cours de flux)
se prouvent par les tests ci-dessous, dans la tâche qui touche le code concerné. La sonde
séparée n'a plus d'objet.*

**Tests automatiques**, chacun prouvé par neutralisation (une garde retirée à la fois, le bon
test rougit seul, pour la bonne raison) :

- **Négociation en boucle locale** : un spectateur H.264 seul obtient H.264 ; un spectateur
  complet face à un hôte complet obtient HEVC 4:4:4 ; des listes disjointes donnent
  `CodecNonNegocie`.
- **Décodage Media Foundation** : des flux NVENC réels HEVC 4:2:0 et H.264 ; chaque unité
  poussée rend sa propre image au même appel ; un flux 1920×1080 rend des images de 1080 lignes
  (pas 1088) ; un flux plus petit que la taille annoncée est décodé à sa vraie taille ; le
  **contenu** de chaque image est celui de son unité (deux motifs alternés, comme le test NVDEC
  du jalon 2). La comparaison à l'octet avec le décodage logiciel, faite par la sonde, n'est pas
  reprise : elle exigerait un chemin logiciel dans le moteur, que D5 interdit.
- **Garde d'image clé** : les tests existants rejoués en H.264.
- **Nuanceur NV12** : test de couleur sur une image connue, sur le modèle du test 4:4:4.
- **Doublures** : elles reproduisent le comportement réel du décodeur Media Foundation, et un
  test le vérifie (leçon du C1 du jalon 2 : une doublure a masqué un décalage d'une image).
- Taille de l'offre comprimée sous 4096 octets, vérifiée par un test.

## §8 Critère de fin

1. Tests Rust et d'interface au vert, clippy et tsc propres, revue finale de branche.
2. **Essai à deux machines** : la machine NVIDIA du propriétaire partage, **le portable AMD
   regarde** en 4:2:0. Le portable n'a pas de dossier de développement : le plan livre une
   **version portable** (exécutable et fichiers, sans Rust) et une **fiche d'essai** sur le
   modèle de `spike/docs/essai-jalon-2.md`. La latence en millisecondes (§2) s'y relève.

## §9 Risques connus, acceptés

- **Windows « N » sans Media Feature Pack.** `mfplat.dll` devient une dépendance de chargement :
  sur une telle édition, l'application **ne démarrerait plus du tout**, même pour partager. Non
  vérifié (aucune machine N). Le remède, un chargement différé, est reporté.
- **HEVC exige l'extension du Store** (HEVCVideoExtension). Sans elle, le repli H.264 (D2)
  prend le relais ; c'est sa raison d'être.
- **Une seule puce non NVIDIA éprouvée** (Vega 8 du Ryzen 7 5700U). Aucune puce Intel.
- **Un flux plus grand que la taille annoncée** (2560×1440) n'est pas éprouvé contre la limite
  DXVA en cours de flux : la question DXVA porte sur la taille annoncée.
