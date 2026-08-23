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
  en mouvement réel → clôt Q1). Le code qui les produit est écrit, compilé et testé ;
  commandes dans `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/mesures-humaines.md`.
- **⚠ Pendant M4 : n'activer aucun journal réseau détaillé.** Le filtrage de données
  personnelles de la bibliothèque ne couvre pas son point de trace le plus volumineux —
  les adresses des deux machines sortiraient en clair dans la console, et donc dans toute
  capture d'écran partagée ensuite.
- Chemin : **architectural** (nouveau projet, 6 sous-systèmes).

## Décisions validées (cadrage)
| # | Sujet | Décision |
|---|-------|----------|
| 1 | Stack | Tauri (UI React) + cœur Rust natif — capture/encodage/transport natifs |
| 2 | Infra | **Zéro serveur.** Signaling via API routes Vercel + Neon. STUN publics. Pas de TURN. |
| 3 | Vie privée | Candidats ICE chiffrés E2E (Vercel/DB ne voient jamais d'IP en clair), IP jamais affichée ni loguée, IP locales masquées en mDNS `.local`. **⚠ Conditionné à D2 (écart 4 du jalon 0)** : seule la *réponse* est scellée aujourd'hui ; l'*offre* part avant qu'une clé de destinataire n'existe, donc en clair. Tant que la boîte aux lettres est un simple relais, l'opérateur du serveur voit l'adresse publique de chaque émetteur — voir spec §2 décisions 4 et 5, et §5.1 |
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
   - [x] Tâche 3 : Encodage NVENC depuis texture D3D11 (Q2) — DONE. **Q2 = OUI** : NVENC accepte directement la texture D3D11 de la capture, en 4:4:4, sans copie CPU. Vérifié par `ffprobe` : `hevc`/`Rext`/`yuv444p`/2560x1440, 1105 images décodées = 1105 encodées. Mesure sur source synthétique (60,0 i/s, 2560x1440) : médiane 5,97 ms, p99 7,39 ms. Repli FFmpeg du Step 8 **non déclenché**. Latence sur écran réel non concluante (WGC ne délivre d'image que si l'écran change) ; qualité visuelle non validée — à juger par le propriétaire. Détail : `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/task-3-report.md`.
   - [x] Tâche 1 : Workspace et détection matérielle — DONE. `pick_best` testé TDD (5/5, GREEN) ; `probe_hardware` charge NVENC dynamiquement (`nvEncodeAPI64.dll` via `libloading`, comme FFmpeg/OBS — pas besoin du NVIDIA Video Codec SDK). `cargo run -p sky-probe -- hw` détecte réellement la RTX 4060 : choix partage d'écran = HEVC 4:4:4. Détail : `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/task-1-report.md`.
   - [x] Tâche 5 : Scellage des adresses réseau (`sky-crypto`) — DONE. TDD strict (RED confirmé sur `Identity` introuvable, GREEN 5/5). API réelle de `crypto_box` 0.9.1 diffère du brief (méthodes `PublicKey::seal`/`SecretKey::unseal`, pas de fonctions libres) ; surcoût de scellage confirmé à 48 octets (32 clé éphémère + 16 tag), inchangé. Détail : `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/task-5-6-report.md`.
   - [x] Tâche 6 : Contrôle de congestion à plancher garanti (`sky-net::pacer`, Q6) — DONE. TDD strict (RED confirmé sur `Pacer` introuvable, GREEN 5/5). Ne descend jamais sous le plancher même à 50 % de perte soutenue ; descente bornée à 15 %/tick ; remontée au plafond en 1 tick après un à-coup. `lib.rs` ne déclare que `pub mod pacer;` (pas `handshake`, réservé à la Tâche 7). Détail : `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/task-5-6-report.md`.
   - [x] Tâche 2 : Capture d'écran GPU (`sky-capture`, Q1) — DONE. Chaîne D3D11 → WGC → `ID3D11Texture2D` fonctionnelle de bout en bout, sans copie processeur (établi par lecture : aucun `Map`, aucun `CopyResource`, aucune texture intermédiaire). **Le débit d'images n'est PAS mesuré** — aucun agent ne peut produire un écran en mouvement réel, et WGC ne délivre d'image que si le contenu change. Q1 reste PARTIELLE → mesure M1. Détail : `task-2-report.md`.
   - [x] Tâche 4 : Comparatif des codecs à débit égal (Q3) — DONE. **Q3 = OUI, large.** À cible 10 Mbps, régime établi (781 images) : HEVC 4:4:4 rend PSNR U 58,30 / V 48,58 dB contre 18,21 / 19,29 dB (H.264 4:2:0) et 18,12 / 19,27 dB (AV1 4:2:0), soit **+40,1 dB sur U, +29,3 dB sur V**. Deux codecs 4:2:0 différents convergent sur le même plancher chroma : l'écart est un artefact du sous-échantillonnage, pas d'un réglage. Lisibilité perçue non jugée → M3. Découverte portant l'écart n°2 : H.264 4:4:4 rend 70,93 Mbps quelle que soit la cible. Détail : `task-4-report.md`.
   - [x] Tâche 7 : Connexion pair-à-pair entre deux machines (Q5) — DONE_WITH_CONCERNS. **Q5 reste OUVERTE** : aucun test n'a franchi un NAT, tout s'est fait en boucle locale. Ce que la tâche a produit : la suppression de **trois mécanismes distincts pouvant produire une fausse réponse négative**, dont aucun n'était visible dans un test local réussi (la bibliothèque ne découvre pas l'adresse publique ; un champ mal renseigné aurait fait rejeter tous les paquets entrants distants ; une minuterie interne coupait à 30 s). Détail : `task-7-report.md`.
   - [x] Tâche 8 : Chaîne complète et mesures de bout en bout (Q4) — DONE_WITH_CONCERNS. **Q4 = OUI** : processeur 0,53 % médian / 1,26 % max, encodeur matériel 25 % médian et jamais nul, 60,0 i/s soutenues sur 60 s en 1440p60 HEVC 4:4:4. Seconde mesure sur écran réel concordante. Porte l'écart n°5 (le régulateur pilote la cadence, pas le débit). Détail : `task-8-report.md`.
   - [x] Tâche 9 : Rapport de faisabilité — DONE. `spike/docs/rapport-jalon-0.md`. Spec révisé, douze sections : §2 (décisions 4 et 5), §5.1, §5.2, §5.3, §5.5, §6.1, §6.2, §6.4, §6.5, §7.5, §9, §11. Détail : `task-9-report.md`.

## Jalon 0 — les six écarts avec le document d'architecture

Reportés du rapport de faisabilité. Les cinq premiers reposent sur une mesure de ce
jalon ; le sixième sur une recherche documentaire, sans matériel de test.

| # | Écart | Section du spec | Suite à donner |
|---|-------|-----------------|----------------|
| 1 | AV1 ne fait pas de 4:4:4 sur NVENC, même sur Ada | §6.4 | **Corrigé.** AV1 réservé au partage de vidéo, pas d'écran de travail |
| 2 | H.264 4:4:4 rend 70,93 Mbps quelle que soit la cible (RTX 4060 / pilote 610.74) | §6.4 | **Corrigé.** Écarté du socle ; fusionne avec l'écart 6 dans la décision **D1** |
| 3 | Les en-têtes de séquence ne sont émis qu'une fois → un spectateur qui rejoint en cours ne verrait **rien** | §5.3, §5.4, §6.4, §6.5 | **Corrigé + à implémenter au jalon 2** : émission à l'arrivée d'un spectateur, ou transmission hors flux vidéo |
| 4 | L'offre de connexion voyage **en clair** et a une forme de diffusion ; au jalon 1 l'opérateur du serveur verrait l'adresse publique de chaque émetteur | §2 (déc. 4 et 5), §5.1, §5.2 | **Inscrit comme décision D2**, non tranchée. Direction : annuaire de clés interrogeable **avant** production de l'offre |
| 5 | Le régulateur ne peut que sauter des images ; il ne pilote pas le débit | §6.1, §6.5, §7.5 | **Corrigé + à implémenter** : exposer `nvEncReconfigureEncoder` dans `sky-encode` |
| 6 | Le 4:4:4 n'existe pas sur AMD (aucune surface dans AMF, `AMF_INVALID_FORMAT` sur RDNA 3), non attesté sur Intel | §6.1, §6.4, §11 | **Corrigé + décision D1**, non tranchée. Prérequis : extraire un trait `VideoEncoder`, inexistant |

## Jalon 0 — décisions remontées au propriétaire, non tranchées

- **D1 — que promet-on aux utilisateurs non-NVIDIA ?** Arbitrage entre « sans compromis
  partout » (repli d'encodage hors NVENC, coût non mesuré) et « sans compromis sur NVIDIA » (4:2:0 ailleurs,
  dit dans l'interface). Voies décrites au §6.4 du spec. **Instruite ailleurs** :
  `docs/superpowers/notes/2026-08-23-compatibilite-toutes-cartes-graphiques.md` consigne
  quatre décisions de périmètre qu'elle présente comme déjà prises par le propriétaire, et
  recommande un spike dédié après le jalon 3 — note non vérifiée par la Tâche 9. Prérequis
  absolu : confirmer sur matériel AMD et Intel réel — tout le constat de l'écart 6 est
  documentaire.
- **D2 — comment restructurer le signaling pour que l'offre soit scellée ?** La direction
  est identifiée ; la conception appartient au jalon 1, qui construit la boîte aux lettres.

## Jalons
| # | Jalon | État |
|---|-------|------|
| 0 | Faisabilité (capture + encode + P2P) | **9/9 tâches faites — GO CONDITIONNEL, en attente de M4 et M1** |
| 1 | Fondations (Discord, amis, listes) | à faire — porte la décision **D2** (annuaire de clés) |
| 2 | Premier pixel (partage 1-à-1) | à faire — doit traiter les écarts **3** et **5**, et porte la décision **D1** |
| 3 | Qualité (simulcast, profils) | à faire — bloqué sur l'écart **5** (reconfiguration de débit à chaud) |
| 4 | Lecteur (multi-flux, zoom, audio) | à faire |
| 5 | Public (liens, salle d'attente) | à faire |
| 6 | Distribution (CI, installateurs) | à faire |
| 7 | Mac et Linux | à faire |
