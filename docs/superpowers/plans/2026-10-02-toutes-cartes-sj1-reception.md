# Toutes cartes, sous-jalon 1 — recevoir sur n'importe quelle carte — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal :** un spectateur sans décodeur HEVC 4:4:4 (AMD, Intel, NVIDIA antérieure à Turing) reçoit et
affiche l'écran d'un hôte NVIDIA, en HEVC 4:2:0 ou en H.264, le format étant négocié à chaque
connexion.

**Architecture :** le spectateur sonde ce qu'il sait décoder et le liste dans son offre SDP ;
l'hôte y répond avec ce que NVENC sait encoder ; les deux côtés déduisent le même format par la
même fonction de `sky-net`. Le 4:4:4 reste à NVDEC ; tout le 4:2:0 passe par un moteur Media
Foundation neuf dans `sky-decode`, qui rend une tranche de texture D3D11 NV12 que `sky-rendu`
copie et convertit par un second nuanceur.

**Tech Stack :** Rust 1.94 (workspace `spike/`), `str0m` 0.23.1, `windows` 0.62 (Direct3D 11,
Media Foundation), NVENC via `nvidia-video-codec-sdk` 0.4, NVDEC via nvcuvid, Tauri 2 + React
(`app/`), Vitest.

**Spec :** `docs/superpowers/specs/2026-10-02-toutes-cartes-sj1-reception-design.md` (lire en
entier : §2 sépare le mesuré du supposé, §3 porte les décisions D1–D6).

**Branche :** `jalon-toutes-cartes` (dépôt `D:\skyshare`), partie de `jalon-2-premier-pixel`.

## Global Constraints

- **Français, accents compris**, dans le code, les commentaires, les messages de commit et les
  textes. Identifiants du domaine en français.
- **Jamais de décodage logiciel** (spec D5) : une sortie Media Foundation qui n'est pas un
  `IMFDXGIBuffer` est une erreur, jamais une image affichée.
- **`MF_LOW_LATENCY = 1`** sur le décodeur Media Foundation (spec D5, corrigée le 02/10/2026).
- **Énumération** des MFT : `MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_LOCALMFT | MFT_ENUM_FLAG_SORTANDFILTER`
  — **jamais** `MFT_ENUM_FLAG_HARDWARE`, qui ne rend aucun décodeur (mesuré, spec §2).
- **Ordre des formats, fixe et unique** : `Hevc444`, puis `Hevc420`, puis `H264`
  (`FormatVideo::PREFERENCE`). Types de charge : 102/103 (HEVC 4:4:4 et RTX), 104/105 (HEVC Main),
  106/107 (H.264).
- **Matrice couleur unique** : BT.601 **pleine plage** pour le 4:4:4 et le NV12 (spec D6).
- **Une unité d'accès entière par appel** de décodage, et l'image rendue est celle de cette unité,
  au même appel (contrat imité par `sky-partage/src/doublure.rs::DecodeurFactice`).
- **La garde d'affichage du spectateur (`cle_vue`) ne change pas** (`sky-partage/src/spectateur.rs`).
- **Aucune adresse IP** dans un texte, un événement ou une mesure. Jamais `RUST_LOG="str0m=debug"`,
  jamais de collecteur `tracing`.
- **Crate `windows` en version 0.62 partout** (une 0.61 traîne dans le lock via Tauri : ne jamais
  mélanger les types COM des deux).
- **Preuve par neutralisation** : chaque garde nouvelle est retirée une fois, le test qui la
  couvre doit rougir **seul et pour la bonne raison**, puis la garde est remise. Consigner chaque
  neutralisation dans le rapport de tâche. Avant tout commit :
  `cargo test --workspace --no-fail-fast` (depuis `spike/`) doit être entièrement vert — une
  neutralisation oubliée ne se voit qu'ainsi.
- **Les tests qui exigent le matériel paniquent** avec un message clair quand il manque (convention
  de `sky-decode/tests/aller_retour.rs`) ; ils ne se sautent pas en vert. La machine du
  propriétaire a une NVIDIA (RTX 4060) et l'extension HEVC.
- **Écriture des fichiers par les outils d'édition dédiés**, jamais par heredoc ni `sed` (piège
  d'antislash mangé, `CLAUDE.md`).
- `git add` des seuls fichiers touchés, **jamais `git add -A`** : `AGENTS.md`, `.claude/` et
  `testm4.md` restent non suivis. `spike/crates/sky-app/Cargo.toml` apparaît modifié après un
  `tauri build` (fins de ligne) : vérifier `git diff --ignore-cr-at-eol` vide et ne pas le commiter.
- Vérifications de fin de tâche, depuis `spike/` : `cargo test --workspace --no-fail-fast`,
  `cargo clippy --workspace --all-targets -- -D warnings`. Si `app/` est touché, depuis `app/` :
  `npx tsc --noEmit` et `npx vitest run`.
- Messages de commit : `type(portée): résumé` en français, terminés par
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Carte des fichiers

| Fichier | Tâche | Rôle |
|---|---|---|
| `spike/crates/sky-net/src/format.rs` (créé) | 1 | `FormatVideo`, table des types de charge, lecture d'un `PayloadParams` |
| `spike/crates/sky-net/src/link.rs` | 1 | `nouveau_rtc(formats)`, `offrant`/`repondant` paramétrés, `format_negocie`, envoi et réception multi-format |
| `spike/crates/sky-net/tests/piste_media.rs` | 1 | profils de la réponse SDP |
| `spike/crates/sky-encode/src/caps.rs`, `nvenc.rs` | 2 | `Codec::Hevc420`, sonde, profil Main |
| `spike/crates/sky-probe/src/cmd_encode.rs`, `cmd_codecs.rs` | 2 | `hevc420` en ligne de commande |
| `spike/crates/sky-partage/src/formats.rs` (créé) | 3, 7 | passages `Codec` ↔ `FormatVideo`, formats décodables |
| `spike/crates/sky-partage/src/hote.rs`, `evenement.rs` | 3 | hôte négociant son format |
| `spike/crates/sky-app/src/noyau.rs`, `materiel.rs`, `partage.rs` | 3, 5, 8 | formats encodables, messages, étiquette |
| `spike/crates/sky-probe/src/cmd_host.rs`, `cmd_view.rs`, `main.rs` | 3, 7 | option `--format` |
| `spike/crates/sky-decode/src/appareil.rs` (créé) | 4 | création unique du périphérique D3D11 |
| `spike/crates/sky-decode/src/sonde.rs` (créé) | 4 | `sonder_decodage` |
| `spike/crates/sky-decode/src/media_foundation.rs` (créé) | 4, 5 | MFT, DXVA, `DecodeurMf`, `ImageMf` |
| `spike/crates/sky-decode/src/source.rs` (créé) | 5 | `SourceImage` |
| `spike/crates/sky-decode/tests/media_foundation.rs` (créé) | 5 | décodage MF de flux NVENC réels |
| `spike/crates/sky-rendu/src/fenetre.rs`, `interop.rs`, `nuanceur.rs`, `image_de_test.rs`, `nv12.rs` (créé) | 4, 6 | affichage NV12 |
| `spike/crates/sky-rendu/tests/couleur.rs`, `tests/image_nv12.rs` (créé) | 6 | couleur et image réelle NV12 |
| `spike/crates/sky-partage/src/spectateur.rs`, `doublure.rs` | 6, 7 | sonde, offre, moteur choisi |
| `spike/crates/sky-app/src/vue.rs`, `app/src/types.ts`, `app/src/messages.ts`, `app/src/composants/PanneauPartage.tsx` et leurs tests | 8 | étiquette « Format », textes réécrits |
| `spike/scripts/version-portable.ps1` (créé), `spike/docs/essai-toutes-cartes.md` (créé), `CLAUDE.md`, `tasks/todo.md` | 9 | version portable, fiche d'essai, documentation |

## Dépendances entre tâches

1 → 3 ; 2 → 3 ; 4 → 5 → 6 → 7 ; 1, 3 → 7 ; 3, 7 → 8 ; tout → 9. L'ordre numérique les respecte.

---

### Tâche 1 : `sky-net` — formats vidéo négociables

**Files :**
- Create : `spike/crates/sky-net/src/format.rs`
- Modify : `spike/crates/sky-net/src/lib.rs` (déclarer et réexporter le module)
- Modify : `spike/crates/sky-net/src/link.rs` — constantes l.122-143, `LinkEvent` l.149-186,
  `offrant` l.~301, `repondant` l.~390, `poll` l.~689-703 (`Event::MediaData`), `ecrire_image`
  l.816-844, `nouveau_rtc` l.1013-1066, tests l.1182-1792
- Modify : `spike/crates/sky-net/tests/piste_media.rs`
- Modify (appelants, pour garder la compilation, comportement inchangé) :
  `spike/crates/sky-partage/src/hote.rs:282`, `spike/crates/sky-partage/src/spectateur.rs:634`,
  `spike/crates/sky-partage/src/doublure.rs` (construction de `LinkEvent::Image`), et tout autre
  `PeerLink::offrant` / `PeerLink::repondant` / `LinkEvent::Image {` trouvé par
  `grep -rn "offrant(\|repondant(\|LinkEvent::Image" spike/crates`.

**Interfaces :**
- Consumes : rien de neuf.
- Produces :
  ```rust
  // sky_net::FormatVideo (réexporté à la racine du crate)
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
  pub enum FormatVideo { Hevc444, Hevc420, H264 }
  impl FormatVideo {
      pub const PREFERENCE: [FormatVideo; 3];
      pub fn libelle(self) -> &'static str;          // "HEVC 4:4:4" | "HEVC 4:2:0" | "H.264"
      pub fn type_de_charge(self) -> u8;             // 102 | 104 | 106
      pub fn retransmission(self) -> u8;             // 103 | 105 | 107
      pub fn depuis_parametres(p: &str0m::format::PayloadParams) -> Option<FormatVideo>;
  }
  impl PeerLink {
      pub fn offrant(identity: Identity, formats: &[FormatVideo]) -> anyhow::Result<(Self, String)>;
      pub fn repondant(identity: Identity, offre_texte: &str, formats: &[FormatVideo]) -> anyhow::Result<(Self, String)>;
      pub fn format_negocie(&self) -> Option<FormatVideo>;
  }
  // LinkEvent::Image gagne un champ :
  Image { donnees: Vec<u8>, horodatage_ms: u64, cle: bool, sans_perte: bool, format: Option<FormatVideo> }
  ```

- [ ] **Étape 1 : écrire `format.rs` et ses tests unitaires**

```rust
//! Les formats vidéo que SkyShare sait transporter, et leur écriture SDP.
//!
//! Une seule table pour les deux côtés : le spectateur y prend ce qu'il sait
//! décoder, l'hôte ce qu'il sait encoder, et `PeerLink::format_negocie` en
//! déduit le même format des deux côtés (spec §4, `sky-net`).

use str0m::format::{Codec, PayloadParams};

/// Profil HEVC « Format Range Extensions » (`profile-id=4`), où vit le 4:4:4.
pub(crate) const PROFIL_HEVC_444: u8 = 4;
/// Profil HEVC « Main » (`profile-id=1`) : 8 bits, 4:2:0.
pub(crate) const PROFIL_HEVC_MAIN: u8 = 1;
/// Palier « Main » (`tier-flag=0`).
pub(crate) const TIER_MAIN: u8 = 0;
/// Niveau HEVC 6.0 : `level_id = (6 * 10 + 0) * 3 = 180` (ITU-T H.265 Annexe A).
pub(crate) const NIVEAU_HEVC_6_0: u8 = 180;
/// `profile-level-id` H.264 : profil High (0x64), sans contrainte (0x00),
/// niveau 5.2 (0x34). High est le profil que NVENC produit
/// (`NV_ENC_H264_PROFILE_HIGH_GUID`) ; 5.2 couvre 2560×1440 à 60 images/s.
/// `str0m` exige l'égalité du PROFIL ; un écart de niveau ne fait que baisser
/// le score de concordance (lu dans `payload_params.rs`, `match_h264_score`).
pub(crate) const PROFIL_NIVEAU_H264: u32 = 0x64_00_34;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatVideo {
    /// HEVC 4:4:4, décodé par NVDEC.
    Hevc444,
    /// HEVC Main 4:2:0, décodé par Media Foundation.
    Hevc420,
    /// H.264 High 4:2:0, décodé par Media Foundation.
    H264,
}

impl FormatVideo {
    /// L'ordre de préférence, le même pour les deux côtés. Le 4:4:4 d'abord :
    /// c'est la couleur pleine résolution, la raison d'être du projet.
    pub const PREFERENCE: [FormatVideo; 3] = [FormatVideo::Hevc444, FormatVideo::Hevc420, FormatVideo::H264];

    /// Le nom montré à l'utilisateur dans les mesures.
    pub fn libelle(self) -> &'static str {
        match self {
            FormatVideo::Hevc444 => "HEVC 4:4:4",
            FormatVideo::Hevc420 => "HEVC 4:2:0",
            FormatVideo::H264 => "H.264",
        }
    }

    /// Type de charge RTP. Fixe et propre à chaque format : l'hôte adopte ceux de
    /// l'offre (`str0m`, réponse à un offrant `RecvOnly`), et les deux côtés sont
    /// SkyShare — la table suffit donc à relire un type de charge retenu.
    pub fn type_de_charge(self) -> u8 {
        match self {
            FormatVideo::Hevc444 => 102,
            FormatVideo::Hevc420 => 104,
            FormatVideo::H264 => 106,
        }
    }

    /// Type de charge des retransmissions (RTX).
    pub fn retransmission(self) -> u8 {
        self.type_de_charge() + 1
    }

    /// Le format que décrivent des paramètres négociés, s'il est l'un des nôtres.
    pub fn depuis_parametres(p: &PayloadParams) -> Option<FormatVideo> {
        let spec = p.spec();
        match spec.codec {
            Codec::H264 => Some(FormatVideo::H264),
            Codec::H265 => match spec.format.h265_profile_tier_level.map(|ptl| ptl.profile_id()) {
                Some(PROFIL_HEVC_444) => Some(FormatVideo::Hevc444),
                Some(PROFIL_HEVC_MAIN) => Some(FormatVideo::Hevc420),
                _ => None,
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_types_de_charge_sont_distincts_et_suivis_de_leur_retransmission() {
        let mut vus = std::collections::HashSet::new();
        for f in FormatVideo::PREFERENCE {
            assert!(vus.insert(f.type_de_charge()), "{f:?} : type de charge en double");
            assert!(vus.insert(f.retransmission()), "{f:?} : retransmission en double");
        }
    }

    #[test]
    fn la_preference_commence_par_la_couleur_pleine_resolution() {
        assert_eq!(FormatVideo::PREFERENCE[0], FormatVideo::Hevc444);
        assert_eq!(FormatVideo::PREFERENCE.len(), 3);
    }
}
```

Vérifier, en lisant le source de `str0m` 0.23.1
(`C:\Users\killi\.cargo\registry\src\index.crates.io-*\str0m-0.23.1\src\format\`), que
`H265ProfileTierLevel::profile_id()` existe et rend un `u8` ; sinon adapter la comparaison au type
réel, sans changer la sémantique. Déclarer `mod format;` et `pub use format::FormatVideo;` dans
`lib.rs`.

- [ ] **Étape 2 : écrire les tests de négociation (rouges)** dans le module `tests` de `link.rs`,
  à côté de `paire_avec_piste`. Ajouter une aide paramétrée et cinq tests :

```rust
/// `paire_avec_piste`, avec des listes de formats choisies de chaque côté.
fn paire_avec_formats(spectateur: &[FormatVideo], hote: &[FormatVideo]) -> (PeerLink, PeerLink) {
    let (mut s, offre) = PeerLink::offrant(Identity::generate(), spectateur).unwrap();
    let (mut h, reponse) = PeerLink::repondant(Identity::generate(), &offre, hote).unwrap();
    s.accepter_reponse(&reponse).unwrap();
    let limite = Instant::now() + Duration::from_secs(10);
    while Instant::now() < limite {
        let _ = s.poll();
        let _ = h.poll();
        if s.canal_ouvert() && h.canal_ouvert() && h.piste_ouverte() {
            return (h, s);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("le canal ou la piste ne se sont pas ouverts dans les 10 s");
}

#[test]
fn un_spectateur_h264_seul_obtient_h264_des_deux_cotes() {
    let (mut hote, mut spectateur) = paire_avec_formats(&[FormatVideo::H264], &FormatVideo::PREFERENCE);
    assert_eq!(hote.format_negocie(), Some(FormatVideo::H264));
    assert_eq!(spectateur.format_negocie(), Some(FormatVideo::H264));
    // Et le paquet arrive bien étiqueté H.264, et reconnu comme image clé :
    // les deux côtés s'accordent.
    hote.ecrire_image(UNITE_H264_IDR, 7).unwrap();
    let evenements = pomper_jusqu_a(&mut spectateur, &mut hote, Duration::from_secs(5));
    let (format, cle) = evenements
        .iter()
        .find_map(|e| match e {
            LinkEvent::Image { format, cle, .. } => Some((*format, *cle)),
            _ => None,
        })
        .expect("aucune image reçue");
    assert_eq!(format, Some(FormatVideo::H264));
    assert!(cle, "une IDR H.264 doit être vue comme image clé");
}

#[test]
fn deux_cotes_complets_s_accordent_sur_le_444() {
    let (hote, spectateur) = paire_avec_formats(&FormatVideo::PREFERENCE, &FormatVideo::PREFERENCE);
    assert_eq!(hote.format_negocie(), Some(FormatVideo::Hevc444));
    assert_eq!(spectateur.format_negocie(), Some(FormatVideo::Hevc444));
}

#[test]
fn un_hote_sans_444_et_un_spectateur_complet_s_accordent_sur_le_hevc_420() {
    let (hote, spectateur) =
        paire_avec_formats(&FormatVideo::PREFERENCE, &[FormatVideo::Hevc420, FormatVideo::H264]);
    assert_eq!(hote.format_negocie(), Some(FormatVideo::Hevc420));
    assert_eq!(spectateur.format_negocie(), Some(FormatVideo::Hevc420));
}

#[test]
fn des_listes_disjointes_ne_negocient_aucun_format() {
    let (_, offre) = PeerLink::offrant(Identity::generate(), &[FormatVideo::Hevc444]).unwrap();
    let (mut hote, _) = PeerLink::repondant(Identity::generate(), &offre, &[FormatVideo::H264]).unwrap();
    let _ = hote.poll();
    assert_eq!(hote.format_negocie(), None);
    assert_eq!(hote.ecrire_image(b"rien", 0), Err(ErreurEnvoi::PisteFermee));
}

#[test]
fn offrir_une_liste_vide_est_refuse() {
    assert!(PeerLink::offrant(Identity::generate(), &[]).is_err());
}
```

`UNITE_H264_IDR` : une constante d'octets Annex-B minimale, SPS (`00 00 00 01 67 …`), PPS
(`00 00 00 01 68 …`) et IDR (`00 00 00 01 65 …`) — le dépaquetiseur RFC 6184 de `str0m` ne lit que
les en-têtes NAL, l'unité n'a pas à être décodable. Si le dépaquetiseur ne la rend pas comme
image clé, lire `str0m-0.23.1/src/packet/h264.rs` pour savoir ce qu'il exige, ajuster la
constante, et noter le constat dans le rapport.

- [ ] **Étape 3 : lancer, constater l'échec de compilation**

Run : `cargo test -p sky-net` (depuis `spike/`). Attendu : erreurs de compilation (`offrant` prend
un argument, `format_negocie` inconnue, champ `format` inconnu).

- [ ] **Étape 4 : implémenter dans `link.rs`**

1. Retirer `PROFIL_MAIN_444`, `TIER_MAIN`, `NIVEAU_6_0`, `PAYLOAD_TYPE_H265`, `RTX_H265` (l.122-143)
   au profit de `crate::format` ; garder le commentaire de `PROFIL_MAIN_444` (pourquoi le profil
   est verrouillé par un test sur la RÉPONSE) en tête de `nouveau_rtc`.
2. `nouveau_rtc(formats: &[FormatVideo]) -> (Rtc, Instant)` : après `codecs.clear()`, pour chaque
   format de `formats` **dans l'ordre reçu** :
   ```rust
   for format in formats {
       match format {
           FormatVideo::Hevc444 => codecs.add_h265(
               format.type_de_charge().into(), Some(format.retransmission().into()),
               PROFIL_HEVC_444, TIER_MAIN, NIVEAU_HEVC_6_0),
           FormatVideo::Hevc420 => codecs.add_h265(
               format.type_de_charge().into(), Some(format.retransmission().into()),
               PROFIL_HEVC_MAIN, TIER_MAIN, NIVEAU_HEVC_6_0),
           FormatVideo::H264 => codecs.add_h264(
               format.type_de_charge().into(), Some(format.retransmission().into()),
               true, PROFIL_NIVEAU_H264),
       }
   }
   ```
   Commenter : **une seule entrée par famille concordable** — deux entrées locales qui concordent
   avec le même type de charge distant font paniquer `str0m` (`assert_claim_once`, « Pt locked
   multiple times ») ; HEVC 4:4:4 et HEVC Main ne concordent jamais entre eux (profil exact
   exigé), d'où deux entrées H265 sûres, et une seule H264. Réécrire le commentaire « Un seul
   codec annoncé » (devenu faux) : la raison d'honnêteté demeure (n'annoncer que ce qu'on sait
   produire ou lire), la raison de taille aussi (étape 6).
3. `offrant(identity, formats)` : `anyhow::ensure!(!formats.is_empty(), "aucun format vidéo à
   annoncer")` en tête, puis `nouveau_rtc(formats)`.
4. `repondant(identity, offre_texte, formats)` : même garde, `nouveau_rtc(formats)`.
5. `format_negocie` :
   ```rust
   /// Le format que les deux côtés utilisent : le premier de
   /// `FormatVideo::PREFERENCE` dont le type de charge a été retenu par la
   /// négociation. Même fonction des deux côtés, donc même réponse, sans
   /// dépendre de l'ordre de la réponse SDP (spec §4).
   pub fn format_negocie(&self) -> Option<FormatVideo> {
       let media = self.rtc.media(self.piste?)?;
       let retenus = media.remote_pts();
       FormatVideo::PREFERENCE
           .into_iter()
           .find(|f| retenus.contains(&f.type_de_charge().into()))
   }
   ```
   Vérifier dans le source de `str0m` que `Rtc::media` et `Media::remote_pts` sont publics
   (`lib.rs:1478`, `media/mod.rs:542`). Côté spectateur, `piste` est connue dès `offrant` ; côté
   hôte, après `Event::MediaAdded`.
6. `ecrire_image` : remplacer la recherche `Codec::H265` par
   ```rust
   let format = self.format_negocie().ok_or(ErreurEnvoi::CodecNonNegocie)?;
   let pt = writer
       .payload_params()
       .map(|params| params.pt())
       .find(|pt| *pt == format.type_de_charge().into())
       .ok_or(ErreurEnvoi::CodecNonNegocie)?;
   ```
   (`format_negocie` emprunte `self.rtc` en lecture : la calculer **avant** `self.rtc.writer(mid)`.)
   Mettre à jour la doc de `ErreurEnvoi::CodecNonNegocie` (« aucun type de charge H265 » → « le
   format négocié n'est pas parmi les types de charge du rédacteur »).
7. `poll`, `Event::MediaData` : la clé devient
   ```rust
   let cle = matches!(donnees.codec_extra, CodecExtra::H265(extra) if extra.is_keyframe)
       || matches!(donnees.codec_extra, CodecExtra::H264(extra) if extra.is_keyframe);
   ```
   et `LinkEvent::Image` reçoit `format: FormatVideo::depuis_parametres(&donnees.params)`.
   Documenter le champ dans `LinkEvent` : « le format lu sur le paquet ; il sert à prouver que les
   deux côtés s'accordent (tests) ; le spectateur choisit son décodeur par `format_negocie` ».
8. Mettre à jour la doc de `LinkEvent::Image` (« Une unité d'accès HEVC » → « Une unité d'accès
   vidéo », dépaquetiseur RFC 7798 **ou RFC 6184**, « Image clé, au sens du dépaquetiseur »).

- [ ] **Étape 5 : adapter les appelants et les tests existants**

- Appelants hors `sky-net` : passer `&[FormatVideo::Hevc444]` partout, **comportement inchangé**
  (les tâches 3 et 7 y mettront les vraies listes). `LinkEvent::Image { .. }` construits dans les
  doublures : ajouter `format: Some(FormatVideo::Hevc444)`.
- Tests de `link.rs` : `PeerLink::offrant(x)` → `PeerLink::offrant(x, &FormatVideo::PREFERENCE)` et
  `repondant(x, &offre)` → `repondant(x, &offre, &FormatVideo::PREFERENCE)` ;
  `paire_connectee`/`paire_avec_piste` aussi. Le test d'unité HEVC 4:4:4 (`une_unite_d_acces_
  traverse_la_piste_intacte`) doit continuer de passer : c'est le 4:4:4 qui est négocié.
- `un_tier_flag_divergent_retire_h265_de_la_reponse` : construire l'offre **et** la réponse avec
  `&[FormatVideo::Hevc444]` (un seul format) pour garder ce qu'il prouve (la ligne média disparaît).
- `tests/piste_media.rs` : `offre_de_spectateur` passe `&FormatVideo::PREFERENCE`, l'hôte aussi ;
  remplacer `ligne_fmtp_h265` (qui prenait le PREMIER `H265/`, ambigu à deux entrées) par une
  aide qui rend la ligne `a=fmtp:` d'un type de charge donné, et vérifier trois choses dans la
  réponse : `a=fmtp:102` contient `profile-id=4`, `a=fmtp:104` contient `profile-id=1`,
  `a=fmtp:106` contient `packetization-mode=1` et `profile-level-id=640034`. Retirer la constante
  dupliquée `PROFIL_MAIN_444` du fichier de test.

- [ ] **Étape 6 : mesurer la taille des blocs et recalibrer la borne**

Run : `cargo test -p sky-net reelle -- --nocapture`. Relever les longueurs imprimées
(`offre reelle : N caracteres`, `reponse reelle : N caracteres`) sur **six** exécutions, et la
longueur d'une offre **non comprimée** (test temporaire qui imprime la longueur du bloc
`SKY2:` construit sur le SDP brut au lieu du SDP comprimé ; le retirer ensuite). Fixer
`BORNE_BLOC_REEL` à ≈ 1,2 × le plus grand bloc mesuré, **en restant sous le plus petit bloc non
comprimé** (sinon la borne ne garde plus la compression), et réécrire son commentaire avec les
nouvelles valeurs mesurées et la date. Vérifier que le plus grand bloc reste sous
`sky_compte::TAILLE_MAX_CLAIR` (4048) ; si ce n'est pas le cas, **arrêter et rendre compte**
(BLOCKED) : la spec suppose que trois codecs tiennent dans l'enveloppe.

- [ ] **Étape 7 : lancer, constater le vert ; neutraliser**

Run : `cargo test -p sky-net`. Puis, une à la fois :
- `format_negocie` rendant toujours `Some(FormatVideo::Hevc444)` → `un_spectateur_h264_seul_...`
  et `un_hote_sans_444_...` rougissent ;
- la branche `CodecExtra::H264` de la clé retirée → `un_spectateur_h264_seul_...` rougit sur
  l'assertion `cle` ;
- `nouveau_rtc` ignorant `formats` (liste fixe `PREFERENCE`) → `des_listes_disjointes_...` rougit.

- [ ] **Étape 8 : tout le workspace, clippy, commit**

Run : `cargo test --workspace --no-fail-fast` puis `cargo clippy --workspace --all-targets -- -D warnings`.

```bash
git add spike/crates/sky-net/src/format.rs spike/crates/sky-net/src/lib.rs spike/crates/sky-net/src/link.rs spike/crates/sky-net/tests/piste_media.rs <appelants modifiés, un par un>
git commit -m "feat(sky-net): formats vidéo négociables, HEVC 4:4:4, HEVC 4:2:0 et H.264"
```

---

### Tâche 2 : `sky-encode` — HEVC Main 4:2:0

**Files :**
- Modify : `spike/crates/sky-encode/src/caps.rs` (enum l.7-12, `label`, `is_444`, `pick_best`
  l.40-55, sonde l.165-184, tests l.189+)
- Modify : `spike/crates/sky-encode/src/nvenc.rs` (imports l.31-47, `parametres_codec` l.609-619,
  tests l.621-821)
- Modify : `spike/crates/sky-probe/src/cmd_encode.rs` (`parse_codec` l.42-56),
  `spike/crates/sky-probe/src/cmd_codecs.rs` (`combinaisons` l.42-47), `main.rs` (texte d'aide
  `--codec`)

**Interfaces :**
- Produces : `sky_encode::Codec::Hevc420` (label `"HEVC 4:2:0"`, `is_444() == false`).

- [ ] **Étape 1 : tests rouges dans `caps.rs`**

```rust
#[test]
fn une_carte_hevc_annonce_aussi_le_hevc_420() {
    // La sonde pousse Hevc420 dès que le GUID HEVC est présent, avant Hevc444.
    let codecs = codecs_annonces(&[(NV_ENC_CODEC_HEVC_GUID, true)]);
    assert!(codecs.contains(&Codec::Hevc420));
    assert!(codecs.contains(&Codec::Hevc444));
}

#[test]
fn une_carte_hevc_sans_444_annonce_le_hevc_420_seul() {
    let codecs = codecs_annonces(&[(NV_ENC_CODEC_HEVC_GUID, false)]);
    assert_eq!(codecs, vec![Codec::Hevc420]);
}

#[test]
fn le_hevc_420_n_est_pas_de_la_couleur_pleine_resolution() {
    assert!(!Codec::Hevc420.is_444());
    assert_eq!(Codec::Hevc420.label(), "HEVC 4:2:0");
}
```

`codecs_annonces(&[(GUID, a_444: bool)]) -> Vec<Codec>` est une fonction pure à **extraire** de la
boucle de `probe_hardware` (l.165-184), qui reçoit la liste `(GUID, présence d'un format d'entrée
4:4:4)` déjà calculée et rend les codecs dans l'ordre de la boucle actuelle. La boucle de
`probe_hardware` l'appelle : c'est elle que les tests exercent, pas une copie. Règles :
H.264 → `H264_420`, puis `H264_444` si `a_444` ; HEVC → `Hevc420`, puis `Hevc444` si `a_444` ;
AV1 → `Av1_420`.

- [ ] **Étape 2 : lancer — échec de compilation (`Hevc420`, `codecs_annonces` inconnus).**

Run : `cargo test -p sky-encode`

- [ ] **Étape 3 : implémenter**

- `Codec::Hevc420` ; `label` → `"HEVC 4:2:0"` ; `is_444` → `false`.
- `pick_best` : `Hevc420` se range **après** `Hevc444` et `H264_444` et **avant** `H264_420`
  dans les deux ordres (texte et vidéo). Ajouter un test `pick_best` qui le prouve.
- `parametres_codec` : `Codec::Hevc420 => (NV_ENC_CODEC_HEVC_GUID, NV_ENC_HEVC_PROFILE_MAIN_GUID, 1)`.
  Vérifier que `NV_ENC_HEVC_PROFILE_MAIN_GUID` existe dans
  `nvidia_video_codec_sdk::sys::nvEncodeAPI` (chercher dans le source du crate sous
  `C:\Users\killi\.cargo\registry\src\`) ; sinon le déclarer **à partir de l'en-tête
  `nvEncodeAPI.h` embarqué par ce crate**, jamais de mémoire, et citer le chemin de l'en-tête
  en commentaire. Le bloc HEVC d'`initialiser` (l.226-237) sert tel quel : il lit
  `chroma_format_idc`.
- `parse_codec` : `"hevc420" | "h265420"` → `Hevc420` ; message d'erreur mis à jour
  (`h264420, h264444, hevc420, hevc444, av1420`). Test dans `cmd_encode.rs`.
- `cmd_codecs::combinaisons` : ajouter `(Codec::Hevc420, "cmp-hevc-420.h265")` ; mettre à jour le
  texte qui annonce « 4 codecs ».

- [ ] **Étape 4 : tests matériels de NVENC généralisés**

Dans `nvenc.rs`, l'aide `encodeur_de_test()` est figée sur `Hevc444` et se saute en vert sans
NVIDIA. La rendre paramétrée : `encodeur_de_test(codec: Codec) -> Option<NvencEncoder>` (même
comportement de saut, c'est l'existant de ce fichier ; ne pas l'étendre ailleurs), et ajouter :

- `le_hevc_420_produit_un_flux_main_avec_ses_entetes` : `entetes_de_sequence()` contient les NAL
  32, 33, 34 (VPS, SPS, PPS) ; **et** le SPS porte `general_profile_idc == 1` (Main). Dans le SPS
  HEVC, après l'en-tête NAL de 2 octets, l'octet suivant porte
  `sps_video_parameter_set_id(4) | sps_max_sub_layers_minus1(3) | temporal_id_nesting(1)`, puis
  l'octet d'après `general_profile_space(2) | general_tier_flag(1) | general_profile_idc(5)` :
  `profile_idc = octet & 0x1f`. (Les octets d'émulation `00 00 03` ne peuvent pas apparaître
  avant ces deux octets.)
- `le_h264_force_une_image_cle_avec_ses_entetes` : sur le modèle exact de
  `forcer_une_image_cle_produit_un_idr_a_l_image_suivante` (l.773) avec `Codec::H264_420` et une
  aide `types_de_nal_h264` (type = `octet & 0x1f` après chaque code de départ) : l'image qui suit
  `forcer_image_cle` contient 5 (IDR), 7 (SPS) et 8 (PPS) — `OUTPUT_SPSPPS`, nvenc.rs:462.

- [ ] **Étape 5 : vert, neutralisation, workspace, commit**

Neutraliser : `codecs_annonces` sans `Hevc420` → les deux tests de sonde rougissent ;
`parametres_codec` rendant le profil FREXT et chroma 3 pour `Hevc420` → `le_hevc_420_...` rougit
sur `general_profile_idc` (4 au lieu de 1).

Run : `cargo test --workspace --no-fail-fast`, clippy.

```bash
git add spike/crates/sky-encode/src/caps.rs spike/crates/sky-encode/src/nvenc.rs spike/crates/sky-probe/src/cmd_encode.rs spike/crates/sky-probe/src/cmd_codecs.rs spike/crates/sky-probe/src/main.rs
git commit -m "feat(sky-encode): HEVC Main 4:2:0, sondé et encodable"
```

---

### Tâche 3 : l'hôte négocie son format

**Files :**
- Create : `spike/crates/sky-partage/src/formats.rs`
- Modify : `spike/crates/sky-partage/src/lib.rs` (module ; réexporter `sky_net::FormatVideo`,
  `formats::{formats_encodables, codec_de}`)
- Modify : `spike/crates/sky-partage/src/hote.rs` (`ParametresHote` l.225, garde l.246-255,
  `repondant` l.282, après `etablir` l.324-333, `diffuser` l.380-425, tests l.1366-1440)
- Modify : `spike/crates/sky-partage/src/evenement.rs` (`Fin` l.128, `Evenement` l.12)
- Modify : `spike/crates/sky-app/src/materiel.rs` (l.40-47 et tests), `noyau.rs` (l.67-77,
  `definir_nvenc` l.1134, `partager` l.1156-1211, tests l.2517-2728), `partage.rs`
  (`Partageur::heberger` l.32, `PartageurReel` l.57-83, `fin_vue` l.120-141), `lib.rs:93`
- Modify : `spike/crates/sky-probe/src/cmd_host.rs` (l.43-130), `cmd_view.rs` (match des fins et
  des événements), `main.rs` (`Cmd::Host`)

**Interfaces :**
- Consumes : `sky_net::FormatVideo` (tâche 1), `PeerLink::repondant(.., formats)`,
  `PeerLink::format_negocie()` ; `sky_encode::Codec::Hevc420` (tâche 2).
- Produces :
  ```rust
  // sky_partage::formats
  pub fn formats_encodables(codecs: &[Codec]) -> Vec<FormatVideo>;  // dans l'ordre PREFERENCE
  pub fn codec_de(format: FormatVideo) -> Codec;
  // sky_partage
  pub struct ParametresHote { pub formats: Vec<FormatVideo>, /* le reste inchangé, `codec` retiré */ }
  pub enum Fin { /* CodecNonTransmissible { codec } retiré */ AucunFormatEncodable, AucunFormatCommun, .. }
  pub enum Evenement { .., Format(FormatVideo) }
  // sky_app::noyau
  pub const MESSAGE_AUCUN_FORMAT: &str;
  pub const MESSAGE_AUCUN_FORMAT_COMMUN: &str;
  // sky_probe::cmd_host
  pub(crate) fn format_depuis_texte(texte: &str) -> anyhow::Result<FormatVideo>;
  ```

- [ ] **Étape 1 : `formats.rs` et ses tests**

```rust
//! Passages entre ce que les cartes savent faire et ce que la piste transporte.
//!
//! Un seul endroit, pour l'hôte comme pour le spectateur : deux traductions
//! écrites à deux endroits finiraient par diverger.

use sky_encode::Codec;
use sky_net::FormatVideo;

/// Les formats transmissibles parmi ce que l'encodeur sait produire, dans
/// l'ordre de préférence commun. `H264_444` et `Av1_420` n'y figurent pas :
/// aucune piste ne les négocie.
pub fn formats_encodables(codecs: &[Codec]) -> Vec<FormatVideo> {
    FormatVideo::PREFERENCE
        .into_iter()
        .filter(|f| codecs.contains(&codec_de(*f)))
        .collect()
}

/// Le réglage NVENC qui produit un format.
pub fn codec_de(format: FormatVideo) -> Codec {
    match format {
        FormatVideo::Hevc444 => Codec::Hevc444,
        FormatVideo::Hevc420 => Codec::Hevc420,
        FormatVideo::H264 => Codec::H264_420,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_rtx_4060_transmet_les_trois_formats_dans_l_ordre() {
        let codecs = [Codec::H264_420, Codec::H264_444, Codec::Hevc420, Codec::Hevc444, Codec::Av1_420];
        assert_eq!(
            formats_encodables(&codecs),
            vec![FormatVideo::Hevc444, FormatVideo::Hevc420, FormatVideo::H264]
        );
    }

    #[test]
    fn une_carte_sans_hevc_transmet_le_h264() {
        assert_eq!(formats_encodables(&[Codec::H264_420]), vec![FormatVideo::H264]);
    }

    #[test]
    fn h264_444_et_av1_ne_sont_pas_transmissibles() {
        assert!(formats_encodables(&[Codec::H264_444, Codec::Av1_420]).is_empty());
    }
}
```

- [ ] **Étape 2 : l'hôte — tests rouges dans `hote.rs`**

Remplacer `un_codec_autre_que_hevc_444_est_refuse_avant_tout_reseau` (l.1366) par
`une_liste_de_formats_vide_est_refusee_avant_tout_reseau` : mêmes vérifications (0
synchronisation, 0 événement), avec `ParametresHote { formats: vec![], .. }` et
`Fin::AucunFormatEncodable` attendu. Adapter `un_arret_pendant_l_attente_...` (l.1406-1440) à
`formats: vec![FormatVideo::Hevc444]`.

- [ ] **Étape 3 : implémenter l'hôte**

- `ParametresHote.codec` → `formats: Vec<FormatVideo>`. La garde l.246-255 devient :
  ```rust
  // Le contrat de format, tenu ICI et pas seulement chez l'appelant : sans
  // aucun format transmissible, rien ne partirait que le spectateur sache lire.
  // `sky-app` refuse aussi en amont (`Noyau::partager`), pour son message ;
  // cette garde-ci protège tout appelant, présent ou futur. Les deux ne sont
  // pas redondantes.
  if p.formats.is_empty() {
      return Ok(Fin::AucunFormatEncodable);
  }
  ```
- l.282 : `PeerLink::repondant(Identity::generate(), &offre.texte, &p.formats)`.
- Après `evenements(Evenement::Connecte { .. })` (l.331), **avant** `en_annoncant_l_arret` :
  ```rust
  // Aucun format commun : `str0m` a écarté la ligne média, le canal de données
  // vit seul. Le spectateur fait le même constat de son côté (même fonction,
  // `PeerLink::format_negocie`) : aucune annonce n'est nécessaire.
  let Some(format) = link.format_negocie() else {
      return Ok(Fin::AucunFormatCommun);
  };
  evenements(Evenement::Format(format));
  ```
  et passer `format` à `diffuser` (nouveau paramètre), qui crée
  `NvencEncoder::new(cap.d3d_device(), codec_de(format), ..)` et émet
  `Evenement::Diffusion { codec: codec_de(format), .. }`.
- `Fin::CodecNonTransmissible { codec }` → `Fin::AucunFormatEncodable` ; ajouter
  `Fin::AucunFormatCommun` ; `Evenement::Format(FormatVideo)`. Documenter chaque variante.
- Tout `match` exhaustif sur `Fin` ou `Evenement`
  (`grep -rn "CodecNonTransmissible\|Evenement::Diffusion\|Fin::" spike/crates`) est mis à jour :
  `sky-probe/src/cmd_view.rs`, `cmd_host.rs`, `sky-app/src/partage.rs`.

- [ ] **Étape 4 : `sky-app`**

- `materiel::detecter_nvenc(sonde) -> Vec<Codec>` : `Ok(Ok(caps)) => caps.codecs`, sinon `vec![]`
  (garder `catch_unwind`). Adapter ses deux tests : `une_sonde_qui_panique_rend_aucune_carte`
  attend `vec![]` ; `une_carte_hevc_444_est_retenue_pour_le_texte` devient
  `les_codecs_de_la_carte_sont_tous_retenus`.
- `Donnees.nvenc: Option<Codec>` → `Vec<Codec>` ; `definir_nvenc(codecs: Vec<Codec>)` ;
  `Instantane.nvenc = !codecs.is_empty()` (sens inchangé : « carte NVIDIA utilisable »).
- `MESSAGE_SANS_HEVC_444` est **retiré** ; à sa place :
  ```rust
  /// TEXTE NOUVEAU, à valider par le propriétaire (spec §6).
  pub const MESSAGE_AUCUN_FORMAT: &str = "Partage impossible : la carte graphique de cette machine \
      n'encode aucun des formats que SkyShare sait transmettre (HEVC ou H.264).";
  /// TEXTE NOUVEAU, à valider par le propriétaire (spec §6).
  pub const MESSAGE_AUCUN_FORMAT_COMMUN: &str = "Aucun format vidéo en commun : la carte \
      graphique qui partage n'encode aucun format que celle qui regarde sait décoder.";
  ```
- `Noyau::partager` (l.1167-1175) :
  ```rust
  if d.nvenc.is_empty() {
      return Err(MESSAGE_SANS_NVIDIA.to_string());
  }
  let formats = sky_partage::formats_encodables(&d.nvenc);
  if formats.is_empty() {
      return Err(MESSAGE_AUCUN_FORMAT.to_string());
  }
  ```
  puis `partageur.heberger(&noyau, formats, ecran, &arret, ..)`.
- `Partageur::heberger(&self, noyau: &Noyau, formats: Vec<FormatVideo>, ecran: usize, ..)` et
  `PartageurReel` : `ParametresHote { formats, .. }`. Les doublures de `Partageur` dans les tests
  suivent.
- `fin_vue` : `Fin::AucunFormatEncodable => FinVue::Autre { message: MESSAGE_AUCUN_FORMAT.into() }`,
  `Fin::AucunFormatCommun => FinVue::Autre { message: MESSAGE_AUCUN_FORMAT_COMMUN.into() }`.
- Tests de `noyau.rs` : `definir_nvenc(Some(Codec::Hevc444))` → `definir_nvenc(vec![Codec::Hevc444])`.
  `une_carte_sans_hevc_444_ne_partage_pas_un_flux_illisible` (l.2547) devient
  `une_carte_sans_format_transmissible_ne_partage_pas` (boucle sur `vec![H264_444]` et
  `vec![Av1_420]`, refus `MESSAGE_AUCUN_FORMAT`) **plus** un test
  `une_carte_h264_seule_partage_desormais` qui vérifie que `partager` ne refuse pas avec
  `vec![Codec::H264_420]` (même montage que les tests voisins qui autorisent le partage).
- `lib.rs:93` : `noyau.definir_nvenc(materiel::detecter_nvenc(sky_encode::probe_hardware))` reste,
  le type suit.

- [ ] **Étape 5 : `sky-probe host --format`**

Remplacer l'option `--codec` de `Cmd::Host` par
`#[arg(long, default_value = "auto")] format: String` (aide : `auto | hevc444 | hevc420 | h264`).
Dans `cmd_host.rs` :
```rust
let caps = sky_encode::probe_hardware()?;
let encodables = sky_partage::formats_encodables(&caps.codecs);
let formats = match format.as_str() {
    "auto" => encodables,
    texte => {
        let voulu = format_depuis_texte(texte)?;
        anyhow::ensure!(encodables.contains(&voulu), "{} : cette carte ne l'encode pas", voulu.libelle());
        vec![voulu]
    }
};
anyhow::ensure!(!formats.is_empty(), "aucun format transmissible sur cette carte");
```
`format_depuis_texte(&str) -> anyhow::Result<FormatVideo>` (dans `cmd_host.rs`, `pub(crate)`,
réutilisée par la tâche 7) : normalise comme `parse_codec` (minuscules, sans `- _ : .`), accepte
`hevc444`, `hevc420`, `h264`, et rejette le reste avec la liste attendue. Un test unitaire.
Supprimer le `bail!` de l.126-130 (devenu faux). `lignes_hote` affiche `Evenement::Format(f)` :
`format négocié : {f.libelle()}`.

- [ ] **Étape 6 : neutraliser, vérifier, commit**

Neutraliser : retirer la garde `p.formats.is_empty()` → le test d'hôte rougit ;
`formats_encodables` laissant passer `H264_444` → `h264_444_et_av1_...` rougit ; `partager` sans
la garde `formats.is_empty()` → `une_carte_sans_format_transmissible_...` rougit **pour la bonne
raison** (la garde de `heberger` ne doit pas répondre à sa place : vérifier le message reçu).

Run : `cargo test --workspace --no-fail-fast`, clippy.

```bash
git add spike/crates/sky-partage/src/formats.rs spike/crates/sky-partage/src/lib.rs spike/crates/sky-partage/src/hote.rs spike/crates/sky-partage/src/evenement.rs spike/crates/sky-app/src/materiel.rs spike/crates/sky-app/src/noyau.rs spike/crates/sky-app/src/partage.rs spike/crates/sky-app/src/lib.rs spike/crates/sky-probe/src/cmd_host.rs spike/crates/sky-probe/src/cmd_view.rs spike/crates/sky-probe/src/main.rs
git commit -m "feat(partage): l'hôte annonce ses formats encodables et encode le format négocié"
```

---

### Tâche 4 : `sky-decode` — un seul périphérique D3D11, et la sonde de décodage

**Files :**
- Create : `spike/crates/sky-decode/src/appareil.rs`, `src/sonde.rs`, `src/media_foundation.rs`
- Modify : `spike/crates/sky-decode/Cargo.toml`, `src/lib.rs`
- Modify : `spike/crates/sky-rendu/src/fenetre.rs` (l.663-749 : `adaptateur_nvidia`,
  `creer_appareil` retirés au profit de `sky_decode::creer_appareil_video` ; test l.1139 déplacé)

**Interfaces :**
- Produces :
  ```rust
  // sky_decode
  pub fn creer_appareil_video() -> anyhow::Result<(ID3D11Device, ID3D11DeviceContext)>;
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum CodecMf { Hevc, H264 }
  pub struct DecodeurMf { /* privé */ }
  impl DecodeurMf {
      pub fn nouveau(codec: CodecMf, appareil: &ID3D11Device, largeur_annoncee: u32, hauteur_annoncee: u32)
          -> Result<Self, ErreurDecodeur>;
  }
  pub fn mf_sait_decoder(appareil: &ID3D11Device, codec: CodecMf, largeur: u32, hauteur: u32) -> bool;
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub struct Decodables { pub hevc_444: bool, pub hevc_420: bool, pub h264: bool }
  pub fn sonder_decodage(largeur: u32, hauteur: u32) -> Decodables;
  // ErreurDecodeur gagne AucunDecodeur et MediaFoundation(String) (texte à la tâche 5, étape 1 —
  // les créer ICI, puisque `nouveau` les rend ; la tâche 5 ne fait que les brancher dans sky-app).
  ```

Cette tâche crée le décodeur Media Foundation **jusqu'à son ouverture** (`nouveau`), parce que la
sonde de décodage l'emploie : il n'existe qu'une ouverture du MFT, et la sonde n'en a pas de copie
simplifiée. La tâche 5 ajoute `decoder`, `ImageMf` et leurs tests.

- [ ] **Étape 1 : `Cargo.toml` de `sky-decode`**

Remplacer la dépendance `windows` normale (reliquat inutilisé) par :
```toml
windows = { version = "0.62", features = [
    "Win32_Foundation",
    "Win32_Graphics_Direct3D",
    "Win32_Graphics_Direct3D10",
    "Win32_Graphics_Direct3D11",
    "Win32_Graphics_Dxgi",
    "Win32_Graphics_Dxgi_Common",
    "Win32_Media_MediaFoundation",
    "Win32_System_Com",
] }
```
`Win32_Graphics_Direct3D10` sert à `ID3D10Multithread`. Si une interface manque à la compilation,
ajouter la feature que le compilateur nomme, et rien d'autre.

- [ ] **Étape 2 : `appareil.rs` — tests rouges puis implémentation**

Déplacer **tel quel** depuis `fenetre.rs` : `FABRICANT_NVIDIA`, `adaptateur_nvidia` (avec son
commentaire sur `DXGI_ERROR_NOT_FOUND`), et `creer_appareil` renommée `creer_appareil_video`,
publique. Trois changements, chacun commenté :

1. Drapeaux : `D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT` —
   `VIDEO_SUPPORT` est exigé par DXVA (sonde AMD, `spike/mesures/sonde-amd/source/src/dxva.rs`),
   `BGRA` par Direct2D.
2. Après création :
   ```rust
   // La protection multi-fil est exigée par le gestionnaire de périphérique de
   // Media Foundation (`IMFDXGIDeviceManager`) : le décodeur peut toucher au
   // contexte depuis ses propres fils. Le jalon 2 l'avait délibérément laissée
   // éteinte, faute de second utilisateur du périphérique ; ce second
   // utilisateur existe désormais. Le coût est une prise de verrou par appel de
   // contexte.
   let multi: ID3D10Multithread = appareil.cast().context("ID3D10Multithread")?;
   unsafe { multi.SetMultithreadProtected(true) };
   ```
3. Le commentaire d'en-tête : la règle d'adaptateur (NVIDIA d'abord) est gardée **et** sert
   désormais aussi au décodage Media Foundation ; la phrase « ce repli ne sert qu'à laisser la
   fenêtre s'ouvrir pour afficher un état, pas à décoder » est **fausse** désormais : la remplacer
   par « sur une machine sans NVIDIA, l'adaptateur par défaut est aussi celui qui décode (Media
   Foundation) ». Le long commentaire « `SetMultithreadProtected` : DÉLIBÉRÉMENT PAS ACTIVÉ » est
   **retiré** (il deviendrait faux), remplacé par le paragraphe du point 2.

Tests dans `appareil.rs` :
```rust
#[test]
fn l_appareil_est_protege_contre_les_acces_concurrents() {
    let (appareil, _) = creer_appareil_video().expect("ce test exige un périphérique Direct3D 11 matériel");
    let multi: ID3D10Multithread = appareil.cast().unwrap();
    assert!(unsafe { multi.GetMultithreadProtected() }.as_bool());
}

#[test]
fn l_appareil_offre_l_api_video() {
    let (appareil, _) = creer_appareil_video().expect("ce test exige un périphérique Direct3D 11 matériel");
    // Sans VIDEO_SUPPORT, cette interface est refusée.
    let _: ID3D11VideoDevice = appareil.cast().expect("ID3D11VideoDevice : VIDEO_SUPPORT manquant ?");
}
```
Déplacer aussi `l_appareil_vit_sur_la_carte_nvidia_quand_il_y_en_a_une` (fenetre.rs:1139) dans
`appareil.rs`. Il recoupe avec `cartes_cuda_disponibles()` de `sky-rendu` ; dans `sky-decode`,
utiliser `cudarc::driver::CudaContext::device_count().unwrap_or(0)` (même chose,
`sky-rendu/src/lib.rs:37`).

Dans `fenetre.rs`, `creer` appelle `sky_decode::creer_appareil_video()`. Retirer les imports
devenus inutiles.

- [ ] **Étape 3 : tests rouges de disponibilité** (dans `media_foundation.rs`)

```rust
#[test]
fn sur_cette_machine_media_foundation_decode_hevc_et_h264_en_materiel() {
    // Machine du propriétaire : RTX 4060, extension HEVC installée (sonde du
    // 02/10/2026, `spike/mesures/sonde-amd/rapport-test-machine-nvidia.txt`).
    let (appareil, _) = crate::creer_appareil_video().expect("périphérique Direct3D 11 matériel");
    assert!(mf_sait_decoder(&appareil, CodecMf::H264, 2560, 1440), "H.264 2560×1440");
    assert!(mf_sait_decoder(&appareil, CodecMf::Hevc, 2560, 1440), "HEVC 2560×1440");
}

#[test]
fn une_taille_demesuree_est_refusee_par_dxva() {
    let (appareil, _) = crate::creer_appareil_video().expect("périphérique Direct3D 11 matériel");
    // 16384×16384 dépasse toute limite DXVA connue (4096 sur le portable AMD, mesuré).
    assert!(!dxva_accepte(&appareil, CodecMf::H264, 16384, 16384));
    assert!(!mf_sait_decoder(&appareil, CodecMf::H264, 16384, 16384));
}
```

- [ ] **Étape 4 : implémenter `media_foundation.rs` jusqu'à l'ouverture**

Recette : `spike/mesures/sonde-amd/source/src/mf.rs` (l.86-111 énumération, l.187-275 échantillons
et sorties, l.380-493 ouverture) et `src/dxva.rs` (l.150-196 question DXVA). **Lire ces passages
avant d'écrire.** En-tête et briques :

```rust
//! Décodage Media Foundation : HEVC Main 4:2:0 et H.264, en matériel.
//!
//! Recette mesurée par la sonde AMD du 02/10/2026
//! (`spike/mesures/sonde-amd/source/src/mf.rs`, rapports à côté). Trois faits
//! en fondent la forme :
//! - aucun décodeur n'est énuméré avec `MFT_ENUM_FLAG_HARDWARE`, ni sur AMD ni
//!   sur NVIDIA : le « matériel » est un MFT SYNCHRONE de Microsoft qui fait son
//!   DXVA en interne quand on lui confie un gestionnaire D3D11 ;
//! - avec ce gestionnaire et `MF_LOW_LATENCY = 1`, il rend 600 images sur 600 et
//!   en retient au plus une ; SANS gestionnaire (logiciel), le même réglage perd
//!   toutes les images après la 120e en HEVC. D'où : jamais de logiciel ;
//! - l'image sort dans une tranche d'un TABLEAU de textures NV12 lié au seul
//!   décodeur : non lisible par un nuanceur, elle se copie (`sky-rendu`).

use windows::core::{Interface, GUID};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11VideoDevice, D3D11_VIDEO_DECODER_DESC};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_NV12;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED};

use crate::ErreurDecodeur;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecMf {
    Hevc,
    H264,
}

impl CodecMf {
    fn sous_type(self) -> GUID {
        match self {
            CodecMf::Hevc => MFVideoFormat_HEVC,
            CodecMf::H264 => MFVideoFormat_H264,
        }
    }

    /// Profil DXVA, d'après `dxva.h` du Windows SDK 10.0.26100.0 (table de la
    /// sonde, `dxva.rs`, lignes « HEVC Main (8 bits 4:2:0) » et « H.264 VLD
    /// (sans FGT) »).
    fn profil_dxva(self) -> GUID {
        match self {
            // D3D11_DECODER_PROFILE_HEVC_VLD_MAIN
            CodecMf::Hevc => GUID::from_values(0x5b11d51b, 0x2f4c, 0x4452, [0xbc, 0xc3, 0x09, 0xf2, 0xa1, 0x16, 0x0c, 0xc0]),
            // D3D11_DECODER_PROFILE_H264_VLD_NOFGT
            CodecMf::H264 => GUID::from_values(0x1b81be68, 0xa0c7, 0x11d3, [0xb9, 0x84, 0x00, 0xc0, 0x4f, 0x2e, 0x73, 0xc5]),
        }
    }
}

/// COM et Media Foundation tenus pour la durée de vie d'un décodeur.
///
/// `CoInitializeEx` peut rendre `S_FALSE` (déjà initialisé sur ce fil) : un
/// succès qu'il faut équilibrer par `CoUninitialize` comme les autres. Un fil
/// déjà en mode STA (`RPC_E_CHANGED_MODE`) n'empêche pas Media Foundation : on
/// n'équilibre alors rien.
pub(crate) struct Plateforme {
    com_a_liberer: bool,
}

impl Plateforme {
    pub(crate) fn demarrer() -> Result<Self, ErreurDecodeur> {
        let com_a_liberer = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let plateforme = Self { com_a_liberer };
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.map_err(|e| mf("MFStartup", e))?;
        Ok(plateforme)
    }
}

impl Drop for Plateforme {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            if self.com_a_liberer {
                CoUninitialize();
            }
        }
    }
}

pub(crate) fn mf(etape: &str, e: windows::core::Error) -> ErreurDecodeur {
    ErreurDecodeur::MediaFoundation(format!("{etape} : {e}"))
}

/// DXVA sait-il décoder ce codec à cette taille, en NV12, sur cet adaptateur ?
/// La question est posée à la puce, pas au MFT : elle répond même si
/// l'extension HEVC manque.
pub(crate) fn dxva_accepte(appareil: &ID3D11Device, codec: CodecMf, largeur: u32, hauteur: u32) -> bool {
    let Ok(video) = appareil.cast::<ID3D11VideoDevice>() else { return false };
    let desc = D3D11_VIDEO_DECODER_DESC {
        Guid: codec.profil_dxva(),
        SampleWidth: largeur,
        SampleHeight: hauteur,
        OutputFormat: DXGI_FORMAT_NV12,
    };
    matches!(unsafe { video.GetVideoDecoderConfigCount(&desc) }, Ok(n) if n > 0)
}

/// Les décodeurs Media Foundation de ce codec, meilleur d'abord.
fn enumerer(codec: CodecMf) -> Result<Vec<IMFActivate>, ErreurDecodeur> {
    let info = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: codec.sous_type() };
    let drapeaux = MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_LOCALMFT | MFT_ENUM_FLAG_SORTANDFILTER;
    let mut tableau: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut n = 0u32;
    unsafe { MFTEnumEx(MFT_CATEGORY_VIDEO_DECODER, drapeaux, Some(&info), None, &mut tableau, &mut n) }
        .map_err(|e| mf("MFTEnumEx", e))?;
    let mut activations = Vec::new();
    if !tableau.is_null() {
        let elements = unsafe { std::slice::from_raw_parts_mut(tableau, n as usize) };
        activations.extend(elements.iter_mut().filter_map(Option::take));
        unsafe { CoTaskMemFree(Some(tableau as _)) };
    }
    Ok(activations)
}

/// Les trois preuves de la spec (§4) : la puce accepte la taille (DXVA), un
/// MFT existe pour le codec, et il accepte le périphérique. La création
/// complète du décodeur (`DecodeurMf::nouveau`) est la preuve la plus forte :
/// elle est faite et défaite ici.
pub fn mf_sait_decoder(appareil: &ID3D11Device, codec: CodecMf, largeur: u32, hauteur: u32) -> bool {
    DecodeurMf::nouveau(codec, appareil, largeur, hauteur).is_ok()
}
```

`DecodeurMf::nouveau` — ordre exact (sonde, `mf.rs:380-493`) ; chaque échec devient
`ErreurDecodeur::MediaFoundation(..)` par `mf(étape, e)` :

1. `Plateforme::demarrer()?`.
2. `dxva_accepte(appareil, codec, largeur_annoncee, hauteur_annoncee)` faux →
   `MediaFoundation(format!("la puce ne décode pas {codec:?} en {l}×{h}"))` (spec §4 : pas
   `ResolutionTropGrande`, faute de maximum).
3. `enumerer(codec)?` ; vide → `ErreurDecodeur::AucunDecodeur`. Prendre le **premier**
   (`SORTANDFILTER` trie).
4. `let transformation: IMFTransform = unsafe { activation.ActivateObject() }`.
5. Attributs : `transformation.GetAttributes()` ; si `GetUINT32(&MF_TRANSFORM_ASYNC)` vaut 1 :
   **refuser** (`MediaFoundation("décodeur asynchrone : chemin non éprouvé")`) — le chemin
   asynchrone n'a jamais été exercé (sonde, `fabrication.md` §5). Puis
   `SetUINT32(&MF_LOW_LATENCY, 1)` — un échec est une erreur (D5 en dépend).
6. Gestionnaire : `MFCreateDXGIDeviceManager(&mut jeton, &mut gestionnaire)`,
   `gestionnaire.ResetDevice(appareil, jeton)`, puis
   `transformation.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, gestionnaire.as_raw() as usize)` ;
   un refus → `MediaFoundation("le décodeur refuse le périphérique Direct3D 11 : pas de décodage matériel")`.
7. Type d'entrée (`MFCreateMediaType`) : `MF_MT_MAJOR_TYPE = MFMediaType_Video`,
   `MF_MT_SUBTYPE = codec.sous_type()`, `MF_MT_FRAME_SIZE = ((l as u64) << 32) | h as u64` (taille
   ANNONCÉE), `MF_MT_FRAME_RATE = (60u64 << 32) | 1`,
   `MF_MT_INTERLACE_MODE = MFVideoInterlace_Progressive.0 as u32` ; `SetInputType(0, &type, 0)`.
8. `choisir_sortie()` (ci-dessous), puis `info = GetOutputStreamInfo(0)` : exiger
   `info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0` — un MFT qui attend qu'on lui
   fournisse les échantillons travaille en mémoire centrale (sonde : drapeaux 0x107 en matériel,
   0x7 en logiciel) → `MediaFoundation("le décodeur ne fournit pas ses images : pas de décodage matériel")`.
9. `ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)` puis `MFT_MESSAGE_NOTIFY_START_OF_STREAM`.

```rust
pub struct DecodeurMf {
    // L'ORDRE DES CHAMPS EST L'ORDRE DE DESTRUCTION, et il compte (sonde,
    // `mf.rs:514-517`) : le MFT d'abord, puis son activation, le gestionnaire,
    // et Media Foundation en dernier. Le périphérique, lui, appartient à la
    // fenêtre et lui survit (`Visionnage` déclare le décodeur avant l'afficheur).
    transformation: IMFTransform,
    activation: IMFActivate,
    _gestionnaire: IMFDXGIDeviceManager,
    info: MFT_OUTPUT_STREAM_INFO,
    largeur: u32,
    hauteur: u32,
    _plateforme: Plateforme,
}

impl Drop for DecodeurMf {
    fn drop(&mut self) {
        unsafe {
            let _ = self.transformation.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            let _ = self.activation.ShutdownObject();
        }
    }
}
```

`choisir_sortie(&mut self) -> Result<(), ErreurDecodeur>` : parcourir
`GetOutputAvailableType(0, i)` jusqu'à l'échec, prendre le premier dont
`MF_MT_SUBTYPE == MFVideoFormat_NV12`, `SetOutputType(0, &type, 0)`, et
`(self.largeur, self.hauteur) = taille_d_affichage(&type)`. Aucun NV12 → `MediaFoundation` avec la
liste des sous-types vus. Pendant `nouveau`, `choisir_sortie` est appelée sur un `DecodeurMf` en
construction : l'écrire comme fonction libre `choisir_sortie(t: &IMFTransform) -> Result<(u32, u32), ErreurDecodeur>`
pour éviter l'objet à moitié construit.

```rust
/// La taille d'AFFICHAGE : l'ouverture minimale si le type la porte (un flux
/// 1080 lignes est codé sur 1088), sinon la taille de l'image.
fn taille_d_affichage(ty: &IMFMediaType) -> (u32, u32) {
    let mut zone = MFVideoArea::default();
    let octets = unsafe {
        std::slice::from_raw_parts_mut((&mut zone as *mut MFVideoArea).cast::<u8>(), std::mem::size_of::<MFVideoArea>())
    };
    let mut lus = 0u32;
    if unsafe { ty.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, octets, Some(&mut lus)) }.is_ok()
        && lus as usize == std::mem::size_of::<MFVideoArea>()
        && zone.Area.cx > 0
        && zone.Area.cy > 0
    {
        return (zone.Area.cx as u32, zone.Area.cy as u32);
    }
    let taille = unsafe { ty.GetUINT64(&MF_MT_FRAME_SIZE) }.unwrap_or(0);
    ((taille >> 32) as u32, taille as u32)
}
```
(Vérifier la signature réelle de `IMFAttributes::GetBlob` dans `windows` 0.62 et l'adapter sans
changer le sens.)

`ErreurDecodeur` (capacites.rs l.17-60) gagne :
```rust
/// Aucun moteur ne sait décoder un format que SkyShare transporte : ni NVDEC
/// en HEVC 4:4:4, ni Media Foundation en matériel (HEVC 4:2:0 ou H.264).
#[error(
    "cette machine ne sait décoder en matériel aucun des formats vidéo de SkyShare \
     (HEVC ou H.264) : elle ne peut pas recevoir d'écran."
)]
AucunDecodeur,
/// Un appel Media Foundation a échoué, ou une image est sortie en mémoire
/// centrale (repli logiciel, refusé : spec D5).
#[error("le décodeur Media Foundation a échoué : {0}")]
MediaFoundation(String),
```
Les `match` exhaustifs hors de `sky-decode` qui cassent (`sky-app/src/partage.rs`,
`fin_de_visionnage`) reçoivent provisoirement `Ouverture(AucunDecodeur | MediaFoundation(_)) =>
FinVue::DecodeurRefuse` ; la tâche 5 y met le vrai message.

- [ ] **Étape 5 : `sonde.rs`**

```rust
//! Ce que cette machine sait décoder, établi AVANT toute négociation.

use crate::media_foundation::{mf_sait_decoder, CodecMf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decodables {
    /// NVDEC, HEVC 4:4:4, à la taille demandée.
    pub hevc_444: bool,
    /// Media Foundation matériel, HEVC Main 4:2:0.
    pub hevc_420: bool,
    /// Media Foundation matériel, H.264.
    pub h264: bool,
}

/// Interroge les deux moteurs. Ne panique pas sans NVIDIA (`sonder_materiel`
/// vérifie `nvcuda.dll` avant `cuInit`) ni sans Media Foundation (tout échec
/// rend `false`).
pub fn sonder_decodage(largeur: u32, hauteur: u32) -> Decodables {
    let hevc_444 = matches!(
        crate::sonder_materiel(),
        Ok(c) if c.hevc_444 && c.largeur_max >= largeur && c.hauteur_max >= hauteur
    );
    let (hevc_420, h264) = match crate::creer_appareil_video() {
        Ok((appareil, _)) => (
            mf_sait_decoder(&appareil, CodecMf::Hevc, largeur, hauteur),
            mf_sait_decoder(&appareil, CodecMf::H264, largeur, hauteur),
        ),
        Err(_) => (false, false),
    };
    Decodables { hevc_444, hevc_420, h264 }
}
```

Test (machine du propriétaire) : `sonder_decodage(2560, 1440)` rend les trois à `true`, et
`sonder_decodage(16384, 16384)` rend `hevc_420 == false && h264 == false`. Vérifier que les champs
de `Capacites` s'appellent bien `hevc_444`, `largeur_max`, `hauteur_max` (`capacites.rs:10`).

- [ ] **Étape 6 : `lib.rs`, neutralisation, vérifications, commit**

`lib.rs` : `mod appareil; mod media_foundation; mod sonde;` et
`pub use appareil::creer_appareil_video; pub use media_foundation::{mf_sait_decoder, CodecMf, DecodeurMf}; pub use sonde::{sonder_decodage, Decodables};`

Neutraliser, une à la fois :
- retirer `VIDEO_SUPPORT` → `l_appareil_offre_l_api_video` **et**
  `sur_cette_machine_media_foundation_...` rougissent ;
- retirer `SetMultithreadProtected` → `l_appareil_est_protege_...` rougit ;
- retirer la question DXVA de `nouveau` (étape 2) → `une_taille_demesuree_...` rougit-il sur sa
  seconde assertion ? S'il reste vert (le MFT refuse aussi la taille), le noter dans le rapport :
  deux gardes répondent l'une pour l'autre ; garder la question DXVA, qui est celle posée à la
  puce et la seule qui nomme la cause ;
- `SetUINT32(MF_LOW_LATENCY, 0)` : aucun test de cette tâche ne le voit ; la tâche 5 y revient.

Run : `cargo test --workspace --no-fail-fast` (les tests de fenêtre de `sky-rendu` doivent rester
verts sur le nouveau périphérique), clippy.

```bash
git add spike/crates/sky-decode/Cargo.toml spike/crates/sky-decode/src/lib.rs spike/crates/sky-decode/src/appareil.rs spike/crates/sky-decode/src/sonde.rs spike/crates/sky-decode/src/media_foundation.rs spike/crates/sky-decode/src/capacites.rs spike/crates/sky-rendu/src/fenetre.rs spike/crates/sky-app/src/partage.rs
git commit -m "feat(sky-decode): périphérique D3D11 unique, ouverture Media Foundation et sonde de décodage"
```

---

### Tâche 5 : `sky-decode` — décoder par Media Foundation

**Files :**
- Modify : `spike/crates/sky-decode/src/media_foundation.rs` (`decoder`, `tirer`, `ImageMf`)
- Create : `spike/crates/sky-decode/src/source.rs`
- Modify : `spike/crates/sky-decode/src/lib.rs`
- Modify : `spike/crates/sky-app/src/partage.rs` (`fin_de_visionnage` l.180-205 et ses tests),
  `noyau.rs` (message)
- Create : `spike/crates/sky-decode/tests/media_foundation.rs`

**Interfaces :**
- Consumes : `creer_appareil_video`, `CodecMf`, `DecodeurMf::nouveau`, `mf`, `ErreurDecodeur::{AucunDecodeur, MediaFoundation}` (tâche 4).
- Produces :
  ```rust
  impl DecodeurMf {
      pub fn decoder(&mut self, unite: &[u8], horodatage_ms: u64) -> Result<Option<ImageMf>, ErreurDecodeur>;
  }
  pub struct ImageMf { pub largeur: u32, pub hauteur: u32, pub horodatage_ms: u64, /* privé */ }
  impl ImageMf {
      pub fn texture(&self) -> &ID3D11Texture2D;
      pub fn tranche(&self) -> u32;
      pub fn copier_luminance(&self) -> anyhow::Result<Vec<u8>>;   // tests et mesures
  }
  // source.rs
  pub enum SourceImage<'a> {
      Cuda444(SurfaceCuda),
      Nv12 { texture: &'a ID3D11Texture2D, tranche: u32 },
  }
  // sky_app::noyau
  pub const MESSAGE_AUCUN_DECODEUR: &str;
  ```

- [ ] **Étape 1 : tests rouges `tests/media_foundation.rs`**

Le flux vient de NVENC, comme `sky-decode/tests/aller_retour.rs`. **Lire ce fichier d'abord** :
ses aides `texture_de_bandes`, `compter_mal_classes`, et le test
`chaque_unite_poussee_rend_sa_propre_image_sans_retard` (l.163-225) sont le modèle à reproduire.
En tête : `use sky_encode as _;` si l'éditeur de liens réclame `NvEncodeAPI*` (LNK2019, cf.
`sky-rendu/src/lib.rs:16-17`). Les encodages se font **sur le périphérique de
`creer_appareil_video()`** — celui que la fenêtre utilisera.

```rust
//! Décodage Media Foundation de flux NVENC réels, HEVC 4:2:0 et H.264.
//! Exige une carte NVIDIA (pour encoder) : panique sans elle, comme
//! `aller_retour.rs`.

const LARGEUR: u32 = 1920;
/// 1080 et non 1088 : la hauteur codée est arrondie au bloc, et le jalon 2 a
/// payé cette différence (`SurfaceCuda`). L'image doit sortir à sa hauteur
/// d'AFFICHAGE.
const HAUTEUR: u32 = 1080;
/// Taille annoncée au décodeur, plus grande que le flux : le décodeur doit
/// suivre la vraie taille (`MF_E_TRANSFORM_STREAM_CHANGE`), spec §2 inconnue 3.
const LARGEUR_ANNONCEE: u32 = 2560;
const HAUTEUR_ANNONCEE: u32 = 1440;
const IMAGES: usize = 8;

fn chaque_unite_rend_sa_propre_image(codec_nvenc: Codec, codec_mf: CodecMf) {
    let (appareil, _) = creer_appareil_video().expect("périphérique Direct3D 11 matériel");
    let mut encodeur = NvencEncoder::new(&appareil, codec_nvenc, LARGEUR, HAUTEUR, 60, 80_000_000)
        .unwrap_or_else(|e| panic!("ce test exige NVENC ({codec_nvenc:?}) : {e:#}"));
    // Deux textures de bandes (rouge en tête / bleu en tête), alternées,
    // encodées en `IMAGES` paquets exactement comme `aller_retour.rs` l.175-196.
    let paquets: Vec<(bool, Vec<u8>)> = /* … */;
    let mut decodeur = DecodeurMf::nouveau(codec_mf, &appareil, LARGEUR_ANNONCEE, HAUTEUR_ANNONCEE)
        .unwrap_or_else(|e| panic!("ce test exige Media Foundation matériel ({codec_mf:?}) : {e}"));
    for (rang, (rouge, paquet)) in paquets.iter().enumerate() {
        let image = decodeur
            .decoder(paquet, rang as u64)
            .expect("décodage")
            .unwrap_or_else(|| panic!("l'unité {rang} n'a rendu aucune image au même appel"));
        assert_eq!(image.horodatage_ms, rang as u64, "l'unité {rang} a rendu l'image d'une autre");
        assert_eq!((image.largeur, image.hauteur), (LARGEUR, HAUTEUR), "taille d'affichage");
        let luminance = image.copier_luminance().expect("copie de test");
        assert_eq!(luminance.len(), (LARGEUR * HAUTEUR) as usize);
        let part = part_mal_classee_luminance(&luminance, *rouge);
        assert!(part <= 0.02, "l'unité {rang} a le contenu de l'autre motif ({:.1} %)", part * 100.0);
    }
}

#[test]
fn hevc_420_chaque_unite_rend_sa_propre_image_a_sa_taille() {
    chaque_unite_rend_sa_propre_image(Codec::Hevc420, CodecMf::Hevc);
}

#[test]
fn h264_chaque_unite_rend_sa_propre_image_a_sa_taille() {
    chaque_unite_rend_sa_propre_image(Codec::H264_420, CodecMf::H264);
}
```

Le `/* … */` ci-dessus est la recopie des lignes 175-196 d'`aller_retour.rs` (fabrication des
deux textures et boucle d'encodage), à reprendre telles quelles avec `IMAGES` au lieu de
`IMAGES_ALTERNEES`. `part_mal_classee_luminance(luminance, rouge_en_tete)` suit
`compter_mal_classes` (aller_retour.rs:385) en lisant la luminance : le rouge pleine plage BT.601
a Y = 76, le bleu Y = 29 ; seuil à mi-chemin (52) ; la bande d'une ligne se calcule comme dans
`ligne_rouge` (aller_retour.rs:370). Rend la part des pixels dont la classe (clair/sombre) contredit
le motif attendu.

- [ ] **Étape 2 : lancer — échec de compilation (`decoder`, `ImageMf` inconnus).**

Run : `cargo test -p sky-decode --test media_foundation`

- [ ] **Étape 3 : `decoder`, `tirer`, `ImageMf`**

```rust
/// Pousse UNE unité d'accès entière et rend l'image de CETTE unité, au même
/// appel — le contrat de `sky-partage/src/doublure.rs::DecodeurFactice`, dont
/// dépend la garde d'affichage. Mesuré sur le chemin matériel : au plus une
/// image retenue (sonde) ; prouvé ici par
/// `tests/media_foundation.rs::*_chaque_unite_rend_sa_propre_image_a_sa_taille`.
/// `Ok(None)` : l'unité n'a rendu aucune image (en-têtes seuls).
pub fn decoder(&mut self, unite: &[u8], horodatage_ms: u64) -> Result<Option<ImageMf>, ErreurDecodeur> {
    let echantillon = echantillon_de(unite, horodatage_ms)?;
    let mut refus = 0;
    loop {
        match unsafe { self.transformation.ProcessInput(0, &echantillon, 0) } {
            Ok(()) => break,
            Err(e) if e.code() == MF_E_NOTACCEPTING => {
                // Une sortie attend : la vider (elle appartient à une unité
                // précédente, on ne la rend pas) puis réessayer.
                refus += 1;
                if refus > 16 {
                    return Err(mf("ProcessInput refuse toujours l'entrée", e));
                }
                while self.tirer()?.is_some() {}
            }
            Err(e) => return Err(mf("ProcessInput", e)),
        }
    }
    let mut derniere = None;
    while let Some(image) = self.tirer()? {
        derniere = Some(image);
    }
    Ok(derniere)
}
```

`echantillon_de(unite, horodatage_ms)` : comme `echantillon` de la sonde (`mf.rs:187-201`) avec
`SetSampleTime(horodatage_ms as i64 * 10_000)` (unités de 100 ns) et `SetSampleDuration(166_667)`.

`tirer(&mut self) -> Result<Option<ImageMf>, ErreurDecodeur>` : comme `Contexte::tirer` de la
sonde (`mf.rs:233-275`), avec `pSample: ManuallyDrop::new(None)` (le décodeur fournit ses
échantillons, exigé par `nouveau`), `ManuallyDrop::take` sur `pSample` ET `pEvents` après l'appel
(sinon fuite COM), `MF_E_TRANSFORM_NEED_MORE_INPUT` → `Ok(None)`, `MF_E_TRANSFORM_STREAM_CHANGE`
→ `(self.largeur, self.hauteur) = choisir_sortie(&self.transformation)?`, relire
`GetOutputStreamInfo(0)`, reboucler — **au plus 4 fois de suite**, sinon erreur. Un succès passe
par :

```rust
fn image_de(&self, echantillon: IMFSample) -> Result<ImageMf, ErreurDecodeur> {
    let tampon = unsafe { echantillon.GetBufferByIndex(0) }.map_err(|e| mf("GetBufferByIndex", e))?;
    // D5 : une image hors GPU signe un repli logiciel, jamais affiché.
    let dxgi: IMFDXGIBuffer = tampon.cast().map_err(|_| {
        ErreurDecodeur::MediaFoundation("image rendue en mémoire centrale : décodage logiciel refusé".into())
    })?;
    let mut brut: *mut core::ffi::c_void = std::ptr::null_mut();
    unsafe { dxgi.GetResource(&ID3D11Texture2D::IID, &mut brut) }.map_err(|e| mf("IMFDXGIBuffer::GetResource", e))?;
    let texture = unsafe { ID3D11Texture2D::from_raw(brut) };
    let tranche = unsafe { dxgi.GetSubresourceIndex() }.map_err(|e| mf("GetSubresourceIndex", e))?;
    let temps = unsafe { echantillon.GetSampleTime() }.map_err(|e| mf("GetSampleTime", e))?;
    Ok(ImageMf {
        largeur: self.largeur,
        hauteur: self.hauteur,
        horodatage_ms: (temps.max(0) as u64 + 5_000) / 10_000,
        texture,
        tranche,
        _echantillon: echantillon,
    })
}
```

`ImageMf` garde l'échantillon (`_echantillon: IMFSample`) : tant qu'il vit, la tranche du tableau
n'est pas réattribuée. `copier_luminance` : périphérique par `texture.GetDevice()`, contexte
immédiat, texture STAGING NV12 `largeur × hauteur` (ArraySize 1, `D3D11_USAGE_STAGING`,
`D3D11_CPU_ACCESS_READ`, BindFlags 0),
`CopySubresourceRegion(&staging, 0, 0, 0, 0, &texture, tranche, Some(&D3D11_BOX { left: 0, top: 0, front: 0, right: largeur, bottom: hauteur, back: 1 }))`,
`Map`, recopier `hauteur` lignes de `largeur` octets avec `RowPitch` (plan Y seul), `Unmap`
(sonde, `mf.rs:312-343`). Doc : « RÉSERVÉ AUX TESTS ET AUX MESURES ».

`source.rs` :
```rust
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;

use crate::SurfaceCuda;

/// D'où l'affichage tire une image décodée.
pub enum SourceImage<'a> {
    /// Trois plans 4:4:4 en mémoire CUDA (NVDEC).
    Cuda444(SurfaceCuda),
    /// Une tranche d'un tableau de textures NV12 (Media Foundation), liée au
    /// décodeur seul : à copier avant d'être lue par un nuanceur.
    Nv12 { texture: &'a ID3D11Texture2D, tranche: u32 },
}
```
Exporter `ImageMf`, `SourceImage` depuis `lib.rs`.

- [ ] **Étape 4 : le message du spectateur**

`noyau.rs` :
```rust
/// TEXTE NOUVEAU, à valider par le propriétaire (spec §6).
pub const MESSAGE_AUCUN_DECODEUR: &str = "Cette machine ne sait décoder en matériel aucun des \
    formats vidéo de SkyShare (HEVC ou H.264) : elle ne peut pas recevoir d'écran. Le pilote \
    de la carte graphique est-il à jour ?";
```
`partage.rs::fin_de_visionnage` : `Ouverture(AucunDecodeur) => FinVue::Autre { message: MESSAGE_AUCUN_DECODEUR.into() }`,
`Ouverture(MediaFoundation(_)) => FinVue::DecodeurRefuse` (remplace la ligne provisoire de la
tâche 4) ; étendre les tests de mapping (l.481-580) aux deux variantes.

- [ ] **Étape 5 : vert, neutralisations, commit**

Run : `cargo test -p sky-decode --test media_foundation -- --nocapture`.

Neutraliser, une à la fois, et consigner chaque issue dans le rapport :
- `MF_LOW_LATENCY` à 0 : si un test rougit, le réglage est porteur ; s'il reste vert, l'écrire
  (la sonde n'a pas mesuré le matériel sans ce réglage).
- `taille_d_affichage` rendant `MF_MT_FRAME_SIZE` seul : si les tests rougissent (hauteur 1088),
  la garde est prouvée ; sinon le MFT ne rembourre pas le type, l'écrire.
- retirer la gestion de `STREAM_CHANGE` (rendre une erreur) : les deux tests rougissent si le MFT
  émet bien `STREAM_CHANGE` (annonce 2560×1440, flux 1920×1080) ; s'ils restent verts, l'inconnue
  3 du §2 se lève dans l'autre sens : l'écrire.
- `decoder` rendant `Ok(None)` sur la première unité : les deux tests rougissent.
- `image_de` acceptant une sortie sans `IMFDXGIBuffer` : aucun test ne peut la produire ; l'écrire
  (D5 repose sur la revue de cette ligne et sur l'exigence `PROVIDES_SAMPLES` de `nouveau`).

`cargo test --workspace --no-fail-fast`, clippy.

```bash
git add spike/crates/sky-decode/src/media_foundation.rs spike/crates/sky-decode/src/source.rs spike/crates/sky-decode/src/lib.rs spike/crates/sky-decode/tests/media_foundation.rs spike/crates/sky-app/src/partage.rs spike/crates/sky-app/src/noyau.rs
git commit -m "feat(sky-decode): décodage Media Foundation matériel, une image par unité, à sa taille"
```

---

### Tâche 6 : `sky-rendu` — afficher une image NV12

**Files :**
- Modify : `spike/crates/sky-rendu/src/interop.rs` (trait `ImageAAfficher` l.48-67)
- Create : `spike/crates/sky-rendu/src/nv12.rs` (`PontNv12`)
- Modify : `spike/crates/sky-rendu/src/nuanceur.rs` (sources, `Programme`)
- Modify : `spike/crates/sky-rendu/src/fenetre.rs` (`afficher` l.179-235, champ `pont_nv12`)
- Modify : `spike/crates/sky-rendu/src/image_de_test.rs`, `src/lib.rs`
- Modify : `spike/crates/sky-partage/src/doublure.rs` (`ImageFactice`)
- Modify : `spike/crates/sky-rendu/tests/couleur.rs` ; Create : `spike/crates/sky-rendu/tests/image_nv12.rs`

**Interfaces :**
- Consumes : `sky_decode::{SourceImage, ImageMf, DecodeurMf, CodecMf}`.
- Produces :
  ```rust
  pub trait ImageAAfficher {
      fn largeur(&self) -> u32;
      fn hauteur(&self) -> u32;
      fn source(&self) -> sky_decode::SourceImage<'_>;   // remplace `surface()`
  }
  impl ImageAAfficher for sky_decode::ImageMf { .. }   // Nv12
  pub fn image_de_test_nv12(appareil: &ID3D11Device, largeur: u32, hauteur: u32, yuv: [u8; 3])
      -> anyhow::Result<ImageNv12DeTest>;
  ```

- [ ] **Étape 1 : test de couleur NV12 (rouge)** dans `tests/couleur.rs`

```rust
#[test]
fn la_conversion_bt601_nv12_rend_les_couleurs_attendues() {
    let mut fenetre = Fenetre::ouvrir_masquee("couleur nv12", 64, 64).expect("fenêtre");
    for (nom, yuv, attendu) in CAS {
        let image = image_de_test_nv12(fenetre.appareil(), 64, 64, yuv).expect("image NV12");
        fenetre.afficher(&image).expect("affichage");
        let rgb = fenetre.pixel_central().expect("pixel");
        for canal in 0..3 {
            let ecart = (rgb[canal] as i32 - attendu[canal] as i32).abs();
            assert!(ecart <= 4, "{nom} : obtenu {rgb:?}, attendu {attendu:?}");
        }
    }
}
```
`CAS` est la table existante du test 4:4:4 (mêmes valeurs YUV BT.601 pleine plage). Ce test
**n'a pas** la sortie silencieuse « aucune carte CUDA » du test 4:4:4 : le chemin NV12 ne dépend
pas de CUDA.

- [ ] **Étape 2 : lancer — échec de compilation.** Run : `cargo test -p sky-rendu --test couleur`

- [ ] **Étape 3 : implémenter**

1. **Trait** : `surface()` → `source()`. `impl ImageAAfficher for ImageDecodee` rend
   `SourceImage::Cuda444(self.surface())` ; `ImageUnie` aussi ; `ImageFactice` (doublure de
   `sky-partage`) rend `SourceImage::Cuda444(SurfaceCuda { pointeur: 0, pas: 0, hauteur_surface: 0 })` ;
   nouvel `impl ImageAAfficher for ImageMf` rend `SourceImage::Nv12 { texture: self.texture(), tranche: self.tranche() }`.
   Réécrire la doc du trait (« une image YUV 4:4:4 encore sur le GPU » → « une image décodée,
   4:4:4 en mémoire CUDA ou NV12 en texture Direct3D 11 »).
2. **`nv12.rs`** :
   ```rust
   /// La copie de l'image Media Foundation dans une texture que le nuanceur
   /// peut lire. La texture du décodeur est une tranche de tableau liée
   /// `D3D11_BIND_DECODER` seulement (mesuré, spec §2) : on ne peut pas
   /// l'échantillonner. La copie se fait à la taille d'AFFICHAGE, ce qui écarte
   /// les lignes de remplissage (1080 contre 1088).
   pub(crate) struct PontNv12 {
       texture: ID3D11Texture2D,
       vue_y: ID3D11ShaderResourceView,
       vue_uv: ID3D11ShaderResourceView,
       largeur: u32,
       hauteur: u32,
   }
   ```
   `nouveau(appareil, largeur, hauteur)` : refuser une taille impaire (NV12) ; texture
   `DXGI_FORMAT_NV12`, `largeur × hauteur`, MipLevels 1, ArraySize 1, `D3D11_USAGE_DEFAULT`,
   `D3D11_BIND_SHADER_RESOURCE` ; deux vues `D3D11_SRV_DIMENSION_TEXTURE2D`, `MipLevels: 1` :
   `DXGI_FORMAT_R8_UNORM` (plan Y) et `DXGI_FORMAT_R8G8_UNORM` (plan UV, demi-résolution).
   `televerser(&self, contexte, texture, tranche)` :
   `CopySubresourceRegion(&self.texture, 0, 0, 0, 0, texture, tranche, Some(&D3D11_BOX { left: 0, top: 0, front: 0, right: largeur, bottom: hauteur, back: 1 }))`.
   `vues(&self) -> [Option<ID3D11ShaderResourceView>; 2]`.
3. **`nuanceur.rs`** : scinder `SOURCE` en `COMMUN` (struct `Sortie`, `sommet`, `yuv_vers_rgb`,
   `SamplerState echantillonneur : register(s0)`) + `PIXEL_444` (les trois `Texture2D<float>` et
   `pixel`) + `PIXEL_NV12` :
   ```hlsl
   Texture2D<float> nv12_y : register(t0);
   Texture2D<float2> nv12_uv : register(t1);

   float4 pixel_nv12(Sortie entree) : SV_Target {
       float y = nv12_y.Sample(echantillonneur, entree.coordonnees);
       float2 uv = nv12_uv.Sample(echantillonneur, entree.coordonnees);
       return float4(saturate(yuv_vers_rgb(float3(y, uv.x, uv.y))), 1.0);
   }
   ```
   Compiler `sommet` et `pixel` depuis `COMMUN` + `PIXEL_444`, `pixel_nv12` depuis `COMMUN` +
   `PIXEL_NV12` (deux sources : deux textures liées au même registre dans une même source sont
   refusées par le compilateur). `compiler_etage` prend la source en paramètre. `Programme` gagne
   `pixels_nv12: ID3D11PixelShader`. **`yuv_vers_rgb` n'existe qu'une fois** (D6) ; le
   commentaire « 85,50 dB contre 36,13 dB en BT.709 » reste au-dessus d'elle.
4. **`fenetre.rs`, `afficher`** : `match image.source()` ; `Cuda444(surface)` → chemin actuel
   (`Pont`) inchangé ; `Nv12 { texture, tranche }` → `PontNv12` recréé si la taille change (comme
   `Pont`), `televerser`, puis la même séquence de dessin avec `pixels_nv12` et **deux** vues.
   Factoriser la séquence de dessin (`OMSetRenderTargets` … `Draw(4, 0)` … détacher les vues) en
   une fonction qui reçoit le nuanceur de pixels et la tranche de vues, pour qu'il n'en existe
   qu'une. Le détachement final passe autant de `None` que de vues liées.
5. **`image_de_test.rs`** : `ImageNv12DeTest { texture: ID3D11Texture2D, largeur: u32, hauteur: u32 }`
   et `image_de_test_nv12` : tampon de `largeur * hauteur * 3 / 2` octets — plan Y rempli de
   `yuv[0]`, puis plan UV entrelacé `yuv[1], yuv[2]` — passé en
   `D3D11_SUBRESOURCE_DATA { pSysMem, SysMemPitch: largeur, .. }` à `CreateTexture2D` (NV12,
   `D3D11_USAGE_DEFAULT`, BindFlags `SHADER_RESOURCE`). Si le pilote refuse les données initiales
   pour NV12, créer la texture vide puis `UpdateSubresource`, et le noter dans le rapport.
   Implémente `ImageAAfficher` en `SourceImage::Nv12 { texture, tranche: 0 }`. Exporter depuis
   `lib.rs`, sous le commentaire « RÉSERVÉ AUX TESTS ET AUX MESURES » du module.

- [ ] **Étape 4 : image réelle NV12** — `tests/image_nv12.rs`

Une image en quatre quadrants unis (rouge, vert, bleu, blanc) 1280×720, encodée par NVENC en
**H.264** puis en **HEVC 4:2:0** (même fonction de test, deux `#[test]`) sur `fenetre.appareil()`
d'une fenêtre masquée de la taille de l'image (vérifier que Windows l'a accordée, comme
`image_reelle.rs`), décodée par `DecodeurMf`, affichée ; relire `pixels_de_la_cible()` au centre de
chaque quadrant ; tolérance **8** par canal, en constante commentée (perte de compression : on
compare à la couleur source, pas à un décodeur de référence comme le test 4:4:4 et ses 2).

- [ ] **Étape 5 : neutraliser, vérifier, commit**

Neutraliser : échanger `uv.x` et `uv.y` dans `pixel_nv12` → le test de couleur NV12 rougit (rouge
et bleu inversés) ; `PontNv12` jamais recréé quand la taille change → écrire un test qui affiche
une 64×64 puis une 128×64 (`image_de_test_nv12`) et vérifie le pixel central de la seconde, et le
faire rougir ; copier la tranche 0 au lieu de `tranche` → le test d'image réelle doit rougir si le
décodeur emploie plusieurs tranches (8 en H.264 sur NVIDIA, mesuré) ; s'il reste vert, l'écrire.

`cargo test --workspace --no-fail-fast`, clippy.

```bash
git add spike/crates/sky-rendu/src/interop.rs spike/crates/sky-rendu/src/nv12.rs spike/crates/sky-rendu/src/nuanceur.rs spike/crates/sky-rendu/src/fenetre.rs spike/crates/sky-rendu/src/image_de_test.rs spike/crates/sky-rendu/src/lib.rs spike/crates/sky-rendu/tests/couleur.rs spike/crates/sky-rendu/tests/image_nv12.rs spike/crates/sky-partage/src/doublure.rs
git commit -m "feat(sky-rendu): affichage NV12 par un second nuanceur, même matrice BT.601"
```

---

### Tâche 7 : le spectateur sonde, offre et choisit son moteur

**Files :**
- Modify : `spike/crates/sky-partage/src/formats.rs` (`formats_decodables`, `formats_a_offrir`)
- Modify : `spike/crates/sky-partage/src/spectateur.rs` (`Decodage` l.160-179, `Visionnage`
  l.224, `ParametresSpectateur` l.590-598, `regarder` l.600-687, `recevoir` l.689-712)
- Modify : `spike/crates/sky-app/src/partage.rs` (construction de `ParametresSpectateur`, l.95)
- Modify : `spike/crates/sky-probe/src/cmd_view.rs`, `main.rs` (`Cmd::View`, option `--format`)

**Interfaces :**
- Consumes : `sonder_decodage`, `Decodables`, `DecodeurMf`, `CodecMf`, `ImageMf` (tâches 4-5) ;
  `PeerLink::offrant(.., formats)`, `format_negocie` (tâche 1) ; `Evenement::Format`,
  `Fin::AucunFormatCommun` (tâche 3) ; `format_depuis_texte` (`sky-probe`, tâche 3).
- Produces :
  ```rust
  pub fn formats_decodables(d: &Decodables) -> Vec<FormatVideo>;   // ordre PREFERENCE
  pub fn formats_a_offrir(decodables: Vec<FormatVideo>, imposes: Option<&[FormatVideo]>) -> Vec<FormatVideo>;
  pub struct ParametresSpectateur<'a> { .., pub formats_imposes: Option<Vec<FormatVideo>> }
  impl Decodage for DecodeurMf { type Image = ImageMf; .. }
  ```

- [ ] **Étape 1 : `formats.rs` — tests rouges puis implémentation**

```rust
/// Les formats que ce spectateur sait décoder, dans l'ordre de préférence.
pub fn formats_decodables(d: &Decodables) -> Vec<FormatVideo> {
    FormatVideo::PREFERENCE
        .into_iter()
        .filter(|f| match f {
            FormatVideo::Hevc444 => d.hevc_444,
            FormatVideo::Hevc420 => d.hevc_420,
            FormatVideo::H264 => d.h264,
        })
        .collect()
}

/// Ce que le spectateur offre : ce qu'il décode, restreint (jamais élargi) aux
/// formats imposés par `sky-probe view --format`.
pub fn formats_a_offrir(decodables: Vec<FormatVideo>, imposes: Option<&[FormatVideo]>) -> Vec<FormatVideo> {
    match imposes {
        Some(imposes) => decodables.into_iter().filter(|f| imposes.contains(f)).collect(),
        None => decodables,
    }
}
```
Tests : le portable AMD (`hevc_444: false, hevc_420: true, h264: true`) → `[Hevc420, H264]` ; une
machine sans l'extension HEVC → `[H264]` ; rien → `[]` ; imposé `[H264]` sur `[Hevc420, H264]` →
`[H264]` ; imposé `[Hevc444]` sur `[Hevc420, H264]` → `[]`.

- [ ] **Étape 2 : `regarder`**

Remplacer le bloc `Decodeur::nouveau` (l.614-623, commentaire compris) par :
```rust
// Les capacités AVANT toute négociation, et c'est voulu (spec §5) : une
// machine qui ne décode rien l'apprend en une fraction de seconde, sans
// traverser la boîte aux lettres ni ICE, et l'hôte ne démarre rien pour elle.
// La sonde ne garde rien d'ouvert : le décodeur réel naît après la connexion,
// sur le périphérique de la fenêtre (Media Foundation l'exige).
let formats = formats_a_offrir(
    formats_decodables(&sonder_decodage(LARGEUR_ANNONCEE, HAUTEUR_ANNONCEE)),
    p.formats_imposes.as_deref(),
);
if formats.is_empty() {
    return Err(ErreurPartage::Visionnage(ErreurVisionnage::Ouverture(ErreurDecodeur::AucunDecodeur)));
}
```
Puis `PeerLink::offrant(Identity::generate(), &formats)?` (l.634). Après
`evenements(Evenement::Connecte { .. })` (l.681) :
```rust
let Some(format) = link.format_negocie() else {
    return Ok(Fin::AucunFormatCommun);
};
```
et `recevoir(&mut link, format, &ami.discord_name, puits, p.duree_max, arret, evenements)`.

- [ ] **Étape 3 : `recevoir` choisit le moteur**

```rust
fn recevoir(
    link: &mut PeerLink,
    format: FormatVideo,
    nom_de_l_hote: &str,
    puits: Option<Puits>,
    duree_max: Option<Duration>,
    arret: &Arret,
    evenements: &mut dyn FnMut(Evenement),
) -> Result<Fin, ErreurPartage> {
    let fenetre = Fenetre::ouvrir(&format!("SkyShare — écran de {nom_de_l_hote}"), LARGEUR_FENETRE, HAUTEUR_FENETRE)?;
    evenements(Evenement::Format(format));
    let ouverture = |e| ErreurPartage::Visionnage(ErreurVisionnage::Ouverture(e));
    match format {
        FormatVideo::Hevc444 => {
            let decodeur = Decodeur::nouveau(LARGEUR_ANNONCEE, HAUTEUR_ANNONCEE).map_err(ouverture)?;
            boucle(link, Visionnage::nouveau(decodeur, fenetre, Instant::now()), puits, duree_max, arret, evenements)
        }
        FormatVideo::Hevc420 | FormatVideo::H264 => {
            let codec = if format == FormatVideo::H264 { CodecMf::H264 } else { CodecMf::Hevc };
            let decodeur = DecodeurMf::nouveau(codec, fenetre.appareil(), LARGEUR_ANNONCEE, HAUTEUR_ANNONCEE)
                .map_err(ouverture)?;
            boucle(link, Visionnage::nouveau(decodeur, fenetre, Instant::now()), puits, duree_max, arret, evenements)
        }
    }
}
```
**Vérifier l'ordre des champs de `Visionnage`** (l.224) : `decodeur` doit être déclaré **avant**
`afficheur`, pour que le décodeur Media Foundation tombe avant la fenêtre qui porte son
périphérique. C'est le cas aujourd'hui ; ajouter sur le champ un commentaire qui le dit et
pourquoi. Mettre à jour la doc de `recevoir` (le décodeur n'est plus « créé en tête de
`regarder` »).

`impl Decodage for DecodeurMf` à côté de celui de `Decodeur` (l.169), même forme.

- [ ] **Étape 4 : appelants**

- `sky-app/src/partage.rs:95` : `formats_imposes: None`.
- `sky-probe view` : option `#[arg(long, default_value = "auto")] format: String` ; `auto` →
  `None`, sinon `Some(vec![format_depuis_texte(&format)?])`. Aide : « restreint ce que le
  spectateur offre ; `h264` ou `hevc420` éprouvent Media Foundation sur une machine NVIDIA ».
  Afficher `Evenement::Format(f)` : `format négocié : {f.libelle()}`.

- [ ] **Étape 5 : tests et neutralisation**

Les ~40 tests de `Visionnage` (spectateur.rs l.804+) ne changent pas : la garde est indépendante
du moteur. Neutraliser : `formats_a_offrir` qui élargit (rend `imposes` tel quel) → le test
« imposé `[Hevc444]` » rougit ; `formats_decodables` ignorant `hevc_444` → le test du portable
rougit. L'intégration complète (`regarder` jusqu'à l'image) n'a pas de test automatique, comme au
jalon 2 : elle relève de l'essai (tâche 9).

`cargo test --workspace --no-fail-fast`, clippy.

```bash
git add spike/crates/sky-partage/src/formats.rs spike/crates/sky-partage/src/spectateur.rs spike/crates/sky-app/src/partage.rs spike/crates/sky-probe/src/cmd_view.rs spike/crates/sky-probe/src/main.rs
git commit -m "feat(partage): le spectateur offre ce qu'il décode et ouvre le moteur négocié"
```

---

### Tâche 8 : l'interface — étiquette « Format » et textes devenus faux

**Files :**
- Modify : `spike/crates/sky-app/src/vue.rs` (`PartageVue::Diffuse` et `Regarde` l.110-146, tests
  l.188-200, l.307-360)
- Modify : `spike/crates/sky-app/src/partage.rs` (`appliquer` l.212-297 et ses tests l.459-468)
- Modify : `spike/crates/sky-encode/src/caps.rs` (message `Dll` l.71-77 et son test)
- Modify : `app/src/types.ts` (l.63-87), `app/src/messages.ts` (l.64-66),
  `app/src/messages.test.ts`, `app/src/composants/PanneauPartage.tsx` (cas `regarde` l.108-146 et
  `diffuse`), `app/src/composants/Partage.test.tsx` (fixtures l.184, 217, 243, 276, 315, 449, 493,
  512, 561)

**Interfaces :**
- Consumes : `Evenement::Format(FormatVideo)` (tâches 3, 7), `FormatVideo::libelle`.
- Produces : `PartageVue::Regarde { .., format: Option<String> }`, `PartageVue::Diffuse { .., format: Option<String> }` ;
  TS : `format: string | null`.

- [ ] **Étape 1 : tests rouges du cœur** (`partage.rs`, tests d'`appliquer`)

```rust
#[test]
fn le_format_negocie_s_inscrit_dans_la_vue_du_spectateur() {
    // Partir de la vue `Regarde` du test existant (l.459-468), `format: None`.
    let suivant = appliquer(&regarde, &Evenement::Format(FormatVideo::H264), 0, None).expect("vue");
    assert!(matches!(suivant, PartageVue::Regarde { format: Some(ref f), .. } if f == "H.264"));
}

#[test]
fn le_format_negocie_s_inscrit_dans_la_vue_de_l_hote() {
    // Même chose pour `PartageVue::Diffuse` et `FormatVideo::Hevc420` → "HEVC 4:2:0".
}

#[test]
fn les_mesures_suivantes_conservent_le_format() {
    // Regarde { format: Some("H.264") } puis Evenement::Mesures(Mesures::Reception(..))
    // → la vue rendue garde format == Some("H.264").
}
```
Écrire les corps complets des deux derniers sur le modèle du premier.

- [ ] **Étape 2 : implémenter le cœur**

`Regarde` et `Diffuse` gagnent `format: Option<String>` (`None` à leur création) ; `appliquer` :
`(Regarde{..}, Evenement::Format(f))` et `(Diffuse{..}, Evenement::Format(f))` → même vue avec
`format: Some(f.libelle().to_string())` ; les transitions existantes qui reconstruisent `Regarde` /
`Diffuse` (mesures) **recopient** `format`. **Vérifier l'ordre réel des événements** : côté
spectateur, `Connecte` (qui crée `Regarde`) précède `Format` (émis par `recevoir`) ; côté hôte,
lire `appliquer` pour savoir quel événement crée `Diffuse` — si c'est `Diffusion`, qui suit
`Format` (tâche 3), l'étiquette se perdrait : faire alors porter le format par la vue qui précède,
ou émettre `Format` après `Diffusion`, au plus simple, et justifier le choix dans le rapport. Un
test d'enchaînement le prouve (suite d'événements réelle de l'hôte → vue finale avec format).
Mettre à jour le test de sérialisation (vue.rs l.188-200, l.307-316) : clé `"format"`.

- [ ] **Étape 3 : l'interface**

- `types.ts` : `format: string | null` dans `regarde` et `diffuse`.
- `PanneauPartage.tsx` : `<Mesure libelle="Format" valeur={vue.format ?? "—"} />` en tête de la
  liste, pour `regarde` et `diffuse`.
- `Partage.test.tsx` : ajouter `format: null` aux fixtures, et un test : une vue `regarde` avec
  `format: "H.264"` affiche « Format » et « H.264 ».

- [ ] **Étape 4 : textes devenus faux** (spec §6), chacun marqué en commentaire « à valider par le
  propriétaire »

- `caps.rs` (erreur `Dll`) : « Cette machine n'a pas de carte graphique NVIDIA : SkyShare ne peut
  pas partager son écran depuis cette machine. » Le test
  `le_message_d_absence_de_carte_est_celui_de_la_spec` devient
  `le_message_d_absence_de_carte_ne_parle_que_du_partage` (le texte n'est plus celui de la spec du
  jalon 0 : le dire en commentaire).
- `messages.ts`, cause `sans_carte_nvidia` (spectateur ; ne survient plus qu'à l'ouverture de NVDEC
  après une sonde réussie) : « Le décodeur NVIDIA de cette machine n'a pas pu être chargé. Relance
  le visionnage. »
- `messages.ts`, cause `sans_decodage_444` (le flux reçu n'est pas le 4:4:4 annoncé) : « L'image
  reçue n'est pas au format annoncé. Relance le visionnage ; si cela se reproduit, demande à ton
  ami de relancer son partage. »
- `messages.test.ts` : les deux tests suivent.

- [ ] **Étape 5 : vérifier, commit**

Run, depuis `spike/` : `cargo test --workspace --no-fail-fast`, clippy ; depuis `app/` :
`npx tsc --noEmit`, `npx vitest run`, `npm run build`.

```bash
git add spike/crates/sky-app/src/vue.rs spike/crates/sky-app/src/partage.rs spike/crates/sky-encode/src/caps.rs app/src/types.ts app/src/messages.ts app/src/messages.test.ts app/src/composants/PanneauPartage.tsx app/src/composants/Partage.test.tsx
git commit -m "feat(interface): le format négocié parmi les mesures, textes devenus faux réécrits"
```

---

### Tâche 9 : version portable, fiche d'essai, documentation

**Files :**
- Create : `spike/scripts/version-portable.ps1`
- Create : `spike/docs/essai-toutes-cartes.md`
- Modify : `CLAUDE.md` (état du projet, « Pas de repli logiciel », pièges, « Où chercher »),
  `tasks/todo.md` (état, textes à valider, risques, mineurs)
- Modify : `.gitignore` (dossier de sortie de la version portable)

- [ ] **Étape 1 : le script** `spike/scripts/version-portable.ps1` (PowerShell 5.1 : pas de `&&`,
  pas de `?:` ; arrêt explicite sur `$LASTEXITCODE -ne 0` après chaque commande native)

1. `npm run build` dans `app/`.
2. Depuis `spike/crates/sky-app` : `..\..\..\app\node_modules\.bin\tauri.cmd build` (la seule
   commande de construction qui fonctionne, `CLAUDE.md`).
3. Copier `spike\target\release\sky-app.exe` en `dist\SkyShare-portable\SkyShare.exe` (à la racine
   du dépôt) et `spike\docs\essai-toutes-cartes.md` à côté.
4. Contrôle par `dumpbin /dependents` (chemin trouvé par `Get-ChildItem` sous
   `C:\Program Files (x86)\Microsoft Visual Studio`, `Hostx64\x64`) : **aucune** DLL dont le nom
   commence par `nv` (`nvcuda`, `nvcuvid`, `nvEncodeAPI64`) — sinon l'exécutable ne démarrerait
   pas sur le portable AMD — et présence de `mfplat.dll` (spec §9 : dépendance de chargement
   assumée). Échec explicite sinon.
5. `Compress-Archive` vers `dist\SkyShare-portable.zip`.
6. Afficher la taille de l'archive et le SHA-256 de `SkyShare.exe`.

Ajouter `/dist/` à `.gitignore`.

- [ ] **Étape 2 : exécuter le script**, garder sa sortie dans le rapport. Puis vérifier
  `git diff --ignore-cr-at-eol spike/crates/sky-app/Cargo.toml` vide et
  `git checkout -- spike/crates/sky-app/Cargo.toml`.

- [ ] **Étape 3 : la fiche d'essai** `spike/docs/essai-toutes-cartes.md`, sur le modèle de
  `spike/docs/essai-jalon-2.md` (le lire d'abord ; mêmes conventions : commandes exactes, ce qu'il
  faut voir, ce qu'il faut relever, chaque fenêtre annoncée avant de s'ouvrir) :
  - **Essai A, machine du propriétaire seule** : Media Foundation sur NVIDIA. Hôte et spectateur
    exigent deux identités (`view` ne regarde qu'un ami) : reprendre le montage du §3 de la fiche
    du jalon 2 (hôte en `debug` sous le second compte Discord, spectateur
    `sky-probe view --release`), avec `--format h264` puis `--format hevc420` côté spectateur.
    Attendu : « format négocié : H.264 » (puis « HEVC 4:2:0 ») des deux côtés, l'image affichée.
  - **Essai B, deux machines** : la machine NVIDIA partage depuis l'application (compte
    principal) ; le portable AMD lance `SkyShare.exe` (version portable, second compte, ami du
    premier) et clique « Regarder ». Attendu : « Format : HEVC 4:2:0 » (ou « H.264 » si
    l'extension HEVC manque), l'image.
  - **À relever** : le format affiché, les images reçues par seconde, la latence de décodage, et
    la **latence de bout en bout** : afficher un chronomètre à la milliseconde sur l'écran partagé
    et photographier ensemble l'écran de l'hôte et celui du portable ; l'écart entre les deux
    chronomètres est la latence (spec §2, inconnue 1).
  - **Ce qui doit faire arrêter l'essai** : une image figée sans message, des couleurs inversées
    (rouge et bleu), une fenêtre noire muette.

- [ ] **Étape 4 : documentation**

- `CLAUDE.md` : dans « Où en est le projet », un paragraphe « Jalon toutes cartes, sous-jalon 1 :
  IMPLÉMENTÉ, essai dû » (date, branche, nombre de tests relevé à la fin de la tâche 8) ; réécrire
  le paragraphe « Pas de repli logiciel » des questions ouvertes : **sans NVIDIA, une machine peut
  désormais recevoir** (HEVC 4:2:0 ou H.264 par Media Foundation, en matériel) mais pas partager
  (sous-jalon 3), et il n'y a toujours **aucun repli logiciel** ; ajouter aux pièges « le décodeur
  HEVC logiciel de Microsoft perd toutes les images après la 120e en faible latence ; le moteur
  refuse toute image hors GPU » ; ajouter la spec, le plan et la fiche dans « Où chercher ».
- `tasks/todo.md` : l'état du sous-jalon ; les textes à valider (`MESSAGE_AUCUN_FORMAT`,
  `MESSAGE_AUCUN_FORMAT_COMMUN`, `MESSAGE_AUCUN_DECODEUR`, et les trois réécrits de la tâche 8) ;
  les risques de la spec §9 ; les mineurs reportés par les revues.

- [ ] **Étape 5 : commit**

```bash
git add spike/scripts/version-portable.ps1 spike/docs/essai-toutes-cartes.md CLAUDE.md tasks/todo.md .gitignore
git commit -m "docs: version portable et fiche d'essai du sous-jalon 1 toutes cartes"
```

---

## Hors plan, après la revue finale

L'essai (fiche de la tâche 9) appartient au propriétaire. La fusion de `jalon-toutes-cartes`
attend celle de `jalon-2-premier-pixel`, elle-même suspendue à l'essai local (décision du
propriétaire).
