# TODO — SkyShare

> ## ⛔ ARRÊT DEMANDÉ PAR LE PROPRIÉTAIRE — 23/08/2026
>
> **Ne pas lancer le jalon 1 tant que le propriétaire n'a pas dit « go » explicitement.**
>
> Cela vaut aussi après la clôture du jalon 0, après la revue de branche, et après
> une éventuelle reprise de session. Aucune interprétation, aucune anticipation :
> le feu vert doit être donné par le propriétaire, en toutes lettres.
>
> Ce qui reste autorisé sans son accord : terminer la revue du jalon 0, la revue de
> branche, et répondre à ses questions.


## État actuel
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
| 1 | Fondations (Discord, amis, listes) | à faire — porte la décision **D2** (annuaire de clés) |
| 2 | Premier pixel (partage 1-à-1) | à faire — doit traiter les écarts **3** et **5**, et porte la décision **D1** |
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
- **Repli logiciel x264** — le spike n'implémente que NVENC. Une machine sans carte
  NVIDIA ne peut pas émettre, seulement recevoir. Constaté en conditions réelles.

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

### Reporté au jalon 2
- Erreur typée rendue par `PeerLink::repondant` : `echec_local` classe aujourd'hui sur le texte du message.
- Agent HTTP recréé à chaque appel : aucune connexion réutilisée, coût non mesuré.
