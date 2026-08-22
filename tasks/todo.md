# TODO — SkyShare

## État actuel
- **Phase : brainstorming (design)** — aucun code écrit.
- Chemin : **architectural** (nouveau projet, 6 sous-systèmes).

## Décisions validées (cadrage)
| # | Sujet | Décision |
|---|-------|----------|
| 1 | Stack | Tauri (UI React) + cœur Rust natif — capture/encodage/transport natifs |
| 2 | Infra | **Zéro serveur.** Signaling via API routes Vercel + Neon. STUN publics. Pas de TURN. |
| 3 | Vie privée | Candidats ICE chiffrés E2E (Vercel/DB ne voient jamais d'IP en clair), IP jamais affichée ni loguée, IP locales masquées en mDNS `.local` |
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
7. [ ] **Exécution du jalon 0** ← en cours
   - [x] Tâche 3 : Encodage NVENC depuis texture D3D11 (Q2) — DONE. **Q2 = OUI** : NVENC accepte directement la texture D3D11 de la capture, en 4:4:4, sans copie CPU. Vérifié par `ffprobe` : `hevc`/`Rext`/`yuv444p`/2560x1440, 1105 images décodées = 1105 encodées. Mesure sur source synthétique (60,0 i/s, 2560x1440) : médiane 5,97 ms, p99 7,39 ms. Repli FFmpeg du Step 8 **non déclenché**. Latence sur écran réel non concluante (WGC ne délivre d'image que si l'écran change) ; qualité visuelle non validée — à juger par le propriétaire. Détail : `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/task-3-report.md`.
   - [x] Tâche 1 : Workspace et détection matérielle — DONE. `pick_best` testé TDD (5/5, GREEN) ; `probe_hardware` charge NVENC dynamiquement (`nvEncodeAPI64.dll` via `libloading`, comme FFmpeg/OBS — pas besoin du NVIDIA Video Codec SDK). `cargo run -p sky-probe -- hw` détecte réellement la RTX 4060 : choix partage d'écran = HEVC 4:4:4. Détail : `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/task-1-report.md`.
   - [x] Tâche 5 : Scellage des adresses réseau (`sky-crypto`) — DONE. TDD strict (RED confirmé sur `Identity` introuvable, GREEN 5/5). API réelle de `crypto_box` 0.9.1 diffère du brief (méthodes `PublicKey::seal`/`SecretKey::unseal`, pas de fonctions libres) ; surcoût de scellage confirmé à 48 octets (32 clé éphémère + 16 tag), inchangé. Détail : `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/task-5-6-report.md`.
   - [x] Tâche 6 : Contrôle de congestion à plancher garanti (`sky-net::pacer`, Q6) — DONE. TDD strict (RED confirmé sur `Pacer` introuvable, GREEN 5/5). Ne descend jamais sous le plancher même à 50 % de perte soutenue ; descente bornée à 15 %/tick ; remontée au plafond en 1 tick après un à-coup. `lib.rs` ne déclare que `pub mod pacer;` (pas `handshake`, réservé à la Tâche 7). Détail : `.superpowers/sdd/2026-08-22-jalon-0-faisabilite/task-5-6-report.md`.

## Jalons
| # | Jalon | État |
|---|-------|------|
| 0 | Faisabilité (capture + encode + P2P) | **plan prêt, 9 tâches** |
| 1 | Fondations (Discord, amis, listes) | à faire |
| 2 | Premier pixel (partage 1-à-1) | à faire |
| 3 | Qualité (simulcast, profils) | à faire |
| 4 | Lecteur (multi-flux, zoom, audio) | à faire |
| 5 | Public (liens, salle d'attente) | à faire |
| 6 | Distribution (CI, installateurs) | à faire |
| 7 | Mac et Linux | à faire |
