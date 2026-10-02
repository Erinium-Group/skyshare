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
- **Avec `MF_LOW_LATENCY=1`, le décodeur Media Foundation perd silencieusement toutes les images
  après la 120e** sur un flux en rafraîchissement intra. Constaté sur la machine NVIDIA et sur le
  portable AMD.

**Non mesuré, et que la tâche 1 mesure** (§7) :

1. La **latence** de Media Foundation **sans** `MF_LOW_LATENCY` : combien d'images il retient
   entre l'entrée et la sortie, sur notre flux.
2. La **taille de l'offre** à trois codecs, une fois comprimée, contre la limite de 4096 octets
   de l'enveloppe.
3. L'**ordre** que `str0m` 0.23 donne à sa réponse quand plusieurs codecs concordent.

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
- **D5 — Pas de `MF_LOW_LATENCY`.** Il perd les images (§2). Si la latence sans lui se révèle
  inacceptable (tâche 1), le repli envisagé est de passer le 4:2:0 en **images clés
  périodiques** ; ce repli est soumis au propriétaire **avant** d'écrire la suite du plan.
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
l'hôte ce qu'il encode ; `str0m` fait l'intersection. Après négociation, l'hôte lit **le format
retenu**, au lieu de chercher `Codec::H265` en dur (`link.rs:827`). **Le choix ne dépend que de
l'ordre de préférence de l'hôte parmi les formats retenus**, jamais de l'ordre de la réponse
(non vérifié, §2). `ErreurEnvoi::CodecNonNegocie` reste le refus quand rien ne concorde.

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
- **Media Foundation** (nouveau) : HEVC 4:2:0 et H.264, décodeur **matériel** sur le
  **périphérique D3D11 de la fenêtre**, confié par `IMFDXGIDeviceManager`. Sortie : une texture
  D3D11 **NV12**, sans copie. Comme NVDEC depuis le correctif C1 du jalon 2, il reçoit **une
  unité d'accès entière par appel**.

Les erreurs : `ErreurDecodeur` gagne **`AucunDecodeur`** (ni NVDEC, ni Media Foundation
matériel). `ResolutionTropGrande` s'applique aussi à Media Foundation, éprouvée à la création du
décodeur.

### `sky-rendu` — deux surfaces, deux nuanceurs

`ImageAAfficher` rend l'une de deux surfaces : trois plans CUDA 4:4:4 (aujourd'hui), ou une
**texture D3D11 NV12**. Un second nuanceur convertit NV12 en RGB (D6). Le chemin NV12 n'utilise
pas l'interopérabilité CUDA.

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

## §7 Tests

**Tâche 1, une sonde avant tout code durable**, sur le portable AMD et sur la machine NVIDIA :
les trois inconnues du §2. Le rapport est versé dans `spike/mesures/`. Une latence inacceptable
déclenche D5 et un retour au propriétaire.

**Tests automatiques**, chacun prouvé par neutralisation (une garde retirée à la fois, le bon
test rougit seul, pour la bonne raison) :

- **Négociation en boucle locale** : un spectateur H.264 seul obtient H.264 ; un spectateur
  complet face à un hôte complet obtient HEVC 4:4:4 ; des listes disjointes donnent
  `CodecNonNegocie`.
- **Décodage Media Foundation** : des flux NVENC réels HEVC 4:2:0 et H.264 décodés à
  l'identique d'une référence logicielle.
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
   modèle de `spike/docs/essai-jalon-2.md`.
