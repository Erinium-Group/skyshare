# Deux sondes avant le jalon 2 : le décodage fonctionne, et les pistes média de str0m conviennent

**Date : 27/09/2026.** Deux sondes de faisabilité lancées avant d'écrire la spec du jalon 2,
parce que deux des trois inconnues qui la cadraient ne reposaient sur **aucune mesure** : le
projet n'avait jamais décodé une seule image, et n'avait jamais écrit une ligne de piste média.

> **Avertissement sur cette note.** Les deux sondes ont écrit des rapports détaillés dans le
> répertoire temporaire de session, qui a été nettoyé avant qu'ils ne soient portés ici. Les
> **chiffres** ci-dessous sont ceux rapportés par les sondes et sont fidèles ; en revanche les
> **sorties de commande brutes** et les **PNG décodés** de la sonde A sont perdus. Un chiffre
> de cette note n'est donc pas reproductible en l'état : il est attesté par la sonde, pas
> rejouable. Refaire la mesure est bon marché (les deux sondes étaient du code jetable, hors
> du dépôt) et devrait être fait au moment d'implémenter, ce qui la revérifiera au passage.
> Leçon associée dans `tasks/lessons.md`.

---

## Sonde A — NVDEC décode notre HEVC 4:4:4

**Question** : le décodeur matériel NVIDIA sait-il décoder le HEVC 4:4:4 produit par notre
propre NVENC, et à quel coût ? C'était la condition sine qua non du « premier pixel ». La
réponse disponible jusque-là était documentaire — un tableau dans
`2026-08-23-compatibilite-toutes-cartes-graphiques.md` disant « ✅ à partir de Turing » —
et cette même note admettait que le jalon 0 **n'avait jamais testé le décodage**, puisqu'il
écrivait un fichier relu par un lecteur externe.

**Entrée** : `spike/cmp-hevc-444.h265`, flux élémentaire HEVC 4:4:4 2560×1440 produit par le
NVENC du projet au jalon 0. **Machine** : Windows 11, RTX 4060.

**Route prise** : Rust appelant `nvcuvid.dll` directement via `libloading`, comme `sky-encode`
le fait pour NVENC. Pas de ffmpeg — il n'est pas installé sur la machine, et il n'aurait pas
été la bonne route (voir « la fausse preuve écartée » plus bas).

### Ce qui est mesuré

| Mesure | Valeur |
|---|---|
| Images décodées | **901 sur 901**, aucun refus du parseur |
| Débit de décodage | **569 im/s** (trois passes : 562 / 569 / 584) ; 575 im/s sur 10 812 images en 18,8 s |
| Latence de décodage | **médiane 1,57 ms**, p99 3,39 ms, min 1,47 ms |
| CPU | **8,2 % d'un cœur** en régime (143 µs par image) |
| Occupation du décodeur | **~89 %** (`nvidia-smi dmon`, bruit de fond 4–7 %) |
| Justesse contre la référence du jalon 0 | **89,78 dB, 99,979 % de pixels identiques** (contre `spike/mesures/frame120-hevc-444.png`) |

### Le 4:4:4 est réellement préservé — preuve discriminante

Ce projet s'est déjà fait piéger par des preuves qui passaient aussi bien dans le cas
négatif. Celle-ci tient par trois angles indépendants :

1. Le `nOutputFormatMask` annoncé par NVDEC pour ce flux **n'offre même pas NV12** : le
   décodeur ne *peut pas* dégrader vers du 4:2:0.
2. Les surfaces rendues sont en `YUV444`, 2560×1440.
3. Test discriminant : un aller-retour 4:2:0 forcé sur les plans obtenus **perd 19–20 dB et
   altère 14 % des échantillons**. L'information de chrominance pleine résolution était donc
   bien présente — ce qui ne serait pas le cas si le décodeur avait converti en amont.

### Deux trouvailles qui changent la conception

- **Le flux est en BT.601 pleine échelle, pas BT.709.** Les six matrices de conversion ont
  été essayées ; BT.709 plafonne à 36 dB contre 89,78 dB pour BT.601 pleine échelle. Sans
  cette information, le premier pixel affiché aurait de fausses couleurs et on aurait cherché
  la cause ailleurs. **À écrire dans le code de rendu, avec ce chiffre en commentaire.**
- **Un seul IDR pour 901 images.** L'écart 3 se confirme depuis l'autre bout de la chaîne,
  côté décodeur cette fois, et non plus seulement par lecture des drapeaux NVENC.

### La fausse preuve écartée

Contrôle croisé avec mpv : `--hwdec=nvdec` et `--hwdec=cuda` **se replient silencieusement
sur le décodage logiciel** sur ce flux. Un `ffmpeg` ou un lecteur qui « décode » ne prouve
donc rien du matériel. La sonde, elle, appelle `nvcuvid.dll` et n'a **aucun repli possible** :
si elle décode, c'est NVDEC.

### Non mesuré

- Le décodage **pendant** un encodage NVENC (un seul moteur NVDEC physique). Sonde courte,
  à faire.
- La **latence jusqu'au pixel affiché**. C'est le vrai risque résiduel : une image décodée
  pèse **11,06 Mio**, donc le chemin décodeur → écran doit rester sur le GPU.
- Un flux abîmé ; le 4:4:4 10 bits ; l'occupation du décodeur à cadence nominale (la règle
  de trois donnant ~25 % à 165 im/s est une extrapolation, pas une mesure).

---

## Sonde B — les pistes média de str0m transportent notre HEVC, et le goulot SCTP disparaît

**Question** : peut-on transporter la vidéo par une **piste média** WebRTC de `str0m` 0.23.1
plutôt que par le **canal de données** actuel, et à quel coût comparé à un passage à
`webrtc-rs` ? Le contexte est l'écart 7, mesuré au jalon 0 sur deux machines et deux réseaux :
RTT **115,7 ms médian / 643 ms p99** (attendu 15–30 ms sur une liaison fibre-fibre) et
**16 % d'échecs d'envoi** (2611 / 16349).

### HEVC n'est pas le point de rupture — constat de code

C'était le point de rupture supposé. Il ne l'est pas. Source : les sources de `str0m` 0.23.1
dans le cache Cargo local.

- `src/format/codec.rs:60` : `Codec::H265` existe (marqué `#[doc(hidden)]` avec un
  `// TODO show this when we support h265`), et `Codec::is_video()` l'inclut.
- `src/packet/h265.rs` : **1300+ lignes**. `H265Packetizer` et `H265Depacketizer` sont réels,
  pas des souches. `src/packet/mod.rs:323` les câble dans le dispatch.
- `src/format/codec_config.rs:84` : **`c.enable_h265(true)` dans `new_with_defaults()`** —
  HEVC est actif par défaut (payload type 102, RTX 103).
- `tests/keyframes.rs:382` : str0m teste sa chaîne HEVC sur une vraie capture
  `tests/data/h265.pcap`.
- **Format d'entrée attendu** (`src/packet/h265.rs:541-600`) : un flux **Annex-B**, que le
  paquetiseur découpe sur les codes de départ. C'est exactement ce que produit NVENC.
  **Aucune conversion de format n'est nécessaire, et aucune piste RTP « brute ».**

### Le pacer de str0m est désactivé par défaut — et c'est ce qui tranche le choix

`src/session.rs:163-171` : sans `RtcConfig::enable_bwe(Some(..))`, str0m installe un
**`NullPacer`** — il n'espace rien, n'estime rien, ne décide d'aucun débit. Le projet garde
donc **intégralement son propre contrôle de congestion** sur une piste média, ce qui est
exactement la décision d'architecture déjà arrêtée (« WebRTC via str0m, contrôle de
congestion réécrit »). **Rien à contourner, rien à désactiver, rien à forker.**

### Ce qui est mesuré : zéro refus d'écriture

Sonde autonome hors du workspace, deux instances `Rtc` sur deux sockets UDP `127.0.0.1`,
négociation SDP complète (offre et réponse sérialisées puis reparsées), piste vidéo
`Direction::SendOnly`, **aucun canal de données créé**.

| Charge offerte | Écritures tentées | Refusées | Taux |
|---|---|---|---|
| 12,17 Mbps (l'ordre de grandeur du jalon 0) | 2593 | **0** | **0,000 %** |
| 99,78 Mbps (8× la charge visée) | 21552 | **0** | **0,000 %** |

À comparer aux **16 %** du canal de données.

**Pourquoi la comparaison est légitime malgré la boucle locale.** Les 16 % ne viennent pas
d'une perte réseau : ils viennent de `Channel::write` renvoyant `false` quand le **tampon
d'émission SCTP** est plein (`sky-net/src/link.rs:585` propage l'échec, `hote.rs` compte un
`echecs_send` et laisse tomber le morceau). C'est une **contre-pression propre à SCTP**. Sur
une piste média, le seul refus possible est `RtcError::WriteWithoutPoll`
(`src/media/mod.rs:464`), déclenché uniquement au-delà de **100 images** en attente de
paquetisation — donc jamais si l'appelant appelle `poll_output` entre deux écritures. Le
mécanisme qui produit les 16 % **n'existe pas sur ce chemin** : le zéro est structurel, pas
circonstanciel, et il est confirmé à 8× la charge visée.

**Ce que le zéro ne dit pas** : il ne dit pas qu'aucune donnée n'est perdue. Il dit que
l'émetteur n'est plus jamais bloqué. La perte se déplace du refus d'écriture vers la perte de
paquets réseau — ce qu'on veut pour du temps réel, mais qui devient la responsabilité du
contrôle de congestion et non de l'API.

**Intégrité vérifiée** : **61 NAL émis, 61 NAL reçus, identiques octet pour octet.** Payload
type 102 négocié, `a=rtpmap:…H265` présent dans l'offre **et** retenu dans la réponse. str0m
signale même les images clés HEVC (`CodecExtra::H265 { is_keyframe }`).

### Ce qui n'est PAS mesuré, et ne doit pas être surinterprété

- **Le RTT.** Les médianes relevées (0,2 à 0,8 ms) mesurent la pile réseau du noyau sur
  `127.0.0.1`, **pas un chemin Internet. Sans aucune valeur comparative avec les 115,7 ms.**
  → **L'écart 7 n'est PAS déclaré résolu.** Ce qui est établi, c'est que sa cause supposée
  disparaît. La confirmation **exige un essai à deux machines sur deux réseaux**, le même
  dispositif que celui qui a produit les 115,7 ms.
- **La perte d'octets observée dans la sonde** (95,6 % reçus à 12 Mbps, 67,1 % à 100 Mbps)
  est un **artefact de la sonde** : un seul fil d'exécution sert les deux instances et ne vide
  les sockets qu'à son tour de boucle, donc le tampon de réception du noyau déborde. **Ce
  n'est pas une mesure du transport.**
- Le profil HEVC annoncé dans le `fmtp` : la sonde a utilisé le défaut (Main / Main tier /
  niveau 6.0, `profile_id = 1`) alors que le projet encode en **Main 4:4:4**. La paquetisation
  a fonctionné quand même parce qu'elle ne lit pas le contenu du NAL — mais l'annonce SDP doit
  être honnête : appeler `add_h265(pt, rtx, profile_id, tier_flag, level_id)` avec la bonne
  valeur. Un correspondant refusant le profil le refuserait à la négociation, pas au transport.
- Perte de paquets réelle et effet de NACK/RTX ; justesse de `str0m::bwe` sur un goulot réel ;
  coût CPU de la paquetisation RFC 7798.

### `str0m::bwe` — mesuré, utilisable, et tout ou rien

Activé par `enable_bwe(Some(Bitrate::bps(12_000_000)))` : **45 estimations en 20 s** (une
toutes ~440 ms), de 0,19 à 28,89 Mbps, **moyenne 12,53 Mbps** pour 12,17 offerts — il converge.
Effet observable sur le fil : **12,04 Mbps émis avec BWE contre 13,49 sans**, à charge offerte
identique ; le `LeakyBucketPacer` espace donc réellement.

Il **remplacerait** le `Pacer` maison (`sky-net/src/pacer.rs`, 196 lignes) plutôt que de le
compléter — les deux estiment la même chose depuis la même information — et c'est **tout ou
rien** : `enable_bwe` installe aussi le `LeakyBucketPacer`, donc rend l'espacement à str0m.
L'API est volontairement étroite : `set_desired_bitrate` et `reset`, sortie par
`Event::EgressBitrateEstimate` et `Event::Probe`, **aucun réglage interne du GCC**.

**Recommandation : commencer sans BWE** (le `NullPacer` préserve la décision d'architecture et
le `Pacer` maison déjà écrit et testé), et garder `enable_bwe` comme point de comparaison
mesurable lors de l'essai réel.

### Coût d'intégration — compté sur le code, pas estimé

**Rester sur str0m : de l'ordre de 400 à 450 lignes touchées, dont ~140 purement supprimées.**

| Fichier | Ce qui change | Lignes |
|---|---|---|
| `sky-net/src/link.rs` (1036 l.) | `add_channel_with_config` → `add_media` ; `Mid` récupéré par `Event::MediaAdded` ; `Event::ChannelOpen/Data/Close` → `MediaAdded/MediaData` ; `send()` → `ecrire_image()` ; `LinkEvent::Data` → `LinkEvent::Image` — 35 lignes portent une référence au canal de données, sur ~6 blocs | ~120 réécrites |
| `sky-partage/src/hote.rs` (589 l.) | le découpage en morceaux se réduit à un `write` de l'unité d'accès entière ; **`envoyer_ou_abandonner` (53 l.) disparaît** ; `EN_TETE_MORCEAU` et `TAILLE_MORCEAU_PAYLOAD` disparaissent | ~90 supprimées, ~25 ajoutées |
| `sky-partage/src/reception.rs` (137 l.) | l'horodatage maison sur 8 octets et le drapeau « premier morceau » cèdent la place à l'horodatage RTP et au bit marqueur, fournis par str0m | ~110 réécrites |
| `spectateur.rs`, `evenement.rs`, `etablissement.rs` | plomberie de statistiques et types d'événement | ~40 |
| `sky-net/src/pacer.rs` (196 l.) | **inchangé** si l'on reste sur `NullPacer` ; son budget cesse seulement de faire sauter des images pour piloter le débit NVENC | ~10 |
| Tests | ceux de `link.rs` portant sur `ChannelOpen`, ceux de `reception.rs` | ~80 |

Le découpage, le réassemblage, la contre-pression et l'horodatage maison sont du code que le
projet **cesse d'écrire**. **Ne changent pas du tout** : `handshake.rs`, `stun.rs`,
`rendez_vous.rs`, `sky-crypto` (les enveloppes portent du SDP, pas du transport), `sky-compte`,
toute l'API du site. La forme de la boucle `handle_input` / `poll_output` / `Output::Transmit`
est **identique** — c'est le même `Rtc`.

**Passer à `webrtc-rs` : 1200 à 3500 lignes.** Lecture de l'API sans installation (version
0.21.0 publiée le **19/09/2026**, soit huit jours avant la sonde) :

- HEVC : `rtp::codecs::h265` existe aussi. **Pas un différenciateur.**
- Le crate `webrtc` est **async** (tokio / smol), alors que `sky-net`, `sky-partage` et
  `sky-app` sont **entièrement synchrones** (`std::net::UdpSocket`, fils et boucles
  bloquantes). Passer par lui imposerait un runtime async jusque dans `sky-app` :
  **2500 à 3500 lignes**.
- Le crate `rtc` sans-I/O 0.21 a une forme proche de str0m (`poll_write` / `poll_event` /
  `handle_read`), donc un coût de forme comparable — mais il resterait à réapprendre une API
  entière, refaire la négociation ICE/DTLS, **revalider la garantie « aucune adresse
  journalisée »** sur une bibliothèque dont on ne connaît pas le comportement de traçage, et
  refaire les tests qui s'appuient sur les types de str0m : **1200 à 1800 lignes**, pour un
  gain nul sur la question posée.
- Son pacing : la documentation lue **ne dit pas s'il est désactivable**. Étiquette honnête :
  **non établi** — ce n'est pas une preuve d'absence, les sources n'ont pas été lues. Mais cela
  suffit à ne pas en faire le choix par défaut, quand str0m le laisse vérifiablement au projet.

---

## Ce que ces deux sondes décident, et ce qu'elles ne décident pas

**Décidé, sur mesure :**

1. Le décodage matériel du HEVC 4:4:4 fonctionne, en 1,57 ms par image, avec le 4:4:4 préservé
   — prouvé de trois façons dont une discriminante.
2. Le transport passe aux **pistes média de `str0m` 0.23.1**. Le mécanisme des 16 % d'échecs
   disparaît structurellement, HEVC est accepté sans conversion, le contrôle de congestion
   reste au projet, et le coût est de 400-450 lignes contre 1200-3500 pour l'alternative.
3. Le rendu doit convertir en **BT.601 pleine échelle**.

**Non décidé, et honnêtement ouvert :**

- **L'écart 7 reste ouvert** jusqu'à un essai à deux machines sur deux réseaux. Aucune mesure
  de boucle locale ne le clôt.
- Le sort du `Pacer` maison contre `str0m::bwe` — à mesurer sur cet essai réel, les deux étant
  mutuellement exclusifs en pratique.
- La latence décodeur → pixel affiché, qui est le vrai risque restant du « premier pixel ».
- Le décodage simultané d'un encodage sur un même moteur NVDEC.
