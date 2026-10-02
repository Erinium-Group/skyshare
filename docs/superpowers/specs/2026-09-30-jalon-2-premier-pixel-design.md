# Jalon 2 — le premier pixel — conception

**Date : 30/09/2026.** Spec du jalon 2 de SkyShare : le spectateur voit enfin l'écran de l'hôte.

Mesures qui fondent ce document :
`docs/superpowers/notes/2026-09-27-sondes-jalon-2-decodage-et-transport.md`.
Architecture générale : `docs/superpowers/specs/2026-08-22-skyshare-architecture-design.md`.

> **Corrections du 02/10/2026 (tâche 12 du jalon), après implémentation.** Neuf affirmations de
> cette spec se sont révélées fausses pendant le jalon. Elles sont corrigées **sur place**, chacune
> signalée par une mention « *Corrigé le 02/10/2026* » qui dit ce qui était écrit et d'où vient la
> correction : (1) le chiffre « 19–20 dB », §2 et §9 ; (2) le PSNR de référence, §2 et §9 ; (3) le
> réglage du canal de données, §3 D2 ; (4) `LinkEvent`, §5 ; (5) les variantes d'`ErreurDecodeur`,
> §5 ; (6) la signature de `Decodeur::nouveau`, §5 ; (7) celles de `Fenetre`, §5 ; (8) le nom de
> l'encodeur, §5 ; (9) ce que voit un spectateur sans image clé, §6. La source de chaque correction est le journal du jalon
> (`.superpowers/sdd/2026-09-30-jalon-2-premier-pixel/progress.md`, **ignoré par git**) ou le
> code lui-même ; elle est recopiée ici pour survivre à la fusion.

---

## §1 Ce que ce jalon livre, et ce qu'il ne livre pas

**Il livre** : un spectateur voit l'écran d'un hôte, en HEVC 4:4:4, dans une fenêtre, sur un
transport fait pour la vidéo. Un spectateur, un écran, sans son.

**Entrent dans le périmètre :**

1. Le transport passe du canal de données aux **pistes média** de `str0m`.
2. Le **décodage** matériel, dans un crate neuf `sky-decode`.
3. L'**affichage**, dans un crate neuf `sky-rendu` : fenêtre native, Direct3D 11.
4. L'**écart 3** — les en-têtes de séquence, sans lesquels une perte de paquets condamne
   définitivement la session.

**Sortent du périmètre, avec la raison :**

- **L'écart 5** (reconfiguration du débit à chaud par `nvEncReconfigureEncoder`). Il améliore la
  qualité sous congestion, il ne conditionne pas le premier pixel, et il se mesure mal sans
  l'essai à deux machines. → jalon 3, qui touche la même API pour le simulcast.
- **Le son** (décision 6 du cadrage : son du partage en Opus). Capture WASAPI, encodage,
  deuxième piste, synchronisation : une chaîne entière, qui doublerait ce jalon.
- **La voie A de la décision D1 du 23/08** (empaquetage de la chrominance dans un porteur 4:2:0
  pour les machines non-NVIDIA). Ses neuf inconnues demandent du matériel AMD et Intel absent à
  ce jour. Le jalon 2 **assume la limite et la dit clairement** (§7) au lieu de la contourner.

---

## §2 Ce qui est déjà mesuré, et sur quoi ce jalon ne revient pas

| Fait | Mesure |
|---|---|
| NVDEC décode notre HEVC 4:4:4 | 901 images sur 901, **569 im/s**, latence **1,57 ms** médiane (p99 3,39 ms), 8,2 % d'un cœur |
| Le 4:4:4 survit au décodage | **89,78 dB** (sonde), 99,979 % de pixels identiques contre `spike/mesures/frame120-hevc-444.png` ; **85,50 dB** par le test de `sky-decode` (voir la correction ci-dessous) |
| L'espace colorimétrique du flux | **BT.601 pleine échelle** — BT.709 plafonne à 36 dB |
| HEVC sur une piste média `str0m` | 61 NAL émis, 61 reçus, identiques octet pour octet ; payload type 102 négocié |
| Le goulot d'envoi disparaît | **0 refus** sur 2593 écritures à 12 Mbps et sur 21552 à 100 Mbps, contre **16 %** aujourd'hui |
| `str0m` laisse le contrôle de congestion au projet | sans `enable_bwe`, `session.rs:171` installe un `NullPacer` |

> *Corrigé le 02/10/2026 — deux chiffres de ce tableau.*
>
> 1. **« Un aller-retour 4:2:0 forcé perd 19–20 dB » était faux.** 19 à 20 dB n'est pas un
>    écart, c'est un **plancher absolu** : celui qu'atteignent les codecs 4:2:0 du jalon 0 sur le
>    motif de test (`spike/mesures/psnr-h264-420.log`, `psnr_avg` de 20,45 à 20,46 par image, U à
>    18,2 dB, V à 19,26 dB ; `psnr-av1-420.log`, 20,42). La seule mesure d'aller-retour 4:2:0 du
>    dépôt, la neutralisation du test de `sky-decode` (tâche 2), donne **15,06 dB absolus**, soit
>    une chute de 70,4 dB depuis 85,50 dB. Établi par la revue de la tâche 2.
> 2. **Le PSNR de référence existe désormais par deux chemins.** La sonde mesurait 89,78 dB ; le
>    test de `sky-decode` mesure **85,50 dB**, avec un écart maximal de **1 niveau** sur 2 028
>    composantes parmi 11 059 200. Les deux mesurent la même propriété par deux conversions
>    différentes (virgule fixe pour la référence du jalon 0, `f64` dans le test) : l'écart est
>    celui d'un arrondi, pas une dégradation. Ce qui fait le pouvoir discriminant du test est la
>    distance au cas faux — 85,50 contre **36,13 dB** sous une matrice BT.709 — et le seuil reste
>    à 80 dB.

**Ce qui reste non mesuré et n'est donc affirmé nulle part dans cette spec** : le RTT sur un lien
réel (la sonde n'a fait que de la boucle locale, sans valeur comparative avec les 115,7 ms) ; la
latence décodeur → pixel affiché ; le décodage simultané d'un encodage sur un même moteur NVDEC.

---

## §3 Décisions

**D1 — Le transport passe aux pistes média de `str0m` 0.23.1. Pas de passage à `webrtc-rs`.**
HEVC y est actif par défaut (`enable_h265(true)` dans `CodecConfig::new_with_defaults()`), son
paquetiseur consomme de l'**Annex-B**, exactement ce que produit NVENC — aucune conversion, et
aucune piste RTP « brute ». Le coût est de 300 à 350 lignes touchées, contre 1200 à 3500 pour
`webrtc-rs`, dont le crate principal est **async** alors que tout le projet est synchrone.
(La note des sondes chiffre 400 à 450 lignes : elle supposait le **retrait complet** du canal de
données. D2 le conserve pour le contrôle, donc on en retire moins.)
Argument décisif : `str0m` est le seul des deux dont on ait **vérifié dans le code** qu'il laisse
le contrôle de congestion au projet.

**D2 — Le canal de données est conservé, pour le contrôle uniquement.**
Il n'est mauvais qu'au transport vidéo, ce pour quoi il n'a jamais été conçu. Pour des messages
rares et petits — demande d'image clé, fin de partage — c'est son usage nominal, et il est déjà
écrit et testé. L'alternative (dépendre du retour RTCP de `str0m` pour les demandes d'image clé)
n'a pas été vérifiée dans son code et ne sera pas supposée.

*Corrigé le 02/10/2026 — le réglage du canal a changé, et cette décision ne le disait pas.* Le
canal est désormais **fiable et ordonné** (`Reliability::Reliable`, `sky-net/src/link.rs:318`),
et non plus non ordonné à durée de vie de 150 ms comme au jalon 0. Ce réglage-là avait été
choisi **pour la vidéo** ; la vidéo ayant quitté le canal, il ne protégeait plus rien et coûtait
une perte silencieuse : un `PartageArrete` perdu n'aurait **jamais été réémis**, laissant le
spectateur devant un flux figé. Question soulevée par la revue de la tâche 5, que ni cette spec
ni le plan ne posaient. Coût : la latence d'une retransmission, **non mesurée**, sur des messages
émis au plus une fois par seconde.

**D3 — Le `Pacer` maison est conservé ; `str0m::bwe` reste désactivé.**
`enable_bwe` installe **aussi** le `LeakyBucketPacer` : c'est tout ou rien, donc un choix
d'architecture et non un réglage. On garde le `NullPacer` et les 196 lignes déjà écrites et
testées. `enable_bwe` reste un point de comparaison à mesurer lors de l'essai réel.

**D4 — Une fenêtre native séparée, pas d'incrustation dans la vue web.**
Contrainte arithmétique, pas préférence : une image décodée 4:4:4 en 2560×1440 fait
**11 059 200 octets**, soit **663 Mo/s** à 60 im/s et **1,18 Go/s** aux 107 im/s mesurées au
jalon 0. Aucun pont d'IPC vers une vue web ne tient ce débit. Une fenêtre enfant du `HWND` de
Tauri a été pesée : elle fait cohabiter deux systèmes de rendu dans une même hiérarchie de
fenêtres (redimensionnement, ordre de recouvrement, plein écran à arbitrer) sans rien acheter
pour un premier pixel. Fenêtre indépendante, titrée, ouverte par `regarder`, fermée à l'arrêt.

**D5 — Le chemin reste sur le GPU de bout en bout.**
NVDEC rend ses surfaces en mémoire CUDA ; la fenêtre affiche une texture Direct3D 11. Les deux
vivent sur la même carte : la texture est enregistrée **une fois** auprès de CUDA, puis chaque
image y est copiée de périphérique à périphérique, sans passer par la mémoire centrale.
Conséquence d'API : `ImageDecodee` **ne porte pas de pixels**. Dès qu'une API expose un
`Vec<u8>` d'image, quelqu'un l'utilisera et paiera 663 Mo/s de copie.

**D6 — La conversion est en BT.601 pleine échelle, et le code dit pourquoi avec le chiffre.**
89,78 dB contre 36 dB pour BT.709 sur la référence du jalon 0. Sans cette phrase en commentaire,
quelqu'un « corrigera » vers BT.709, parce que c'est ce que tout le monde attend d'un flux HD.

**D7 — Latence prioritaire sur l'absence de déchirement.**
Chaîne d'échange DXGI `FLIP_DISCARD` à deux tampons, présentation **sans attente de
synchronisation verticale**. On regarde quelqu'un travailler, pas un film. Le déchirement est
assumé et écrit dans les limites. Réglage à remesurer à l'essai réel, pas une vérité.

**D8 — Le décodage ne rend jamais de pixels dégradés en silence.**
Un décodeur incapable de 4:4:4 doit **refuser**, pas convertir. La sonde a montré que mpv, avec
`--hwdec=nvdec`, se replie silencieusement sur le décodage logiciel : un repli invisible
détruirait l'argument qualité du projet sans que personne ne s'en aperçoive.

---

## §4 Architecture et flux de données

```
HÔTE                                              SPECTATEUR
sky-capture  (WGC, texture D3D11)
     |
sky-encode   (NVENC, HEVC 4:4:4, Annex-B)
     |  unité d'accès + horodatage
sky-net      PeerLink::ecrire_image ──── piste média RTP ────▶ LinkEvent::Image
     |                                     (RFC 7798)              |
     └────── canal de données ◀──── MessageControle ───────────────┤
                                                            sky-decode  (NVDEC)
                                                                   |  ImageDecodee (surface GPU)
                                                            sky-rendu   (D3D11, BT.601)
                                                                   |
                                                            fenêtre native
```

**Crates neufs** : `sky-decode`, `sky-rendu`. **Crates modifiés** : `sky-net` (piste média),
`sky-partage` (le découpage maison disparaît, le spectateur décode et affiche), `sky-encode`
(deux fonctions), `sky-app` (ouverture et fermeture de la fenêtre, remontée des mesures).

**Inchangés** : `handshake.rs`, `stun.rs`, `rendez_vous.rs`, tout `sky-crypto` (les enveloppes
portent du SDP, pas du transport), `sky-compte`, et **l'intégralité de l'API du site**. Le jalon 2
ne demande aucun déploiement.

---

## §5 Interfaces

### `sky-net`

*Corrigé le 02/10/2026 — `LinkEvent::Disconnected` n'a jamais existé.* La spec l'avait écrit de
mémoire ; les variantes réelles de `sky-net` sont `Failed(String)` et `Idle`, et l'implémenteur
de la tâche 5 a gardé le code existant au lieu d'inventer une variante. `Image` porte en outre un
champ `sans_perte`, ajouté à la tâche 6 : sous GOP infini, c'est le **seul** signal qu'un paquet
a manqué. Forme réelle (`sky-net/src/link.rs:139`) :

```rust
pub enum LinkEvent {
    Connected,
    /// Une unité d'accès complète, réassemblée par le dépaquetiseur RFC 7798 de str0m.
    Image { donnees: Vec<u8>, horodatage_ms: u64, cle: bool, sans_perte: bool },
    /// Message de contrôle arrivé par le canal de données (D2).
    Controle(MessageControle),
    Idle,
    /// Lien perdu ou jamais établi ; message déjà rédigé, garanti sans adresse.
    Failed(String),
}

impl PeerLink {
    pub fn ecrire_image(&mut self, unite: &[u8], horodatage_ms: u64) -> Result<(), ErreurEnvoi>;
    pub fn envoyer_controle(&mut self, message: &MessageControle) -> Result<(), ErreurEnvoi>;
}
```

`send(&[u8])` et `LinkEvent::Data(Vec<u8>)` disparaissent : le lien cesse d'être un tuyau
d'octets pour devenir un tuyau d'images, ce qui est plus honnête sur ce qu'il fait.

La négociation annonce le profil **Main 4:4:4** via
`CodecConfig::add_h265(pt, rtx, profile_id, tier_flag, level_id)`. La sonde a transporté
correctement avec le profil Main par défaut, parce que la paquetisation ne lit pas le contenu du
NAL : **aucun test de transport ne verra jamais cette erreur** (§9).

### `sky-net` — messages de contrôle

Le type vit dans `sky-net` (`src/controle.rs`), **pas** dans `sky-partage` : `LinkEvent::Controle`
le nomme, et `sky-partage` dépend de `sky-net`, donc l'inverse serait une dépendance circulaire.
`sky-net` a déjà `serde` et `serde_json` dans ses dépendances.

```rust
#[derive(Serialize, Deserialize)]
pub enum MessageControle {
    /// Le spectateur ne peut pas décoder : il demande un point de reprise.
    DemandeImageCle,
    /// L'hôte a cessé de partager.
    PartageArrete,
}
```

Sérialisés en JSON par `serde_json`, déjà présent dans le workspace. Ces messages sont rares et
minuscules ; la lisibilité vaut plus ici que la compacité.

### `sky-decode`

```rust
pub struct Decodeur { /* ... */ }

/// Ne porte aucun pixel : la surface vit sur le GPU (D5).
pub struct ImageDecodee { /* ... */ }

pub enum ErreurDecodeur {
    /// Aucune carte NVIDIA : cette machine ne peut ni diffuser ni recevoir (§7).
    AucuneCarteNvidia(String),
    /// Carte présente, mais son décodeur ne prend pas le HEVC 4:4:4 (antérieure à Turing).
    QuatreQuatreQuatreNonPris,
    /// Carte capable du 4:4:4, mais pas à cette taille.
    ResolutionTropGrande { largeur: u32, hauteur: u32, maximum: (u32, u32) },
    /// Carte capable, session refusée par le pilote.
    SessionRefusee(i32),
    /// Le contexte CUDA du décodeur n'a pas pu être rendu courant sur le fil.
    ContexteCuda(String),
}

impl Decodeur {
    pub fn nouveau(largeur: u32, hauteur: u32) -> Result<Self, ErreurDecodeur>;

    /// `None` n'est pas une erreur : le décodeur avale les en-têtes sans rendre d'image.
    pub fn decoder(
        &mut self,
        unite: &[u8],
        horodatage_ms: u64,
    ) -> Result<Option<ImageDecodee>, ErreurDecodeur>;
}
```

Les capacités sont interrogées **avant** l'ouverture de session, pour que les motifs de refus
soient distincts dans le type et pas dans une chaîne de caractères.

*Corrigé le 02/10/2026 — `ErreurDecodeur` a cinq variantes, pas trois* (`sky-decode/src/capacites.rs:17`).
`ResolutionTropGrande` (tâche 2) : un refus de taille était rendu sous `QuatreQuatreQuatreNonPris`,
dont le message aurait dit à une carte capable qu'elle ne sait pas recevoir. `ContexteCuda`
(tâche 2, ronde 3) : un échec de `bind_to_thread` était rendu sous `SessionRefusee`, avec un code
qui n'en était pas un. `AucuneCarteNvidia` porte le texte de l'erreur de chargement, que
l'interface **n'affiche jamais** (texte potentiellement traduit par Windows) : elle se branche sur
la variante. Autre écart : `Decodeur::nouveau` ne prend **pas** d'appareil Direct3D — le décodeur
travaille sur son propre contexte CUDA (`sky-decode/src/decodeur.rs:149`).

### `sky-rendu`

```rust
pub struct Fenetre { /* ... */ }

pub enum EtatVisionnage { EnAttente, ConnexionPerdue, PartageArrete }

pub enum EvenementFenetre { FermetureDemandee, PleinEcranBascule }

impl Fenetre {
    pub fn ouvrir(titre: &str, appareil: &ID3D11Device, largeur: u32, hauteur: u32)
        -> anyhow::Result<Self>;
    pub fn afficher(&mut self, image: &ImageDecodee) -> anyhow::Result<()>;
    /// Une fenêtre noire muette est un défaut, pas un état.
    pub fn afficher_etat(&mut self, etat: EtatVisionnage) -> anyhow::Result<()>;
    pub fn pompe_messages(&mut self) -> Vec<EvenementFenetre>;
}
```

Mise à l'échelle en préservant le rapport d'image, bandes noires au besoin : déformer du texte
serait inacceptable pour l'usage visé.

*Corrigé le 02/10/2026 — deux signatures diffèrent dans le code* (`sky-rendu/src/fenetre.rs:131`
et `:179`). `Fenetre::ouvrir(titre, largeur, hauteur)` ne reçoit pas d'appareil : la fenêtre crée
le sien, sur l'adaptateur NVIDIA choisi explicitement, et l'expose par `Fenetre::appareil()`.
`afficher` prend un `&dyn ImageAAfficher` (trait introduit à la tâche 4, le constructeur
d'`ImageDecodee` étant privé à `sky-decode`) ; aucune de ses méthodes ne rend d'octets de pixel,
donc D5 tient.

### `sky-encode` — deux ajouts

*Corrigé le 02/10/2026 — l'encodeur s'appelle `NvencEncoder`, et sa méthode d'encodage `encode`*
(`sky-encode/src/nvenc.rs:79` et `:304`). La spec et le plan écrivaient `Encodeur` et `encoder`,
de mémoire ; les deux ajouts ci-dessous existent sous ces noms (`nvenc.rs:358` et `:391`).

```rust
impl NvencEncoder {
    /// VPS + SPS + PPS en Annex-B, récupérés par nvEncGetSequenceParams.
    pub fn entetes_de_sequence(&self) -> anyhow::Result<Vec<u8>>;
    /// Pose NV_ENC_PIC_FLAG_FORCEIDR | NV_ENC_PIC_FLAG_OUTPUT_SPSPPS sur la prochaine image.
    pub fn forcer_image_cle(&mut self);
}
```

---

## §6 L'écart 3 — en-têtes de séquence

Constat confirmé des deux bouts de la chaîne : **un seul IDR, un seul jeu VPS/SPS/PPS pour
901 images**, parce que `idrPeriod = NVENC_INFINITE_GOPLENGTH` rend le drapeau de répétition
inerte — `sky-encode/src/nvenc.rs:211` le dit déjà lui-même.

**Le risque aigu à ce jalon n'est pas celui que décrivait la spec d'architecture.** Comme
`partager` attend un spectateur avant d'encoder, l'IDR unique est produit **après**
l'établissement du canal : « rejoindre en cours de route » est un problème du jalon 3
(multi-spectateurs). Le risque réel ici est la **perte** : si les paquets portant les en-têtes
n'arrivent pas, le décodeur refuse tout le flux et le spectateur voit une fenêtre noire
définitive, sans issue.

*Corrigé le 02/10/2026 — ce paragraphe décrivait le mauvais symptôme.* Le flux est en
**rafraîchissement intra progressif** : GOP et `idrPeriod` infinis, période de rafraîchissement de
2 s, vague d'environ 1 s (`sky-encode/src/nvenc.rs:176` et `:230-232`, confirmé par la revue de la
tâche 7 ; la durée exacte du cycle n'est **pas mesurée**). Conséquence que cette spec n'énonçait
pas : un spectateur qui décode sans avoir vu d'image clé obtient **des images, mais fausses ou
corrompues, sans aucun signal d'erreur du décodeur**. Ce n'est donc pas « rien à l'écran » mais
**une image faussement plausible** — un défaut différent, et pire, puisque personne ne s'en
aperçoit. Ce qui se passe sans les en-têtes est autre chose encore : NVDEC rend `Ok(None)` à
chaque unité (mesuré à la tâche 7 : 0 image sur 9 paquets sans en-têtes), donc une attente muette
et non un refus. Le garde-fou ne peut venir que de l'application : le spectateur **n'affiche
jamais avant une image clé**, cesse d'afficher dès que `sans_perte` est faux ou que le décodeur
refuse, montre « En attente de l'image… » entre-temps, et demande une image clé dans tous ces
cas (tâche 9, `sky-partage/src/spectateur.rs`). Cela vaut aussi pour « rejoindre en cours de
route » au jalon 3.

**Remède, en deux temps :**

1. L'hôte place `entetes_de_sequence()` en tête du flux envoyé à chaque spectateur.
2. Un spectateur qui ne peut pas décoder envoie `MessageControle::DemandeImageCle` par le canal
   de données ; l'hôte appelle `forcer_image_cle()`. La demande est **limitée à une par seconde**
   côté spectateur : sans cela, un flux inintelligible provoquerait une avalanche d'images clés
   qui saturerait la liaison au pire moment.

---

## §7 Erreurs et messages destinés à l'utilisateur

Textes exacts, en français, à reprendre verbatim dans `app/src/messages.ts` :

| Situation | Message |
|---|---|
| Aucune carte NVIDIA | « Cette machine n'a pas de carte graphique NVIDIA. SkyShare ne peut ni partager son écran ni en recevoir un sur cette machine. » |
| Carte NVIDIA antérieure à Turing | « La carte graphique de cette machine peut partager un écran, mais pas en recevoir un : son décodeur ne prend pas en charge la couleur pleine résolution. » |
| Session de décodage refusée | « Le décodeur vidéo n'a pas pu démarrer. Fermez les autres applications qui utilisent la carte graphique, puis réessayez. » |
| Flux illisible malgré une demande d'image clé | « L'image ne peut pas être reconstituée. Demandez à la personne qui partage de relancer son partage. » |

**Correction d'une inexactitude du dépôt.** `CLAUDE.md` affirmait jusqu'ici : « Pas de repli
logiciel x264. Sans carte NVIDIA, une machine ne peut que recevoir. » C'est faux sous cette
conception : sans carte NVIDIA, il n'y a **ni NVENC ni NVDEC**, donc ni diffusion ni réception.
La phrase supposait un décodage logiciel qui n'existe nulle part dans le projet. Le cas « peut
diffuser, pas recevoir » est celui des **cartes NVIDIA antérieures à Turing**, qui encodent mais
ne décodent pas le 4:4:4.

---

## §8 Diagnostic, mesures, vie privée

`Reception::absorber` calcule déjà images par seconde, débit, gigue RFC 3550 et transit — puis
les jette. Le jalon 2 les remonte jusqu'à l'interface, avec la latence de décodage et le nombre
d'images abandonnées.

**La contrainte « aucune adresse IP n'est jamais journalisée » tient par une seule chose et ce
jalon ne la touche pas : aucun collecteur `tracing` n'est installé**, ce qui rend les macros de
`str0m` inertes. Il est donc interdit, dans ce jalon comme ailleurs :

- d'installer un collecteur `tracing`, même « juste pour déboguer » ;
- d'utiliser `RUST_LOG="str0m=debug"` — le drapeau `pii` ne masque que ce qu'il enveloppe dans
  `Pii<T>`, et les traces les plus bavardes de `str0m` formatent source et destination avec un
  `Debug` ordinaire.

Toute instrumentation de ce jalon porte sur **notre propre code**, par compteurs typés remontés
par les événements existants. Aucune adresse ne circule : ni dans un compteur, ni dans un message
d'erreur, ni dans le titre de la fenêtre. La question ouverte « diagnostic : rien n'est conçu »
n'est pas close par ce jalon ; elle recule d'un cran, parce qu'on aura enfin des nombres à
montrer.

---

## §9 Vérification

Trois classes de preuves, avec ce que chacune **ne** prouve pas.

### Une preuve automatisable et discriminante : le décodage contre une référence

Décoder `spike/cmp-hevc-444.h265` et comparer l'image 120 à
`spike/mesures/frame120-hevc-444.png`, seuil **≥ 80 dB** (mesuré par la sonde : 89,78 dB ; par le
test de `sky-decode` : **85,50 dB** — *corrigé le 02/10/2026*, deux chemins de conversion, voir §2).

Ce test rougit pour les deux erreurs les plus probables du jalon, et pour la bonne raison :

- une conversion 4:2:0 parasite fait tomber le PSNR à **15,06 dB absolus** (neutralisation
  mesurée à la tâche 2) — *corrigé le 02/10/2026 : la spec disait « fait perdre 19–20 dB », ce qui
  confondait un écart avec le plancher absolu du jalon 0, voir §2* ;
- une matrice **BT.709** au lieu de BT.601 plafonne à **36 dB** (36,13 dB mesurés à la tâche 2).

Neutralisations à exécuter, chacune devant faire rougir ce test **seul** : remplacer la matrice
par BT.709 ; insérer un aller-retour 4:2:0 avant la comparaison.

### Une preuve à écrire d'avance, parce qu'aucun test de transport ne la donnera

Le `fmtp` doit annoncer **Main 4:4:4**. Le test porte sur **ce que la réponse SDP retient**, pas
sur ce qui passe : la sonde a transporté correctement avec le mauvais profil annoncé, puisque la
paquetisation ne lit pas le contenu du NAL. Neutralisation : remettre `profile_id = 1` — le test
doit rougir, et lui seul.

C'est le motif « la serrure posée mais jamais branchée », l'une des classes de défauts
récurrentes du projet : une annonce fausse qu'aucun chemin d'exécution ne contredit.

### Les autres tests exigés

| Propriété | Test | Neutralisation qui doit le faire rougir |
|---|---|---|
| Le décodeur refuse au lieu de dégrader (D8) | capacités simulées sans 4:4:4 → `QuatreQuatreQuatreNonPris` | rendre un repli 4:2:0 au lieu de l'erreur |
| `None` n'est pas une erreur | pousser des en-têtes seuls → `Ok(None)` | transformer `None` en erreur |
| Les en-têtes précèdent le flux (§6) | la première écriture contient VPS, SPS et PPS | retirer l'appel à `entetes_de_sequence()` |
| La demande d'image clé est limitée | dix demandes en 100 ms → un seul `forcer_image_cle()` | retirer la limitation |
| Un état est affiché quand il n'y a pas d'image | chaque variante d'`EtatVisionnage` produit un rendu distinct | rendre une fenêtre noire dans tous les cas |
| Aucune adresse dans les mesures | les compteurs remontés ne contiennent aucune forme d'adresse | insérer l'adresse du pair dans un compteur |

Chaque test répond à la question « qu'est-ce qui, précisément, ferait échouer celui-ci ? », et sa
neutralisation doit être **une seule décision changée à l'intérieur d'un objet qui continue
d'exister** — pas un déplacement qui casse la construction, piège déjà payé au jalon C2.

### Ce qu'aucun test ne prouvera

Qu'un pixel juste apparaît à l'écran, et que la latence de bout en bout est acceptable.

**Deux essais réels, dans cet ordre**, et la leçon du C2 est appliquée : ils se prévoient tôt,
sur le plus petit chemin de bout en bout, pas après que les revues ont fini de polir des morceaux
jamais assemblés.

1. **Essai local**, dès que le décodage et la fenêtre tiennent debout : hôte et spectateur sur la
   même machine. Prouve le pixel, la couleur, la fenêtre, l'arrêt propre. Ne prouve rien du
   réseau.
2. **Essai à deux machines sur deux réseaux**, dispositif identique à celui du C2 (PC fixe et
   portable, deux comptes Discord). Seul cet essai peut clore l'**écart 7** — le RTT ne se mesure
   pas autrement — et seul lui donne la latence de bout en bout réelle.

À mesurer aussi, non mesuré à ce jour : **le décodage pendant un encodage** sur un même moteur
NVDEC. Une machine peut diffuser vers l'une et regarder l'autre. Si cela ne tient pas, c'est une
limite à écrire, pas à laisser découvrir par un utilisateur.

---

## §10 Limites assumées, écrites d'avance

- **Un seul spectateur, un seul écran, pas de son.**
- **Sans carte NVIDIA : ni diffusion ni réception.** Carte antérieure à Turing : diffusion
  seulement.
- **Déchirement possible**, conséquence assumée de D7 (latence prioritaire).
- **L'écart 7 reste ouvert** jusqu'au second essai réel. Ce jalon établit que la cause supposée
  disparaît, pas que le RTT est bon.
- **L'écart 5 n'est pas traité** : sous congestion, le régulateur continue de sauter des images
  entières au lieu de baisser le débit de l'encodeur.
- Un spectateur qui rejoint **en cours** de partage n'est pas un cas traité (jalon 3).

---

## §11 Questions ouvertes à la fin de ce jalon

- Le sort du `Pacer` maison face à `str0m::bwe`, à trancher sur mesure lors de l'essai à deux
  machines. Les deux sont mutuellement exclusifs en pratique.
- La latence décodeur → pixel affiché, et si le budget de 11 059 200 octets par image tient à
  165 im/s.
- L'empaquetage de la voie A (D1 du 23/08) : neuf inconnues, et du matériel AMD et Intel à
  emprunter ou acheter.
- Le diagnostic d'un incident rapporté par un utilisateur, sous la contrainte « aucune adresse
  journalisée » : ce jalon donne des nombres, pas encore un moyen d'enquêter.
