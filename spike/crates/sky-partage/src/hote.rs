//! Côté hôte : attendre la demande d'un ami, y répondre, puis capturer,
//! encoder et envoyer le flux. Déplacé de `sky-probe/src/cmd_host.rs` (C2) :
//! chaque `println!` y est devenu un `Evenement`, chaque `return Ok(())` une
//! `Fin`, chaque `?` est resté un `?`.
//!
//! `WgcCapture::next_frame()` → `NvencEncoder::encode()` → `link.send()`,
//! avec le débit réellement piloté par `Pacer::target_bps()`.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sky_capture::wgc::WgcCapture;
use sky_capture::CapturedFrame;
use sky_compte::{deposer, relever, Coffre, Config, ErreurCompte, Etat};
use sky_crypto::Identity;
use sky_encode::{nvenc::NvencEncoder, Codec};
use sky_net::{LinkEvent, Pacer, PeerLink};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

use crate::arret::{synchroniser_sauf_arret, Arret, ErreurAttente, HorlogeArretable};
use crate::etablissement::{etablir, Etablissement};
use crate::evenement::{Bilan, BilanEnvoi, ErreurPartage, Evenement, Fin, Mesures, Quantiles};
use crate::rendez_vous::{echec_local, interroger, offres_recevables, CADENCE, FENETRE_HOTE};

/// Cadence visée pour l'encodage. `sky-probe` la reprend (`cmd_encode::FPS`).
pub const FPS: u32 = 60;

/// En-tête préfixé à chaque MORCEAU de message envoyé : 8 octets
/// d'horodatage (microsecondes depuis l'époque Unix, horloge **système** —
/// pas `Instant`, propre à un seul processus) puis 1 octet drapeau (non nul
/// = premier morceau d'une image encodée).
///
/// Les deux bords tournent sur la même machine pendant ce spike, donc
/// l'horloge système est directement comparable entre eux — c'est ce qui
/// permet au spectateur de mesurer un temps de transit, et à nous de calculer
/// un RTT quand il nous renvoie l'horodatage tel quel dans son retour.
///
/// Ce préfixe ne quitte jamais le processus spectateur : `cmd_view` l'ôte
/// avant d'écrire quoi que ce soit dans `recu.h265`, qui reste un flux HEVC
/// brut, structurellement valide pour `ffprobe` — le découpage en morceaux
/// est un détail de transport, invisible dans le fichier écrit.
pub const EN_TETE_MORCEAU: usize = 9;

/// Taille maximale de la charge utile d'un morceau — reprend la taille de
/// message (1200 octets) avec laquelle la Tâche 7 a mesuré un débit soutenu
/// de 182 à 246 Mbps en boucle locale. Découverte de ce banc de test :
/// envoyer un paquet NVENC entier en un seul message (jusqu'à ~40 Ko à
/// 20 Mbps/60 i/s, plafonné par le tampon VBV d'une image) sature le tampon
/// d'émission de `str0m` de façon soutenue et fait échouer des envois — alors
/// que des messages de cette taille-ci s'écoulent sans accroc. Chaque paquet
/// NVENC est donc redécoupé ici ; le spectateur ne fait que recoller les
/// morceaux dans l'ordre, le flux Annex B écrit sur disque est identique.
pub const TAILLE_MORCEAU_PAYLOAD: usize = 1200 - EN_TETE_MORCEAU;

/// Granularité à laquelle le lien est servi pendant les attentes de la
/// boucle principale (cadence FPS, capture écran, relance d'un envoi en
/// échec). Découverte de banc de test : un seul sommeil de ~16 ms entre deux
/// trames (première version de cette boucle) laisse `str0m` sans service
/// pendant tout ce temps. `str0m` est sans-IO : ses accusés de réception ne
/// sont traités que lorsqu'on l'interroge, et son contrôle de flux SCTP a
/// besoin d'un service bien plus fréquent que 60 Hz pour garder sa fenêtre
/// ouverte. Mesuré avant correction : RTT médian ~1,1 s et ~27 % d'échecs
/// d'envoi à 20 Mbps en boucle locale — un artefact du banc de test, pas du
/// réseau. Avec un service toutes les millisecondes, ces deux chiffres
/// retombent à des valeurs de boucle locale plausibles (voir le rapport).
const GRANULARITE_SERVICE_RESEAU: Duration = Duration::from_millis(1);

/// Budget de relance pour un morceau déjà encodé qui échoue à partir — voir
/// `envoyer_ou_abandonner`. Généreux par rapport aux quelques millisecondes
/// de résorption observées en boucle locale une fois les morceaux réduits à
/// `TAILLE_MORCEAU_PAYLOAD` : ce budget n'est atteint qu'en cas de congestion
/// soutenue et anormale.
pub const BUDGET_RETRY_ENVOI: Duration = Duration::from_millis(300);

pub fn epoch_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

/// Une source d'images sur le device de la capture — la texture synthétique
/// de `sky-probe` l'implémente.
pub trait Images {
    fn prochaine_image(&mut self) -> anyhow::Result<CapturedFrame>;
}

/// Fabrique la source synthétique sur le device de la capture.
pub type FabriqueSynthetique = fn(&ID3D11Device, u32, u32) -> anyhow::Result<Box<dyn Images>>;

pub enum SourceImages {
    Ecran,
    Synthetique { largeur: u32, hauteur: u32, fabrique: FabriqueSynthetique },
}

pub struct ParametresHote {
    pub codec: Codec,
    /// Débit cible de NVENC — aussi le plafond du Pacer.
    pub plafond_mbps: u32,
    pub plancher_mbps: u32,
    /// Écran capturé (ordre d'`EnumDisplayMonitors`). Le device de cet écran
    /// sert aussi à la source synthétique, comme au C2.
    pub moniteur: usize,
    pub source: SourceImages,
    /// `None` : jusqu'à l'arrêt (application). `Some` : `sky-probe --seconds`.
    pub duree_max: Option<Duration>,
}

pub fn heberger(
    config: &Config,
    coffre: &Coffre,
    mut synchroniser: impl FnMut(Option<&Etat>) -> Result<Etat, ErreurCompte>,
    p: ParametresHote,
    arret: &Arret,
    evenements: &mut dyn FnMut(Evenement),
) -> Result<Fin, ErreurPartage> {
    // Construit AVANT la négociation (C2) : une faute de bornes doit coûter
    // une seconde, pas une attente de FENETRE_HOTE.
    let mut pacer =
        Pacer::new(p.plancher_mbps * 1_000_000, p.plafond_mbps * 1_000_000).map_err(anyhow::Error::from)?;
    evenements(Evenement::Pret);

    // Sans appareil enregistré, personne ne peut nous adresser de demande.
    if coffre.identifiant_appareil().map_err(ErreurPartage::Compte)?.is_none() {
        return Ok(Fin::AucunAppareilLocal);
    }
    let identite = coffre.identite().map_err(ErreurPartage::Compte)?;
    evenements(Evenement::Disponible { fenetre: FENETRE_HOTE });

    let debut_attente = Instant::now();
    let mut synchronisations = 0u32;
    let mut horloge = HorlogeArretable::demarrer(arret);
    let attente = interroger(
        None,
        synchroniser_sauf_arret(arret, |precedent| {
            synchronisations += 1;
            synchroniser(precedent)
        }),
        |etat| {
            // Recevable ne veut pas dire utilisable : si `repondant` refuse le
            // SDP, on essaie l'offre suivante sans interrompre l'attente.
            for offre in offres_recevables(etat, relever(etat, &identite)) {
                match PeerLink::repondant(Identity::generate(), &offre.texte) {
                    Ok((link, reponse)) => return Some((link, reponse, offre.destinataire)),
                    Err(e) => {
                        let raison = e.to_string();
                        // Cause LOCALE : la dire « écartée » ferait porter la
                        // faute à l'ami. Rien n'est retenté (C2, revue I2).
                        if echec_local(&raison) {
                            evenements(Evenement::EchecLocal { raison });
                        } else {
                            evenements(Evenement::DemandeEcartee { raison });
                        }
                    }
                }
            }
            None
        },
        &mut horloge,
        CADENCE,
        FENETRE_HOTE,
    );
    let (mut link, reponse, destinataire) = match attente {
        Ok(Some(retenue)) => retenue,
        Ok(None) => return Ok(Fin::AucuneDemande),
        Err(ErreurAttente::Arrete) => return Ok(Fin::Arrete),
        Err(ErreurAttente::Compte(e)) => return Err(ErreurPartage::Compte(e)),
    };
    evenements(Evenement::DemandeRecue {
        expediteur_device_id: destinataire.id,
        apres: debut_attente.elapsed(),
        synchronisations,
    });

    // L'enveloppe est scellée par `deposer` pour la clé d'ANNUAIRE de
    // l'appareil expéditeur (voir `rendez_vous::destinataire_de_la_reponse`).
    let deposes = deposer(config, coffre, std::slice::from_ref(&destinataire), reponse.as_bytes())
        .map_err(ErreurPartage::Compte)?;
    if deposes == 0 {
        return Ok(Fin::ReponseRefusee);
    }
    evenements(Evenement::Negociation);

    let garde = link.maintenir_mapping()?;
    let duree = match etablir(&mut link, arret)? {
        Etablissement::Ouvert(duree) => duree,
        Etablissement::Rompu(raison) => return Ok(Fin::NegociationRompue(raison)),
        Etablissement::Delai(diagnostic) => return Ok(Fin::EtablissementEchoue(diagnostic)),
        Etablissement::Arrete => return Ok(Fin::Arrete),
    };
    drop(garde);
    evenements(Evenement::Connecte { en: duree, depuis_le_lancement: None });

    diffuser(&mut link, &mut pacer, p, arret, evenements)
}

fn diffuser(
    link: &mut PeerLink,
    pacer: &mut Pacer,
    p: ParametresHote,
    arret: &Arret,
    evenements: &mut dyn FnMut(Evenement),
) -> Result<Fin, ErreurPartage> {
    let mut cap = WgcCapture::new(p.moniteur, None)?;
    let (largeur, hauteur, mut synth): (u32, u32, Option<Box<dyn Images>>) = match p.source {
        SourceImages::Ecran => {
            let attente_max = Instant::now() + Duration::from_secs(5);
            let premiere = loop {
                if let Some(f) = cap.next_frame(Duration::from_millis(200))? {
                    break f;
                }
                if Instant::now() >= attente_max {
                    return Err(anyhow::anyhow!("aucune image capturée en 5 s — l'écran est-il figé ?").into());
                }
            };
            (premiere.width, premiere.height, None)
        }
        SourceImages::Synthetique { largeur, hauteur, fabrique } => {
            let s = fabrique(cap.d3d_device(), largeur, hauteur)?;
            (largeur, hauteur, Some(s))
        }
    };

    let mut enc = NvencEncoder::new(cap.d3d_device(), p.codec, largeur, hauteur, FPS, p.plafond_mbps * 1_000_000)?;
    evenements(Evenement::Diffusion {
        largeur,
        hauteur,
        codec: p.codec,
        plancher_mbps: p.plancher_mbps,
        plafond_mbps: p.plafond_mbps,
    });

    let t0 = Instant::now();
    let fin = p.duree_max.map(|d| t0 + d);
    let periode = Duration::from_micros(1_000_000 / FPS as u64);
    let mut prochaine_image = Instant::now();
    let mut budget_octets = 0.0f64;
    let mut dernier_budget = Instant::now();
    let mut envoyes_octets = 0u64;
    let mut images_encodees = 0u64;
    let mut images_sautees = 0u64;
    let mut retours = 0u64;
    let mut echecs_send = 0u64;
    let mut tentatives_send = 0u64;
    let mut echantillons_encode_us: Vec<u64> = Vec::new();
    let mut dernier_rtt_ms: f64 = 0.0;
    let mut echantillons_rtt: Vec<f64> = Vec::new();
    let mut dernier_feedback = Instant::now();
    let mut fenetre_tentatives = 0u64;
    let mut fenetre_echecs = 0u64;
    let mut dernier_affichage = Instant::now();
    let mut octets_precedent = 0u64;

    loop {
        if arret.est_demande() {
            return Ok(Fin::Arrete);
        }
        if fin.is_some_and(|f| Instant::now() >= f) {
            break;
        }

        // 1. Recharger le budget, plafonné à 250 ms de crédit.
        let maintenant = Instant::now();
        let dt = maintenant.duration_since(dernier_budget).as_secs_f64();
        dernier_budget = maintenant;
        budget_octets += dt * pacer.target_bps() as f64 / 8.0;
        budget_octets = budget_octets.min(pacer.target_bps() as f64 / 8.0 * 0.25);

        // 2. Image suivante, le lien servi PENDANT l'attente.
        let image = match &mut synth {
            Some(s) => {
                loop {
                    let m = Instant::now();
                    if m >= prochaine_image {
                        break;
                    }
                    if let Some(raison) = servir_reseau(link, &mut retours, &mut dernier_rtt_ms, &mut echantillons_rtt)? {
                        return Ok(Fin::LienTombe(raison));
                    }
                    std::thread::sleep(GRANULARITE_SERVICE_RESEAU.min(prochaine_image.saturating_duration_since(m)));
                }
                prochaine_image += periode;
                Some(s.prochaine_image()?)
            }
            None => {
                let mut trouvee = None;
                let echeance = Instant::now() + Duration::from_millis(50);
                while Instant::now() < echeance {
                    if let Some(raison) = servir_reseau(link, &mut retours, &mut dernier_rtt_ms, &mut echantillons_rtt)? {
                        return Ok(Fin::LienTombe(raison));
                    }
                    if let Some(f) = cap.next_frame(GRANULARITE_SERVICE_RESEAU)? {
                        trouvee = Some(f);
                        break;
                    }
                }
                trouvee
            }
        };

        if let Some(image) = image {
            if budget_octets < 0.0 {
                // Budget épuisé : on saute la CAPTURE→ENCODAGE, jamais un
                // paquet déjà produit (GOP infini : le flux resterait cohérent).
                images_sautees += 1;
            } else if let Some(pkt) = enc.encode(&image)? {
                images_encodees += 1;
                budget_octets -= pkt.data.len() as f64;
                echantillons_encode_us.push(pkt.encode_us);

                let horodatage = epoch_us();
                let nb_morceaux = pkt.data.chunks(TAILLE_MORCEAU_PAYLOAD).count().max(1);
                for (i, morceau) in pkt.data.chunks(TAILLE_MORCEAU_PAYLOAD).enumerate() {
                    let mut charge = Vec::with_capacity(EN_TETE_MORCEAU + morceau.len());
                    charge.extend_from_slice(&horodatage.to_le_bytes());
                    charge.push(if i == 0 { 1 } else { 0 });
                    charge.extend_from_slice(morceau);

                    tentatives_send += 1;
                    match envoyer_ou_abandonner(
                        link,
                        &charge,
                        &mut envoyes_octets,
                        &mut echecs_send,
                        &mut fenetre_tentatives,
                        &mut fenetre_echecs,
                        &mut retours,
                        &mut dernier_rtt_ms,
                        &mut echantillons_rtt,
                    )? {
                        ResultatEnvoi::Envoye => {}
                        ResultatEnvoi::Abandonne => {
                            return Ok(Fin::TamponSature { morceau: i + 1, morceaux: nb_morceaux });
                        }
                        ResultatEnvoi::LienTombe(raison) => return Ok(Fin::LienTombe(raison)),
                    }
                }
            }
        }

        // 3. Un dernier service réseau après l'encodage/envoi.
        if let Some(raison) = servir_reseau(link, &mut retours, &mut dernier_rtt_ms, &mut echantillons_rtt)? {
            return Ok(Fin::LienTombe(raison));
        }

        // 4. Nourrir le régulateur de débit à ~10 Hz.
        if dernier_feedback.elapsed() >= Duration::from_millis(100) {
            let perte_pct = if fenetre_tentatives > 0 {
                fenetre_echecs as f32 / fenetre_tentatives as f32 * 100.0
            } else {
                0.0
            };
            pacer.on_feedback(perte_pct, dernier_rtt_ms.round() as u32, dernier_feedback.elapsed());
            fenetre_tentatives = 0;
            fenetre_echecs = 0;
            dernier_feedback = Instant::now();
        }

        // 5. Une mesure par seconde.
        if dernier_affichage.elapsed() >= Duration::from_secs(1) {
            let delta = envoyes_octets - octets_precedent;
            let ecoule = dernier_affichage.elapsed().as_secs_f64();
            evenements(Evenement::Mesures(Mesures::Envoi {
                debit_mbps: delta as f64 * 8.0 / ecoule / 1e6,
                cible_mbps: pacer.target_bps() as f64 / 1e6,
                images_sautees,
                rtt_ms: dernier_rtt_ms,
            }));
            octets_precedent = envoyes_octets;
            dernier_affichage = Instant::now();
        }
    }

    let duree_s = t0.elapsed().as_secs_f64();
    echantillons_rtt.sort_by(|a, b| a.partial_cmp(b).unwrap());
    echantillons_encode_us.sort_unstable();
    let encodage_ms = (!echantillons_encode_us.is_empty()).then(|| Quantiles {
        p50: percentile_u64(&echantillons_encode_us, 50) as f64 / 1000.0,
        p99: percentile_u64(&echantillons_encode_us, 99) as f64 / 1000.0,
        echantillons: echantillons_encode_us.len(),
    });
    let rtt_ms = (!echantillons_rtt.is_empty()).then(|| Quantiles {
        p50: percentile_f64(&echantillons_rtt, 50),
        p99: percentile_f64(&echantillons_rtt, 99),
        echantillons: echantillons_rtt.len(),
    });
    let (_, vers_internet) = link.destinations();
    Ok(Fin::DureeEcoulee(Box::new(Bilan::Envoi(BilanEnvoi {
        duree_s,
        images_encodees,
        images_sautees,
        encodage_ms,
        envoyes_octets,
        cible_finale_bps: pacer.target_bps(),
        retours,
        echecs_envoi: echecs_send,
        tentatives_envoi: tentatives_send,
        rtt_ms,
        vers_internet,
    }))))
}

/// Un tour de service réseau : vide un événement de `link`, compte les
/// retours et met à jour l'échantillon de RTT (l'horodatage que le
/// spectateur renvoie tel quel). Factorisée parce qu'elle est appelée à
/// plusieurs points de la boucle principale — voir `GRANULARITE_SERVICE_RESEAU`
/// pour pourquoi la fréquence d'appel compte ici.
///
/// Rend `Some(raison)` si le lien est tombé ; l'appelant doit alors arrêter.
fn servir_reseau(
    link: &mut PeerLink,
    retours: &mut u64,
    dernier_rtt_ms: &mut f64,
    echantillons_rtt: &mut Vec<f64>,
) -> anyhow::Result<Option<String>> {
    match link.poll()? {
        LinkEvent::Data(d) => {
            *retours += 1;
            if let Ok(brut) = <[u8; 8]>::try_from(d.as_slice()) {
                let echo = u64::from_le_bytes(brut);
                let maintenant_us = epoch_us();
                if maintenant_us >= echo {
                    let rtt_ms = (maintenant_us - echo) as f64 / 1000.0;
                    *dernier_rtt_ms = rtt_ms;
                    echantillons_rtt.push(rtt_ms);
                }
            }
            Ok(None)
        }
        LinkEvent::Failed(raison) => Ok(Some(raison)),
        _ => Ok(None),
    }
}

/// Issue d'une tentative d'envoi d'un morceau déjà encodé.
enum ResultatEnvoi {
    /// Parti — au premier coup ou après relance.
    Envoye,
    /// Le tampon d'émission est resté plein au-delà de `BUDGET_RETRY_ENVOI`.
    Abandonne,
    /// Le lien est tombé pendant l'attente ; raison déjà rédigée pour
    /// l'utilisateur, sans adresse.
    LienTombe(String),
}

/// Envoie un morceau déjà découpé, en relançant tant que le tampon
/// d'émission est plein, jusqu'à `BUDGET_RETRY_ENVOI`. Sert le lien entre
/// chaque tentative (`servir_reseau`) : c'est ce service qui vide le tampon
/// en traitant les accusés de réception de `str0m`.
///
/// Un morceau déjà encodé DOIT partir : NVENC vient de chaîner l'image dont
/// il fait partie sur la dernière qu'il a réellement encodée (GOP infini,
/// frameIntervalP = 1), et sous ce régime aucune image de référence ne
/// revient jamais. L'abandonner silencieusement casserait cette chaîne pour
/// tout le reste du flux — c'est précisément ce qu'a révélé un premier essai
/// de ce banc de test : ffprobe rapportait des « ref POC introuvable » en
/// cascade dès le premier échec d'envoi ignoré. D'où la relance, et l'arrêt
/// propre plutôt qu'une mesure sur un flux qu'on sait corrompu.
#[allow(clippy::too_many_arguments)]
fn envoyer_ou_abandonner(
    link: &mut PeerLink,
    charge: &[u8],
    envoyes_octets: &mut u64,
    echecs_send: &mut u64,
    fenetre_tentatives: &mut u64,
    fenetre_echecs: &mut u64,
    retours: &mut u64,
    dernier_rtt_ms: &mut f64,
    echantillons_rtt: &mut Vec<f64>,
) -> anyhow::Result<ResultatEnvoi> {
    let debut = Instant::now();
    let mut retente = false;
    loop {
        match link.send(charge) {
            Ok(()) => {
                *envoyes_octets += charge.len() as u64;
                if retente {
                    // Congestion réelle, résorbée : signal légitime pour le
                    // Pacer, même si le morceau est finalement parti.
                    *echecs_send += 1;
                    *fenetre_echecs += 1;
                }
                *fenetre_tentatives += 1;
                // `link.send` ne fait que déposer le morceau dans le tampon
                // interne de `str0m` : les octets ne partent réellement sur
                // le socket que pendant `poll()`. Découverte de banc de test :
                // sans ce drainage après CHAQUE morceau, une rafale de
                // plusieurs dizaines de morceaux d'une même image s'empile
                // sans jamais être poussée sur le fil, jusqu'à saturer le
                // tampon — RTT en centaines de ms puis échec, en boucle
                // locale. Un morceau parti selon `str0m` n'est pas encore un
                // morceau émis sur le réseau.
                if let Some(raison) =
                    servir_reseau(link, retours, dernier_rtt_ms, echantillons_rtt)?
                {
                    return Ok(ResultatEnvoi::LienTombe(raison));
                }
                return Ok(ResultatEnvoi::Envoye);
            }
            Err(_) if debut.elapsed() < BUDGET_RETRY_ENVOI => {
                retente = true;
                if let Some(raison) =
                    servir_reseau(link, retours, dernier_rtt_ms, echantillons_rtt)?
                {
                    return Ok(ResultatEnvoi::LienTombe(raison));
                }
                std::thread::sleep(GRANULARITE_SERVICE_RESEAU);
            }
            Err(_) => return Ok(ResultatEnvoi::Abandonne),
        }
    }
}

fn percentile_f64(tries: &[f64], p: usize) -> f64 {
    if tries.is_empty() {
        return 0.0;
    }
    let idx = (tries.len() * p / 100).min(tries.len() - 1);
    tries[idx]
}

fn percentile_u64(tries: &[u64], p: usize) -> u64 {
    if tries.is_empty() {
        return 0;
    }
    let idx = (tries.len() * p / 100).min(tries.len() - 1);
    tries[idx]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn un_arret_pendant_l_attente_termine_heberger_sans_attendre_la_fenetre() {
        // L'arrêt est demandé dès la PREMIÈRE synchronisation : c'est alors
        // l'horloge arrêtable, et elle seule, qui évite l'attente de CADENCE
        // (2 s) avant que la synchronisation suivante ne constate l'arrêt.
        // Neutralisation : `HorlogeReelle::demarrer()` au lieu de
        // `HorlogeArretable::demarrer(arret)` — `heberger` rend bien
        // `Fin::Arrete`, mais après ~2 s : la durée rougit. Le fil et le
        // `recv_timeout` ne sont qu'un garde-fou (fenêtre de 30 minutes).
        let (envoi, reception) = mpsc::channel();
        std::thread::spawn(move || {
            let coffre = Coffre::pour_test("sky-test-partage-arret-hote");
            coffre.ranger_identifiant_appareil(1).unwrap();
            let config = Config::vers("http://127.0.0.1:1");
            let arret = Arret::nouveau();
            let mut appels = 0u32;
            let mut evenements = Vec::new();
            let debut = Instant::now();
            let fin = heberger(
                &config,
                &coffre,
                |_| {
                    appels += 1;
                    arret.demander();
                    Ok(Etat {
                        version: 1,
                        code: "ABCDEFGH".to_string(),
                        amis: Vec::new(),
                        demandes: Vec::new(),
                        listes: Vec::new(),
                        appareils: Vec::new(),
                        enveloppes: Vec::new(),
                    })
                },
                ParametresHote {
                    codec: Codec::Hevc444,
                    plafond_mbps: 30,
                    plancher_mbps: 10,
                    moniteur: 0,
                    source: SourceImages::Ecran,
                    duree_max: None,
                },
                &arret,
                &mut |e| evenements.push(e),
            );
            let duree = debut.elapsed();
            let _ = envoi.send((fin.map_err(|e| format!("{e:?}")), duree, appels, evenements));
        });
        let (fin, duree, appels, evenements) =
            reception.recv_timeout(Duration::from_secs(20)).expect("heberger n'a pas rendu la main en 20 s");
        assert_eq!(fin, Ok(Fin::Arrete));
        assert!(duree < Duration::from_secs(1), "heberger a attendu {duree:?} après l'arrêt");
        assert_eq!(appels, 1, "aucune synchronisation après l'arrêt");
        assert_eq!(evenements, vec![Evenement::Pret, Evenement::Disponible { fenetre: FENETRE_HOTE }]);
    }
}
