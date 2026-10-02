# TODO — SkyShare

## État au 02/10/2026 — à lire en premier

- **Jalons 0, C1, C2 et 1 : terminés et fusionnés.** Jalon 0 : GO ferme le 23/08 (M4 et M1
  mesurées). Jalon 1 : fusionné dans `main` le 27/09.
- **Jalon 2 — le premier pixel : implémenté, revu, corrigé ; essais dus.** Branche
  `jalon-2-premier-pixel`, non fusionnée. Revue finale de branche le 02/10 (1 critique,
  4 importants), puis vague de correction finale. Ce qui reste dû est dans la section « Jalon 2 »
  plus bas ; la fiche d'essai est `spike/docs/essai-jalon-2.md`.
- Le bandeau d'arrêt du 23/08 et l'« état actuel » du jalon 0 qui suivent sont **conservés pour
  l'historique et périmés** : le propriétaire a donné le feu vert du jalon 1, et le jalon 0 est
  passé de GO CONDITIONNEL à GO ferme.

> ## ⛔ ARRÊT DEMANDÉ PAR LE PROPRIÉTAIRE — 23/08/2026 *(historique, levé)*
>
> **Ne pas lancer le jalon 1 tant que le propriétaire n'a pas dit « go » explicitement.**
>
> Cela vaut aussi après la clôture du jalon 0, après la revue de branche, et après
> une éventuelle reprise de session. Aucune interprétation, aucune anticipation :
> le feu vert doit être donné par le propriétaire, en toutes lettres.
>
> Ce qui reste autorisé sans son accord : terminer la revue du jalon 0, la revue de
> branche, et répondre à ses questions.


## État du jalon 0 au 23/08/2026 *(historique, périmé)*
- **Phase : jalon 0 terminé côté agents — GO CONDITIONNEL.**
  Rapport de faisabilité : `spike/docs/rapport-jalon-0.md`.
- **Deux mesures humaines bloquent le passage à un GO ferme** : **M4** (test P2P avec un
  correspondant distant → clôt Q5, le risque n°1) et **M1** (débit de capture sur écran
  en mouvement réel → clôt **la moitié** de Q1). Le code qui les produit est écrit,
  compilé et testé ; **commandes et avertissements dans `spike/docs/mesures-a-realiser.md`**
  (dans le dépôt depuis la revue de branche — la checklist d'origine vivait sous
  `.superpowers/`, exclu du dépôt).
- **⚠ M4 se lance avec `--source synthetique --seconds 5`, jamais `sky-probe host` seul.**
  Sans options, la commande capture l'écran réel de l'opérateur pendant 30 s, l'encode,
  l'envoie — et le programme du correspondant l'écrit sur **son** disque (jusqu'à ~112 Mo).
  Q5 ne demande aucune vidéo : la connexion s'établit ou non.
- **⚠ Pendant M4 : n'activer aucun journal réseau détaillé.** Le filtrage de données
  personnelles de la bibliothèque ne couvre pas son point de trace le plus volumineux —
  les adresses des deux machines sortiraient en clair dans la console, et donc dans toute
  capture d'écran partagée ensuite.
- **⚠ M1 ne clôt que la moitié de Q1.** Le seuil du plan est « ≥ 59 fps **et** < 1 %
  d'images perdues ». `CaptureStats.dropped` compte des délais d'attente dépassés, pas
  des images perdues (`wgc.rs:134-136` le dit lui-même) : **aucun instrument de cette
  branche ne mesure le taux réel de perte.** À écrire au jalon 2.
- **Les rapports de tâche restent hors dépôt.** `.superpowers/` est exclu par
  `.gitignore`, délibérément : artefacts de travail. Les renvois `task-N-report.md`
  ci-dessous nomment leur source sans la rendre rouvrable depuis le dépôt seul.
- Chemin : **architectural** (nouveau projet, 6 sous-systèmes).

## Décisions validées (cadrage)
| # | Sujet | Décision |
|---|-------|----------|
| 1 | Stack | Tauri (UI React) + cœur Rust natif — capture/encodage/transport natifs |
| 2 | Infra | **Zéro serveur.** Signaling via API routes Vercel + Neon. STUN publics. Pas de TURN. |
| 3 | Vie privée | Candidats ICE chiffrés E2E (Vercel/DB ne voient jamais d'IP en clair), IP jamais affichée ni loguée, IP locales masquées en mDNS `.local`. **✅ D2 tranchée le 23/08/2026 — le sens de l'échange est inversé (spec §5.1)** : seule la *réponse* est scellée aujourd'hui ; l'*offre* part avant qu'une clé de destinataire n'existe, donc en clair. Tant que la boîte aux lettres est un simple relais, l'opérateur du serveur voit l'adresse publique de chaque émetteur — voir spec §2 décisions 4 et 5, et §5.1 |
| 4 | Accès public | Ami accepté → entrée directe. Inconnu via lien → salle d'attente + approbation de l'hôte. |
| 5 | Multi-spectateurs | 10+ supportés. Simulcast 3 couches (encodage constant, réseau linéaire). Jauge d'upload en direct. |
| 6 | Audio | Son du partage uniquement (Opus haute qualité). Pas de micro/vocal — Discord s'en charge. |
| 7 | Plateforme | **Windows d'abord**, tranche verticale complète. Abstraction OS dès J1. Mac/Linux ensuite. |
| 9 | Sync annuaire | Bouton manuel + auto-sync **30 s au premier plan / 5 min en arriere-plan**. Signaling rapide (500ms) uniquement pendant les ~3s de negociation. ~112k req/mois pour 10 users (11% du quota Vercel). 1 sync = 1 seule requete DB groupee. |
| 8 | DB | Tables préfixées `sky_*` dans le Neon existant. **Zéro modification** du schéma Erinium. Liaison par `discord_id`. |

## Évolutions notées (hors périmètre v1)
- Relais entre pairs (arbre de diffusion) pour diviser l'upload de l'hôte sans serveur
- Piste micro (le pipeline audio sera dimensionné pour l'accueillir)

## Prochaines étapes
1. [x] Questions de cadrage
2. [x] Choix de l'approche transport — WebRTC via `str0m`, congestion réécrite
3. [x] Design validé section par section (6/6)
4. [x] Document d'architecture écrit et commité (`docs/superpowers/specs/2026-08-22-skyshare-architecture-design.md`)
5. [x] Relecture et validation du document d'architecture
6. [x] Plan d'implémentation du jalon 0 → `docs/superpowers/plans/2026-08-22-jalon-0-faisabilite.md`
7. [x] **Exécution du jalon 0** — 9 tâches sur 9 terminées. Verdict : **GO CONDITIONNEL**.
   - [x] Tâche 3 : Encodage NVENC depuis texture D3D11 (Q2) — DONE. **Q2 = OUI** : NVENC accepte directement la texture D3D11 de la capture, en 4:4:4, sans copie CPU. Vérifié par `ffprobe` : `hevc`/`Rext`/`yuv444p`/2560x1440, **1007 images décodées = 1007 encodées**. Mesure sur source synthétique (60,0 i/s, 2560x1440) : **médiane 6,29 ms, p99 7,47 ms**. *(Trois valeurs corrigées en revue de branche : cette ligne portait encore 1105 = 1105 / 5,97 / 7,39, mesurés avec le chronomètre défectueux qui s'arrêtait avant la libération des ressources — écart d'environ 0,3 ms par image, et favorable à la conclusion. Le rapport de faisabilité, lui, **écarte explicitement** les latences de la Tâche 3 : elles ont été prises sur le motif d'avant sa refonte en Tâche 4 et ne sont plus reproductibles. Les chiffres courants sont ceux du rapport : 4,88 / 5,96 ms.)* Repli FFmpeg du Step 8 **non déclenché**. Latence sur écran réel non concluante (WGC ne délivre d'image que si l'écran change) ; qualité visuelle non validée — à juger par le propriétaire. Détail : `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/task-3-report.md`.
   - [x] Tâche 1 : Workspace et détection matérielle — DONE. `pick_best` testé TDD (5/5, GREEN) ; `probe_hardware` charge NVENC dynamiquement (`nvEncodeAPI64.dll` via `libloading`, comme FFmpeg/OBS — pas besoin du NVIDIA Video Codec SDK). `cargo run -p sky-probe -- hw` détecte réellement la RTX 4060 : choix partage d'écran = HEVC 4:4:4. Détail : `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/task-1-report.md`.
   - [x] Tâche 5 : Scellage des adresses réseau (`sky-crypto`) — DONE. TDD strict (RED confirmé sur `Identity` introuvable, GREEN 5/5). API réelle de `crypto_box` 0.9.1 diffère du brief (méthodes `PublicKey::seal`/`SecretKey::unseal`, pas de fonctions libres) ; surcoût de scellage confirmé à 48 octets (32 clé éphémère + 16 tag), inchangé. Détail : `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/task-5-6-report.md`.
   - [x] Tâche 6 : Contrôle de congestion à plancher garanti (`sky-net::pacer`, Q6) — DONE. TDD strict (RED confirmé sur `Pacer` introuvable, GREEN 5/5). Ne descend jamais sous le plancher même à 50 % de perte soutenue ; descente bornée à 15 %/tick ; remontée au plafond en 1 tick après un à-coup. `lib.rs` ne déclare que `pub mod pacer;` (pas `handshake`, réservé à la Tâche 7). **Corrigé en revue de branche : `Pacer::new` ne validait pas ses bornes** — `--floor-mbps 50 --bitrate-mbps 30` faisait paniquer `clamp` au premier retour d'information, après tout l'aller-retour humain. `new` rend désormais un `Result` et le régulateur est construit avant l'offre ; **7/7 GREEN**. Détail : `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/task-5-6-report.md`.
   - [x] Tâche 2 : Capture d'écran GPU (`sky-capture`, Q1) — DONE. Chaîne D3D11 → WGC → `ID3D11Texture2D` fonctionnelle de bout en bout, sans copie processeur (établi par lecture : aucun `Map`, aucun `CopyResource`, aucune texture intermédiaire). **Le débit d'images n'est PAS mesuré** — aucun agent ne peut produire un écran en mouvement réel, et WGC ne délivre d'image que si le contenu change. Q1 reste PARTIELLE → mesure M1. Détail : `task-2-report.md`.
   - [x] Tâche 4 : Comparatif des codecs à débit égal (Q3) — DONE. **Q3 = OUI, large.** À cible 10 Mbps, régime établi (781 images) : HEVC 4:4:4 rend PSNR U 58,30 / V 48,58 dB contre 18,21 / 19,29 dB (H.264 4:2:0) et 18,12 / 19,27 dB (AV1 4:2:0), soit **+40,1 dB sur U, +29,3 dB sur V**. Deux codecs 4:2:0 différents convergent sur le même plancher chroma : l'écart est un artefact du sous-échantillonnage, pas d'un réglage. Lisibilité perçue non jugée → M3. Découverte portant l'écart n°2 : H.264 4:4:4 rend 70,93 Mbps quelle que soit la cible. Détail : `task-4-report.md`.
   - [x] Tâche 7 : Connexion pair-à-pair entre deux machines (Q5) — DONE_WITH_CONCERNS. **Q5 reste OUVERTE** : aucun test n'a franchi un NAT, tout s'est fait en boucle locale. Ce que la tâche a produit : la suppression de **trois mécanismes distincts pouvant produire une fausse réponse négative**, dont aucun n'était visible dans un test local réussi (la bibliothèque ne découvre pas l'adresse publique ; un champ mal renseigné aurait fait rejeter tous les paquets entrants distants ; une minuterie interne coupait à 30 s). Détail : `task-7-report.md`. **⚠ Réserve de promotion :** le journal de bord porte « 11 constats mineurs mis de côté » pour cette tâche **sans en énumérer un seul**. Ils sont perdus et portent sur `link.rs` et `stun.rs`, deux fichiers désignés pour promotion → à relire intégralement avant reprise, pas à reprendre en confiance.
   - [x] Tâche 8 : Chaîne complète et mesures de bout en bout (Q4) — DONE_WITH_CONCERNS. **Q4 = OUI** : processeur 0,53 % médian / 1,26 % max, encodeur matériel 25 % médian et jamais nul, 60,0 i/s soutenues sur 60 s en 1440p60 HEVC 4:4:4. Seconde mesure sur écran réel concordante. Porte l'écart n°5 (le régulateur pilote la cadence, pas le débit). Détail : `task-8-report.md`.
   - [x] Tâche 9 : Rapport de faisabilité — DONE. `spike/docs/rapport-jalon-0.md`. Spec révisé, douze sections : §2 (décisions 4 et 5), §5.1, §5.2, §5.3, §5.5, §6.1, §6.2, §6.4, §6.5, §7.5, §9, §11. Détail : `task-9-report.md`.
8. [x] **Revue finale de branche** (32 commits vus comme un tout) — correctifs appliqués.
   Deux critiques : (1) `README-AMI.md` promettait à un tiers l'inverse de ce que le
   programme fait — défaut né **entre** les Tâches 7 et 8, invisible à toute revue de
   tâche ; protocole M4 basculé sur la source synthétique et mode d'emploi réécrit.
   (2) Le tableau des codecs du spec accordait le 4:4:4 à Pascal et Maxwell — seuil
   corrigé à Turing. Plus six correctifs importants et cinq mineurs. Détail :
   `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/correctifs-revue-branche.md`.

## Jalon 0 — les six écarts avec le document d'architecture

Reportés du rapport de faisabilité. Les cinq premiers reposent sur une mesure de ce
jalon ; le sixième sur une recherche documentaire, sans matériel de test.

| # | Écart | Section du spec | Suite à donner |
|---|-------|-----------------|----------------|
| 1 | AV1 ne fait pas de 4:4:4 sur NVENC, même sur Ada | §6.4 | **Corrigé.** AV1 réservé au partage de vidéo, pas d'écran de travail |
| 2 | H.264 4:4:4 rend 70,93 Mbps quelle que soit la cible (RTX 4060 / pilote 610.74) | §6.4 | **Corrigé.** Écarté du socle ; fusionne avec l'écart 6 dans **D1, tranchée : voie A** |
| 3 | Les en-têtes de séquence ne sont émis qu'une fois → un spectateur qui rejoint en cours ne verrait **rien** | §5.3, §5.4, §6.4, §6.5 | **Corrigé + à implémenter au jalon 2** : émission à l'arrivée d'un spectateur, ou transmission hors flux vidéo |
| 4 | L'offre de connexion voyage **en clair** et a une forme de diffusion ; au jalon 1 l'opérateur du serveur verrait l'adresse publique de chaque émetteur | §2 (déc. 4 et 5), §5.1, §5.2 | **D2 TRANCHÉE le 23/08/2026** : le sens de l'échange est inversé — c'est le spectateur qui produit l'offre, scellée avec la clé de l'hôte publiée sans adresse. L'hôte ne livre les siennes qu'après acceptation. Voir spec §5.1 |
| 5 | Le régulateur ne peut que sauter des images ; il ne pilote pas le débit | §6.1, §6.5, §7.5 | **Corrigé + à implémenter** : exposer `nvEncReconfigureEncoder` dans `sky-encode` |
| 6 | Le 4:4:4 n'existe pas sur AMD (aucune surface dans AMF, `AMF_INVALID_FORMAT` sur RDNA 3), non attesté sur Intel, **et absent du parc NVIDIA d'avant Turing** (GTX 10xx : pas de HEVC 4:4:4 du tout) | §6.1, §6.4, §11 | **Corrigé + D1 TRANCHÉE le 23/08/2026 : voie A, empaquetage.** Formulation exacte : « **NVIDIA de 2018 ou plus récent contre tout le reste** », pas « NVIDIA contre le reste » — corrigé en revue de branche. Prérequis : extraire un trait `VideoEncoder`, inexistant |

## Jalon 0 — décisions remontées au propriétaire, non tranchées

- **D1 — que promet-on aux utilisateurs dont la carte ne fait pas de 4:4:4 ?** La
  population n'est pas « les non-NVIDIA » mais **tout ce qui n'est pas une NVIDIA de
  septembre 2018 ou plus récente** : AMD, Intel, le mobile, *et* le parc NVIDIA
  antérieur à Turing. Arbitrage entre « sans compromis
  partout » (repli d'encodage hors NVENC, coût non mesuré) et « sans compromis sur le
  matériel récent » (4:2:0 ailleurs,
  dit dans l'interface). Voies décrites au §6.4 du spec. **Instruite ailleurs** :
  `docs/superpowers/notes/2026-08-23-compatibilite-toutes-cartes-graphiques.md` consigne
  quatre décisions de périmètre qu'elle présente comme déjà prises par le propriétaire, et
  recommande un spike dédié après le jalon 3 — note non vérifiée par la Tâche 9. Prérequis
  absolu : confirmer sur matériel AMD et Intel réel — tout le constat de l'écart 6 est
  documentaire.
- **D2 — comment restructurer le signaling pour que l'offre soit scellée ?** La direction
  est identifiée ; la conception appartient au jalon 1, qui construit la boîte aux lettres.

## Manque identifié au jalon 0, à instruire avant le jalon 2

- **Diagnostic et journalisation** — rien n'est conçu, rien n'existe. Aucun moyen de
  comprendre un incident signalé par un utilisateur. Tension à résoudre : le spec promet
  qu'aucune adresse n'est jamais journalisée, et le jalon 0 a établi que le filtrage de
  `str0m` ne couvre pas son point de trace le plus volumineux. Contraintes détaillées
  dans le spec, §10, sous-section « Manque identifié au jalon 0 ».

## Jalons
| # | Jalon | État |
|---|-------|------|
| 0 | Faisabilité (capture + encode + P2P) | ✅ **TERMINÉ — GO ferme. 6/6 questions closes, M4 et M1 mesurées le 23/08 sur réseaux réels** |
| 1 | Fondations (Discord, amis, listes) — l'application | ✅ **TERMINÉ le 27/09/2026**, fusionné dans `main` ; essai réel à deux machines dû |
| 2 | Premier pixel (partage 1-à-1) | **implémenté, revue finale et vague de correction faites** (branche `jalon-2-premier-pixel`) — essai local, mesure du décodage pendant un encodage et essai à deux machines **dus**. Écart **3** traité ; écart **5** reporté au jalon 3 ; D1 hors périmètre (voir section « Jalon 2 ») |
| 3 | Qualité (simulcast, profils) | à faire — bloqué sur l'écart **5** (reconfiguration de débit à chaud) |
| 4 | Lecteur (multi-flux, zoom, audio) | à faire |
| 5 | Public (liens, salle d'attente) | à faire |
| 6 | Distribution (CI, installateurs) | à faire |
| 7 | Mac et Linux | à faire |

## Acquis du jalon 0 — 23 août 2026

Connexion pair-à-pair établie en **0,4 s** entre deux machines, deux réseaux, deux
fournisseurs d'accès, **sans aucun serveur relais**. Chaîne vidéo complète transmise :
2560×1440 HEVC 4:4:4, **jusqu'à 107 images/s reçues**, 12,4 Mbps, gigue 5-17 ms.
Capture à **164,3 im/s** (rapport 0,99 au taux d'écran), CPU de la chaîne à **0,10 %**.

**Le pari technique du projet tient.**

## Nouvelle question ouverte, à trancher au jalon 2

- **Écart 7 — le canal de données n'est pas un transport vidéo.** RTT mesuré à 115 ms là
  où une liaison fibre-fibre directe devrait donner 15-30 ms, et **16 % d'échecs d'envoi**
  (2611/16349). Le canal de données est conçu pour des messages, pas pour un flux temps
  réel. Comparer aux **pistes média** de WebRTC — ce qui rouvre la décision de
  bibliothèque, `str0m` contre `webrtc-rs` (spec §2, décision 3).
- **Repli logiciel x264** — le spike n'implémente que NVENC. ~~Une machine sans carte
  NVIDIA ne peut pas émettre, seulement recevoir.~~ *Corrigé le 02/10/2026 : faux depuis le
  jalon 2, qui décode par NVDEC sans aucun repli logiciel. Sans carte NVIDIA, une machine ne
  peut **ni émettre ni recevoir**. Même phrase que celle corrigée dans `CLAUDE.md` le 30/09.*
  *Corrigé par la vague finale du jalon 2 (02/10/2026) : cette ligne ajoutait « une NVIDIA
  antérieure à Turing peut émettre, pas recevoir ». Faux : partager exige un encodeur HEVC 4:4:4
  (sinon refus, revue finale I2), et l'écart 6 ci-dessus dit que les GTX 10xx n'en ont pas.*

---

## Jalon C1 — l'API de signaling — CLOS le 04/09/2026

Fusionné dans `main` (`89161ff`) et déployé en production. 33 commits, **338 tests**
contre 45 au début du jalon. Six tables, douze routes `/api/sky/*`. Vérifié en
production : les routes répondent 401 sans session — un 404 aurait signifié qu'elles
n'étaient pas parties, un 500 qu'elles étaient cassées.

Livré : amitiés avec acceptation, blocage ineffaçable par la personne bloquée, code ami
à 8 caractères, listes d'amis, boîte aux lettres à enveloppes scellées. L'échange
d'enveloppe est prouvé de bout en bout avec de vraies clés X25519 — le test **déchiffre**
réellement, une clé fausse ferait échouer le GCM.

### Reporté au jalon C2

- ~~Les comptes protégés par double authentification ne peuvent pas se connecter depuis
  l'application native.~~ **Réglé le 13/09/2026** (jalon C2, tâche 2, déployé en
  `f916dcc`) : le second facteur se fait dans le navigateur, et le `state` signé traverse
  l'étape TOTP, lié à la session qui l'a ouvert.
- ~~`sontAmis` et `proprietaireDe` n'ont aucun appelant en production — une protection
  construite sans être branchée, à câbler au C2.~~ **Diagnostic faux, corrigé le
  13/09/2026** : leur prédicat est répliqué exprès dans `deposer`, `definirMembres` et
  `amisDe` ; les câbler rouvrirait la fuite temporelle. **Ne pas les câbler** (voir
  tasks/lessons.md, 13/09).
- **Le versionnage de la synchronisation ignore les suppressions non maximales** : un
  client peut manquer une suppression si une autre, plus récente, a déjà relevé la version.
- **Ne pas toucher à l'alphabet du code ami** (31 caractères, S/5 et B/8 exclus) : le
  changer modifierait l'entropie et invaliderait les codes déjà émis.
- **Poids de la synchronisation : 2 929 octets** dans le cas réaliste, soit ~55 Mo/mois.
  Réglable par la cadence de sondage si nécessaire ; le budget de requêtes n'en dépend pas.

---

## Jalon C2 — le client de signaling — TERMINÉ le 19/09/2026

Branche `jalon-c2-client-signaling`. Spec : `docs/superpowers/specs/2026-09-11-jalon-c2-client-signaling-design.md`
(D1–D8). Plan : `docs/superpowers/plans/2026-09-11-jalon-c2-client-signaling.md` (11 tâches). Journal détaillé,
avec chaque arbitrage : `.superpowers/sdd/2026-09-11-jalon-c2-client-signaling/progress.md` (hors dépôt).

### État au 16/09/2026
- [x] Tâches 1 à 11 closes, chacune relue et corrigée jusqu'à revue propre.
  - Partie site (tâche 2, second facteur dans le flux natif) **déployée** le 13/09 (`f916dcc`).
  - Crate `sky-compte` : connexion native, trousseau, renouvellement, annuaire, boîte aux lettres.
  - `sky-probe host` / `view` négocient **par la boîte aux lettres**, plus aucun copier-coller.
- [x] Revue finale de branche : avec corrections, 0 critique, 2 importants, 5 mineurs.
- [x] Vague de correction de la revue finale, puis re-revue ciblée (N1 corrigé par le contrôleur).
- [x] **Essai réel — RÉUSSI le 19/09/2026.** PC fixe (RTX 4060, box) en `host`, portable sur un autre réseau
  avec un second compte Discord en `view`. Réponse reçue **6,1 s** après le lancement de `view` (3
  synchronisations), canal ouvert en **0,6 s**, **7,1 s** au total ; aucune enveloppe restée non ouverte ; aucun
  relais. Deux bugs de `login` trouvés et corrigés en route (`cmd` qui coupait l'URL au `&` ; `natif=1` absent),
  invisibles à tout test car aucun n'ouvre de navigateur.
  - La vidéo a ensuite échoué (tampon d'émission saturé, RTT 330 ms, plancher 10 Mbps) : **écart 7**, jalon 2,
    hors périmètre du C2.
- [x] **Défaut du site, trouvé pendant l'essai — corrigé et déployé le 19/09/2026** (`54fd093`, statut Vercel
  `success`, route vérifiée en production) : `creerSessionNative` refusait tout compte à double
  authentification, alors que la tâche 2 leur fait traverser le TOTP puis émet un code. Le code émis par
  `totp/verify` porte désormais `auth_codes.totp_verifie = TRUE` ; celui du callback reste `FALSE`, ce qui garde
  la protection contre un TOTP activé entre l'émission et l'échange. Test de chaîne ajouté (vérification TOTP
  en flux natif, puis échange du code émis). 360 tests. Relu : aucun constat critique ni important.
- [ ] Ergonomie : `view` sur un nom inconnu pourrait lister les amis disponibles (l'essai a d'abord tenté le nom
  de l'appareil).
- [x] Fin de branche : fusionnée dans `main` le 19/09/2026 (avance rapide `97f73c5..9ea7cc5`), jalon 0 compris,
  sur décision du propriétaire.

### Limites connues, assumées
- **Les enveloppes sont consommées à la livraison** : `friends list`, `friends add`, `device list`, `code`, ou
  un autre `host`/`view` sur le même appareil pendant un partage volent les messages de la négociation.
- **Une session vit 7 jours** et chaque `login` en crée une nouvelle ; l'appareil est lié à la session. La
  vague de correction finale rattache l'appareil au login (révocation puis réenregistrement, même clé).
- Un site malveillant qui substituerait une clé d'annuaire n'est pas détecté — inhérent à D2.
- Cadences `CADENCE` 2 s, `FENETRE_HOTE` 30 min, `ATTENTE_SPECTATEUR` 60 s : **argumentées, pas mesurées**.

---

## Jalon 1 — l'application — développement terminé, essai réel dû

Branche `jalon-1-application`, **non fusionnée**. Spec : `docs/superpowers/specs/2026-09-19-jalon-1-application-design.md`.
Plan : `docs/superpowers/plans/2026-09-19-jalon-1-application.md` (13 tâches). Journal détaillé, avec chaque
arbitrage : `.superpowers/sdd/2026-09-19-jalon-1-application/progress.md` (hors dépôt).

### État au 27/09/2026
- [x] **Tâches 1 à 12 closes**, chacune relue et corrigée jusqu'à revue propre (26 commits).
  - Site : les membres des listes dans la synchronisation — **déployé en production** le 23/09 (`fe7af2d`).
  - `sky-partage` : la négociation extraite de `sky-probe`, événements typés, signal d'arrêt.
  - `sky-app` : cœur Tauri — boucle de synchronisation unique, dix-sept commandes, icône près de l'horloge,
    instance unique, démarrage avec Windows, identifiant **et trousseau distincts** en développement.
  - `app/` : Connexion, Amis, Listes, Mon compte, panneau de partage. **321 tests Rust, 86 d'interface.**
- [x] Revue finale de branche : **0 critique**, 1 important, 4 mineurs — corrigés, re-revue propre.
- [x] **Premier usage réel, 27/09/2026** : le propriétaire a installé l'application et s'est connecté. Le code
  ami, les appareils et les écrans s'affichent. Mesuré par instrumentation sur la build empaquetée : la boucle
  démarre, la synchronisation réussit en **0,9 s**, l'interface reçoit l'état rempli **1,9 s** après le lancement.
  Le pont cœur → interface fonctionne donc de bout en bout.
  - **Les appareils révoqués sont désormais masqués** (`d197ca2`) : le site ne sait pas les supprimer, et chaque
    connexion en crée un. Les supprimer vraiment demanderait une route de plus et un déploiement.
- [ ] **Essai réel à deux machines — toujours dû** : partage et réception entre deux comptes, sur deux réseaux.
  Installateur : `spike/target/release/bundle/nsis/`.
- [x] Fin de branche : fusionnée dans `main` le 27/09/2026, sur décision du propriétaire.

### Limites connues, assumées
- **Aucune image** : « Regarder » montre la connexion et ses mesures, le flux est mesuré puis jeté (spec D2).
  L'image arrive au jalon 2, avec le transport corrigé (écart 7). *Vrai au jalon 1 ; **faux depuis le jalon 2**,
  dont le spectateur décode et affiche dans une fenêtre native.*
- **Application non signée** : avertissement « éditeur inconnu » à l'installation (signature : jalon 6).
- **`sky-probe` en profil `debug` ouvre un coffre vide** : le lancer en `--release` pour retrouver l'identité
  réelle, sinon il enregistrerait un second appareil sur le compte.
- Pendant qu'un partage attend, aucune autre commande ne doit tourner sur la même machine : elle consommerait
  les enveloppes de la négociation.
- Le rang d'écran suppose l'ordre d'`EnumDisplayMonitors` — **relevé, pas mesuré** : à confirmer à l'essai réel
  sur une machine à plusieurs écrans.

### Reporté au jalon 2
- Coalescer les synchronisations déclenchées par une commande (deux requêtes par clic aujourd'hui).
- Joindre le fil de partage à la fermeture (le cœur ne garde aucun `JoinHandle` : changement structurel).
- Erreur typée plutôt qu'une comparaison de chaîne pour `MESSAGE_SESSION_CHANGEE`.

### Dette héritée, réglée
- ~~Nettoyer les jetons sur la sortie gardée de `demarrer`~~ — **fait à la tâche 8 du jalon 1** (`10fbae1`),
  avec son test et sa neutralisation.
- **Supprimer l'appareil orphelin éventuel** : une déconnexion tombant pendant le tout premier enregistrement
  d'appareil peut laisser une ligne inutile sur le compte (au plus une par machine). Elle ne détourne aucun
  partage ; l'écran « Mon compte » du jalon 1 permet de la révoquer.

### Reporté au jalon 2
- Erreur typée rendue par `PeerLink::repondant` : `echec_local` classe aujourd'hui sur le texte du message.
- Agent HTTP recréé à chaque appel : aucune connexion réutilisée, coût non mesuré.

---

## Jalon 2 — le premier pixel — implémenté, essais dus

Branche `jalon-2-premier-pixel`, **non fusionnée**. Spec :
`docs/superpowers/specs/2026-09-30-jalon-2-premier-pixel-design.md` (corrigée le 02/10, neuf points
signalés sur place). Plan : `docs/superpowers/plans/2026-09-30-jalon-2-premier-pixel.md` (12 tâches).
Journal détaillé : `.superpowers/sdd/2026-09-30-jalon-2-premier-pixel/progress.md` — **ignoré par
git** ; tout ce qui doit survivre à la fusion est recopié ci-dessous. Fiche d'essai pour le
propriétaire : `spike/docs/essai-jalon-2.md`.

### ⚠️ À lire avant l'essai

**1. Des textes affichés à l'utilisateur ne viennent pas de la spec — à valider par le propriétaire**
(`app/src/messages.ts`, chacun marqué sur son `case`). **Liste complète : spec du jalon 2, §7,
« Textes affichés qui ne viennent pas de ce tableau ».** La vague finale en a ajouté un (refus de
partager sans HEVC 4:4:4, `sky-app/src/noyau.rs`) et en a **corrigé deux** qui désignaient une
fausse cause : `sans_decodage_444` (promettait « peut partager un écran ») et `trop_lente`, devenu
`envoi_en_retard` (« la connexion était trop lente » pour une file locale). Les trois de la tâche 10,
jugés honnêtes par sa re-revue :
- `partage_arrete` : « Ton ami a arrêté son partage. »
- `resolution_trop_grande` : « Le décodeur vidéo de cette carte graphique s'arrête à
  ${largeurMax}×${hauteurMax} : SkyShare a besoin d'au moins ${largeur}×${hauteur} pour recevoir
  un écran sur cette machine. »
- `decodage_interrompu` : « Le décodage de l'image s'est interrompu en cours de visionnage, malgré
  les demandes de reprise. Relance le visionnage ; si cela se reproduit, demande à ton ami de
  relancer son partage. » — réserve de la re-revue : les demandes de reprise sont espacées d'une
  seconde, donc sur un enchaînement rapide de refus il y en a peu.
- Également signalé : l'interface **tutoie**, la spec §7 **vouvoie** (c'est la spec qui détonne).

**2. Le régulateur est bloqué au plafond sur le chemin nominal.** Depuis la migration vers la piste
média, l'hôte n'a **plus aucune mesure de RTT** : il passe un zéro en dur au `Pacer`
(`sky-partage/src/hote.rs:434`), l'écho du canal de données ayant disparu et RTCP n'étant lu par
personne. La perte n'est plus visible qu'à travers les **refus de la file de paquetisation** — 0 sur
21552 écritures mesurées à 100 Mbps, **par la sonde du 27/09, en boucle locale**, quand la boucle sert
le réseau entre deux images (`hote.rs`) : ni par l'hôte réel, ni sur un réseau. Cette file est en
outre **locale** (vidée par nos propres `poll`) : un refus dirait que la boucle de l'hôte a pris du
retard, pas que le réseau est lent (revue finale, M4). Sur ce chemin nominal mesuré, `congestionne` reste faux et le
débit monte de 8 % par tic jusqu'au plafond, **et n'en redescend pas**. **Ce n'est pas structurel** :
`SEUIL_PERTE` valant 2 %, un seul refus dans une fenêtre de moins de cinquante images suffit à
faire détecter une congestion. **C'est la première chose à regarder si l'image se dégrade pendant
l'essai à deux machines.** Corollaire visible : la colonne « RTT » de `sky-probe host` et la ligne
« Aller-retour » du panneau de l'hôte affichaient **0 ms en dur**, qui se lisait comme une mesure
parfaite. Depuis la vague finale (revue finale, I4), les deux disent **« non mesuré »** : la valeur
voyage en `Option` du cœur jusqu'à l'écran. Le `Pacer`, lui, reçoit toujours un 0 en interne.

**3. Dette de vérifiabilité : la preuve du décodage n'est rejouable que sur la machine du
propriétaire.** Les tests de `sky-decode` lisent `spike/cmp-hevc-444.h265` (17,5 Mo) jusqu'à
l'image 120, et ce fichier est **exclu par `spike/.gitignore`**. Un extrait ne suffirait pas (il
faudrait plusieurs mégaoctets). Le test de transport de la tâche 6, lui, versionne son extrait de
63 003 octets (`.gitattributes -text`, vérifié).

**4. L'essai à deux machines sur deux réseaux reste dû.** Seul lui peut clore l'**écart 7** : le RTT
ne se mesure pas en boucle locale. Il exige désormais une **NVIDIA Turing ou plus récente des deux
côtés** — sans NVDEC, une machine ne peut plus regarder (le mode d'emploi du C2,
`spike/docs/essai-reel-c2.md`, dit encore « n'importe laquelle » pour le spectateur : vrai au C2,
faux pour le jalon 2).

### Ce qui est fait — tâches 1 à 10, chacune relue et corrigée jusqu'à revue propre

Au terme de la tâche 10 : `cargo test --workspace --no-fail-fast` **420 réussis, 0 échec**, Vitest
**97**, clippy et `tsc` propres, `tauri build` réussi (manifeste `PerMonitorV2` vérifié présent dans
`sky-app.exe`). Tout est poussé sur la branche (dernier : `516685b`).

- [x] **T1 — sonde matérielle du décodeur** (`sky-decode`, `bd37dd2..af59c2f`) : verdict des
  capacités testable sans GPU ; absence de pilote vérifiée par `nvcuda.dll` **avant** `cudarc`, sans
  `catch_unwind`.
- [x] **T2 — décodage NVDEC** (`af59c2f..f164832`, 3 rondes) : **85,50 dB** contre la référence,
  écart maximal 1 niveau ; neutralisations BT.709 → 36,13 dB, 4:2:0 → 15,06 dB. Aller-retour
  encodage → décodage en 1920×1080 : **0 pixel mal classé sur 2 073 600** — la surface mappée suit
  `ulTargetHeight`, pas `coded_height` (une revue avait conclu l'inverse ; la mesure l'a réfuté).
- [x] **T3 — fenêtre native** (`sky-rendu`, `f164832..99d0b70`) : trois états sans image aux fonds
  distincts, adaptateur NVIDIA (`0x10DE`) choisi explicitement.
- [x] **T4 — affichage GPU** (`99d0b70..276527e`, 2 rondes) : interopérabilité CUDA–D3D11, BT.601
  pleine échelle, couleurs à 1 niveau près ; test sur une vraie surface NVDEC.
- [x] **T5 — canal de données réduit au contrôle** (`sky-net`, `276527e..797e388`, 2 rondes) :
  `MessageControle`, canal passé en `Reliability::Reliable`.
- [x] **T6 — piste média HEVC** (`797e388..7d5793d`, 2 rondes) : profil **Main 4:4:4** annoncé et
  prouvé par la réponse SDP (le transport, lui, passe intact sous un profil menteur) ;
  `WriteWithoutPoll` traité comme récupérable ; `sans_perte` exposé.
- [x] **T7 — en-têtes de séquence et image clé forcée** (`sky-encode`, `7d5793d..f5a79b4`, 2 rondes) :
  NVDEC consomme les en-têtes de `nvEncGetSequenceParams` (0 image sur 9 paquets sans eux, au moins
  une avec — HEVC 4:4:4, 1080p, RTX 4060 seulement).
- [x] **T8 — hôte sur la piste média** (`f5a79b4..67765fc`, **4 rondes**) : en-têtes joints au premier
  envoi, image clé à la demande non reportable par le budget, annonce de l'arrêt couverte par
  construction (`en_annoncant_l_arret`), API transitoire `envoyer_octets_bruts` retirée.
- [x] **T9 — le spectateur décode et affiche** (`67765fc..5618c16`, 1 ronde) : n'affiche jamais avant
  une image clé, reprise après perte, demandes d'image clé limitées à une par seconde, pause sur
  inactivité (la boucle active brûlait un cœur).
- [x] **T10 — fenêtre dans l'application, messages du décodage** (`5618c16..516685b`, 1 ronde) :
  `Fin::PartageArrete` (un arrêt annoncé n'est plus un « ÉCHEC »), `DecodageInterrompu` distinct des
  erreurs d'ouverture, F11 transactionnel (un échec ne coupe plus le visionnage), conscience DPI par
  manifeste.

### Ce qui reste dû
- [ ] **Tâche 11 — essai local** : premier regard humain sur l'image. **Premier geste : lancer
  l'application empaquetée, qui n'a jamais démarré avec son nouveau manifeste** (un manifeste
  invalide bloquerait le démarrage). Puis couleur, netteté à 150 %, trois états, F11.
  **Obstacle relevé à la tâche 12, déduit du code et jamais essayé** : la commande du plan
  `view <nom-de-l-appareil>` ne peut pas marcher — `view` ne regarde qu'un **ami**
  (`sky-compte/src/annuaire.rs:781`) et une build `--release` n'a qu'une identité par machine.
  Hôte et spectateur sur une même machine exigent une seconde identité, donc le coffre `debug` avec
  le second compte Discord : montage proposé dans la fiche, **à accepter par le propriétaire**.
- [ ] **Tâche 12, volet mesure — décodage pendant un encodage** (hôte et spectateur simultanés sur
  la même machine, 60 s, `nvidia-smi dmon`). Si cela ne tient pas, l'écrire dans les limites ici
  **et** dans `CLAUDE.md`.
- [ ] Écrire les deux résultats dans `spike/docs/mesures-jalon-2.md` (fichier prévu par le plan,
  pas encore créé).
- [ ] **Essai à deux machines sur deux réseaux** — clôt l'écart 7 (voir ci-dessus).
- [x] Revue finale de branche (02/10/2026) : 1 critique, 4 importants, et des mineurs. **Le critique,
  les importants et les mineurs classés « avant fusion » sont traités** par la vague de correction
  finale (voir la sous-section suivante). Restent ouverts, classés « peut attendre » par la revue :
  M3 (une seule image relevée par appel au décodeur), M5 (fragment technique affiché sur une panne
  d'affichage), M6 (déchirement probablement absent, donc latence d'une période), M7 (gigue qui mêle
  l'encodage au réseau), et les mineurs reportés ci-dessous qui ne sont pas marqués soldés.
- [ ] Fusion, sur décision du propriétaire — **après** les essais, qui seuls peuvent confirmer C1 et I1
  sur un vrai écran.
- [ ] Le plan n'a **pas** été corrigé (la spec et la note l'ont été) : il porte encore `npx tauri
  build`, `Encodeur`/`encoder`, `LinkEvent::Disconnected`, le chiffre « 19–20 dB » et une
  neutralisation inopérante (amputer le **premier** octet d'une unité d'accès ne discrimine pas :
  `next_start_code` accepte trois ou quatre octets, `str0m` `h265.rs:331-334`). Document
  historique ; à corriger ou à marquer comme tel.
- [x] **Double non corrigé du chiffre « 19–20 dB », dans du code** (`sky-decode/tests/reference.rs`) :
  corrigé par la vague finale (15,06 dB absolus mesurés, voir la spec §2).

### Vague de correction finale (02/10/2026)

Rapport : `.superpowers/sdd/2026-09-30-jalon-2-premier-pixel/final-fix-report.md` (ignoré par git).
Chaque correction a son test et sa neutralisation, appliquée seule.
- **C1 — le décodeur rendait l'image de l'unité précédente** : `CUVID_PKT_ENDOFPICTURE` posé ; prouvé
  sur le vrai décodeur. La garde d'affichage n'est plus décalée d'une image.
- **I1 — écran figé côté hôte** : la dernière capture est copiée et ré-encodée quand une image clé est
  due (au plus ~50 ms d'attente) ou quand la première n'a pas encore été envoyée. **Coût non mesuré** :
  une copie GPU de ~14 Mo par image capturée en 2560×1440.
- **I2 — le partage exige le HEVC 4:4:4** : refus avant tout réseau sinon (application et `heberger`).
- **I3 — une erreur de décodage avant la première image affichée est une erreur d'ouverture** ; la
  taille codée du flux est aussi comparée au maximum de la carte.
- **I4 — « Aller-retour : 0 ms »** devient « non mesuré », du cœur jusqu'à l'écran.
- Mineurs 20 et 49, M1 (faits périmés dans le code), M2 (« images fausses » : déduit), M4 (deux fins
  qui désignaient une fausse cause).
- **Défense en profondeur non faite** : la revue recommandait de faire porter la garde d'affichage sur
  l'horodatage de l'image rendue plutôt que sur l'ordre des appels. Le test sur le vrai décodeur suffit
  à la correction ; la garde reste sur l'ordre des appels.

### Limites connues, écrites d'avance
- **Un seul spectateur, un écran, pas de son** ; déchirement possible (présentation sans attente de
  synchronisation verticale, D7).
- **Sans NVIDIA : ni diffusion ni réception.** Sans encodeur HEVC 4:4:4 : pas de diffusion — le
  partage est refusé avant tout réseau, au lieu d'envoyer un flux que personne ne peut lire (revue
  finale, I2). Sans décodeur HEVC 4:4:4 : pas de réception. *La ligne disait « NVIDIA antérieure à
  Turing : diffusion seulement », en contradiction avec l'écart 6 (GTX 10xx : pas de HEVC 4:4:4) ;
  quelles générations passent l'un ou l'autre n'est pas tranché, aucune n'ayant été essayée.*
- **Un spectateur sans image clé verrait une image plausible et peut-être fausse**, sans erreur du
  décodeur (rafraîchissement intra progressif) — l'absence d'erreur est mesurée, la fausseté déduite.
  Le garde-fou est applicatif, dans `spectateur.rs`.
- **L'aller-retour décodage n'est éprouvé qu'en 1440 et 1080**, deux multiples de 8 ; l'en-tête
  n'exige qu'un alignement sur 2, donc une hauteur comme 1050 n'est couverte ni par la mesure ni par
  la documentation.
- **Consigne absolue pendant tout essai : jamais `RUST_LOG="str0m=debug"`** (adresses en clair).
- Constantes **choisies, non mesurées** : `ATTENTE_IMAGE_CLE_MAX` = 10 s,
  `ECHECS_DECODAGE_AVANT_ABANDON` = 120, `DRAINAGE_ARRET` = 50 ms ; effet de la pause sur inactivité
  (1 ms demandée, ~15 ms réelles sous Windows) sur la latence : non mesuré.
- **`transit_ms` compare deux horloges d'origines différentes** (hérité, documenté) : ce n'est pas
  une latence de bout en bout. **Aucun instrument du dépôt ne mesure la latence capture → pixel.**

### Mineurs reportés, recopiés du journal (62 constats, tâche par tâche)

Recopiés parce que le journal est ignoré par git. « Soldé » signifie qu'une tâche ultérieure l'a
traité selon le journal ou le code ; tous les autres sont **ouverts**.

**Tâche 1 — `sky-decode`, sonde matérielle**
1. `#[allow(dead_code)]` posé sur tout `nvcuvid_sys.rs` au lieu des seuls éléments concernés.
2. `Capacites` dérive `Clone` sans en avoir besoin (seul `Debug` est exigé par `expect_err`).
3. Le test de sonde matérielle accepte toute erreur, `SessionRefusee` comprise : une régression du
   matériel ou du pilote resterait verte.
4. Le message de `SessionRefusee` parle d'une « session » alors qu'il peut naître d'un simple appel
   de capacités (`cuvidGetDecoderCaps`).
5. Les pointeurs de fonction FFI rendent l'énumération Rust `CUresult` : un code inconnu renvoyé par
   le pilote serait un comportement indéfini.
6. `depuis_brut` ignore `eCodecType`, `eChromaFormat` et `nBitDepthMinus8` : les lignes de fixture
   correspondantes ne testent rien.
7. `anyhow` et `windows` sont déclarés dans `sky-decode` sans être utilisés (prescrits par le brief).
8. `le_pilote_cuda_se_charge_sur_cette_machine` échoue sur une machine sans NVIDIA — voulu, à revoir
   si une intégration continue sans GPU apparaît.
9. Le message d'`AucuneCarteNvidia` incorpore un texte de `libloading` possiblement traduit par
   Windows. **Soldé** : l'interface se branche sur la variante, jamais sur le texte (`messages.ts`).

**Tâche 2 — `sky-decode`, décodage**
10. Aucun vidage `CUVID_PKT_ENDOFSTREAM` : les dernières images d'un flux borné peuvent rester dans
    le décodeur. Sans effet sur un flux vivant.
11. `unites_acces` découpe des NAL, pas des unités d'accès : nom trompeur. **Soldé** par la vague
    finale : la fonction regroupe désormais les NAL en unités d'accès (exigé par C1).
12. Le test de référence retient 1,34 Go en mémoire.
13. Le `allow(dead_code)` de `nvcuvid_sys.rs` est devenu obsolète.
14. `ulErrorThreshold = 0` rend une corruption indiscernable d'un en-tête — à reprendre avec
    l'écart 7.
15. `display_area` est rétréci en `i16`.
16. La saturation des quatre surfaces de sortie n'est pas exercée ; son issue documentée inclut un
    **blocage du fil appelant**, et un test qui peut figer la suite a été refusé à raison.
17. `cargo fmt --all --check` échoue sur du code **préexistant** (`sky-app`, `sky-encode`) : dette
    d'outillage hors jalon, à signaler à la revue finale.
18. Le test d'aller-retour dépend du `coded_height` choisi par NVENC ; sa garde de pertinence doit
    le signaler si NVENC change d'avis.

**Tâche 3 — `sky-rendu`, fenêtre**
19. Un échec de `redimensionner` est avalé sans commentaire (`fenetre.rs:391` à l'époque).
20. Le commentaire « échec = intact » est faux pour la chaîne d'échange (`fenetre.rs:532` à l'époque).
    **Soldé** par la vague finale (commentaire corrigé ; le comportement, rare, reste à corriger).
21. L'image est étirée pendant le glissement du bord, la boucle de redimensionnement de Windows étant
    modale (`fenetre.rs:381` à l'époque).
22. `ouvrir` montre une fenêtre vide avant le premier `afficher_etat` (`fenetre.rs:431` à l'époque).
23. Aucune assertion sur `IsWindowVisible == false` pour la fenêtre masquée des tests.
24. Le `Drop` de la fenêtre n'appelle ni `ClearState` ni `Flush`.
25. `let Ok(..) else { break }` sur `EnumAdapters1` traitait toute erreur comme une fin de liste
    (repli silencieux possible sur l'Intel). **Soldé** à la tâche 4.
26. Le test d'adaptateur sortait en silence si `adaptateur_nvidia()` rendait `None` à tort. **Soldé**
    à la tâche 4.

**Tâche 4 — `sky-rendu`, interopérabilité CUDA–D3D11**
27. `desenregistrer` ignore en silence l'échec de `bind_to_thread` (hérité, documenté).
28. Le test du descripteur de copie n'assertait pas l'absence de `srcArray`/`dstDevice`. **Soldé** à
    la ronde 2 (et mesuré : le pilote ignore ces champs).
29. `D3DCompile` est appelé à chaque ouverture de fenêtre.
30. Le filtrage linéaire a été choisi sans mesure de qualité perçue.
31. Le périphérique CUDA 0 est codé en dur — comme dans `sky-decode` : à traiter dans les deux crates
    ou dans aucun.

**Tâche 5 — `sky-net`, canal de contrôle**
32. Un échec non reproduit : deux tests ont dépassé 10 s à l'ouverture du canal, une fois. Attribué
    sans preuve à un démarrage à froid, hypothèse ensuite écartée. Si cela revient, c'est un test
    instable, pas du bruit.
33. `messages_illisibles()` n'a toujours aucun appelant de production (tests seulement) ;
    `envoyer_controle`, lui, est désormais branché par l'hôte et le spectateur.
34. Stabilité sous charge non exercée ; délai de retransmission SCTP de `str0m` ni mesuré ni lu.
35. **À surveiller** : une exécution sur cinq a pris 4,5 s au lieu de 0,5 s en réussissant ; le
    `sleep` d'1 ms n'est pas prouvé être le remède. Les helpers `pomper_*` n'ont que 5 s de marge
    contre 10 s pour `paire_connectee` : si l'instabilité revient, c'est là qu'elle frappera, et il
    faudra instrumenter le temps d'ouverture du canal avant de régler quoi que ce soit.

**Tâche 6 — `sky-net`, piste média**
36. Un `to_vec()` par image reçue.
37. Le nom `PROFIL_MAIN_444` face à l'appellation officielle *Format Range Extensions*.
38. Désalignement entre `wallclock` et `rtp_time` (`link.rs:783` à l'époque).
39. Le test de `sans_perte` a une assertion faible : rien ne se perd en boucle locale, il vérifie le
    **câblage**, pas la **détection**. La détection réelle ne se prouvera qu'à l'essai à deux machines.

**Tâche 7 — `sky-encode`, en-têtes et image clé**
40. Sans NVIDIA, les trois tests de `sky-decode` **paniquent** là où ceux de `sky-encode` sortent en
    silence : asymétrie à trancher si une intégration continue sans GPU apparaît.
41. La **justesse** de l'image rendue après une image P orpheline n'est pas mesurée : les tests
    mesurent la production d'images, pas leur exactitude. Mesure suggérée : image rendue après IDR
    forcé contre après P orphelin.

**Tâche 8 — `sky-partage`, hôte**
42. En-têtes doublés sur le premier paquet (licite : le premier paquet est un IDR qui porte déjà les
    siens).
43. Le signal `arret` n'est pas consulté pendant la relance d'une écriture refusée ni pendant le
    drainage.
44. La documentation d'`entetes_a_joindre` décrit un cas impossible.
45. La fenêtre du `Pacer` est modifiée de l'extérieur.
46. Les 50 ms de `DRAINAGE_ARRET` ne sont pas vérifiées ; l'abrègement du drainage n'est prouvé que
    sur doublure, et sur un vrai lien l'événement de déconnexion est déjà consommé quand l'annonce
    démarre.
47. Une sortie d'`etablir` en `Fin::Arrete` avant `diffuser` n'annonce rien : jugé non-problème (canal
    pas encore ouvert, rien d'affiché chez le spectateur), à documenter seulement.
48. L'enveloppe `en_annoncant_l_arret` ne couvre ni une panique ni un `return` qu'on ajouterait
    **avant** elle : « structurellement difficile à oublier », pas « impossible ».

**Tâche 9 — `sky-partage`, spectateur**
49. `images_par_seconde` compte les images **reçues**, pas les images affichées — la valeur
    « images/s » de `sky-probe view` est donc celle des arrivées. **Soldé** par la vague finale : le
    panneau dit « Images reçues », le terminal « images reçues/s ».
50. `latence_decodage_ms` n'est pas testée (c'est une **moyenne** par relevé, pas une médiane).
51. Le `flush` du fichier de sortie est sauté sur les sorties par `?`.
52. Des accesseurs publics ne servent qu'aux tests.
53. Le test d'absence d'adresse cherche la sous-chaîne « ip » : source de faux positifs futurs.
54. **À vérifier à l'essai local** : si une unité d'image clé arrive **sans en-têtes exploitables**,
    `cle_vue` devient vrai, `Ok(None)` ne déclenche aucune demande et le délai d'abandon ne démarre
    pas — attente muette possible. Ce n'est pas le scénario du critique C1 (une unité perdue entière
    donne `sans_perte == false`) et ce n'est pas vérifiable sans carte graphique.
55. Sur le chemin d'erreur du décodage, la demande d'image clé est appelée deux fois par unité, sans
    effet grâce à la limitation.

**Tâche 10 — `sky-app`, `app/`, fenêtre dans l'application**
56. La fenêtre s'ouvre à 1280×720 **pixels physiques** : petite à 200 %.
57. Le changement de DPI quand on déplace la fenêtre d'un écran à l'autre n'est pas traité.
58. Pas de sortie du plein écran par Échap.
59. L'ouverture du décodeur dans `regarder` n'est protégée ni par le type ni par un test (relevé deux
    fois au journal, fusionné ici). *La vague finale (I3) touche le même endroit sans le solder.*
60. L'interface tutoie, la spec vouvoie (c'est la spec qui détonne).
61. Le test du F11 raté court-circuite l'appel Windows : il prouve la logique de retour arrière, pas
    l'état réel laissé par un `SetWindowPos` en échec.
62. L'échec de F11 n'a plus aucun témoin (assumé, documenté).

---

### Cadrage d'origine (27/09/2026), conservé pour l'historique

Deux sondes de faisabilité ont été lancées **avant** d'écrire la spec, parce que deux des trois
inconnues du jalon ne reposaient sur aucune mesure. Résultats complets :
`docs/superpowers/notes/2026-09-27-sondes-jalon-2-decodage-et-transport.md`.

#### Tranché sur mesure (27/09/2026)
- [x] **NVDEC décode notre HEVC 4:4:4.** 901 images sur 901, **569 im/s**, latence **1,57 ms médiane**
  (p99 3,39 ms), 8,2 % d'un cœur, décodeur à ~89 %. Justesse contre la référence du jalon 0 :
  **89,78 dB, 99,979 % de pixels identiques**. Le 4:4:4 est préservé, prouvé de trois façons dont une
  discriminante (un aller-retour 4:2:0 forcé perd 19–20 dB et altère 14 % des échantillons).
  *Corrigé le 02/10/2026 : « perd 19–20 dB » est faux — c'est un plancher absolu, pas un écart ; la
  neutralisation mesurée donne 15,06 dB absolus. Le test de `sky-decode` mesure 85,50 dB par un
  autre chemin de conversion. Détail dans la note et la spec §2.*
  - **Le flux est en BT.601 pleine échelle, pas BT.709** (BT.709 plafonne à 36 dB). À porter dans le
    code de rendu, avec le chiffre en commentaire.
  - Piège écarté : mpv en `--hwdec=nvdec` **se replie silencieusement sur le logiciel** sur ce flux.
    Un lecteur qui « décode » ne prouve rien du matériel.
- [x] **Le transport passe aux pistes média de `str0m` 0.23.1** — pas de passage à `webrtc-rs`.
  HEVC est actif par défaut dans str0m (`enable_h265(true)`), son paquetiseur consomme de l'**Annex-B**,
  donc aucune conversion. **0 refus d'écriture sur 2593 à 12 Mbps et sur 21552 à 100 Mbps**, mesurés
  **en boucle locale**, contre 16 % entre deux réseaux. Ce qui est structurel, c'est l'absence de la
  contre-pression SCTP qui produit les 16 % ; le zéro, lui, est une mesure locale, pas une propriété.
  *Corrigé par la vague finale (02/10/2026) : la phrase disait « et c'est structurel » juste après le
  zéro, double survivant de la correction faite dans la note des sondes.* Intégrité vérifiée : 61 NAL
  émis, 61 reçus, identiques octet pour octet.
  - Argument décisif : sans `enable_bwe`, str0m installe un **`NullPacer`** — le projet garde
    intégralement son contrôle de congestion, sans rien contourner. Coût : **400 à 450 lignes**, dont
    ~140 supprimées, contre 1200 à 3500 pour `webrtc-rs` (async dans une application synchrone).

### Reste à trancher
- [ ] **L'écart 7 n'est PAS clos.** Les 115,7 ms ont été mesurés entre deux machines et deux réseaux ;
  la sonde n'a mesuré qu'une boucle locale, **sans valeur comparative**. Ce qui est établi, c'est que la
  cause supposée disparaît. **Un essai réel à deux machines reste dû** — le même dispositif que le C2.
- [ ] `Pacer` maison contre `str0m::bwe` : mutuellement exclusifs en pratique (`enable_bwe` installe
  aussi le `LeakyBucketPacer`). Commencer sans BWE, comparer les deux à l'essai réel.
- [ ] **Latence décodeur → pixel affiché** : le vrai risque restant. Une image décodée pèse 11,06 Mio,
  le chemin doit rester sur le GPU.
- [ ] Décodage **pendant** un encodage NVENC sur le même moteur : non mesuré, sonde courte.
- [x] Profil HEVC annoncé dans le `fmtp` : la sonde a utilisé Main par défaut, le projet encode en
  **Main 4:4:4** — `add_h265` avec le bon `profile_id`. *Fait à la tâche 6 du jalon 2, prouvé par la
  réponse SDP.*
- [x] Écart 3 (en-têtes de séquence émis une seule fois — confirmé côté décodeur : **un seul IDR pour
  901 images**) : *traité au jalon 2 (tâches 7 à 9).*
- [ ] Écart 5 (le régulateur ne pilote pas le débit : `nvEncReconfigureEncoder` absent de l'API de
  `sky-encode`) : reporté au jalon 3.
- [ ] Décision D1 (machines sans NVIDIA récente) : direction fixée le 23/08 (voie A, empaquetage), ses
  neuf inconnues restent entières et demandent du matériel AMD/Intel absent à ce jour.
