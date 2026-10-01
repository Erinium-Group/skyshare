//! Côté hôte : attendre la demande d'un ami, y répondre, puis capturer,
//! encoder et envoyer le flux. Déplacé de `sky-probe/src/cmd_host.rs` (C2) :
//! chaque `println!` y est devenu un `Evenement`, chaque `return Ok(())` une
//! `Fin`, chaque `?` est resté un `?`.
//!
//! `WgcCapture::next_frame()` → `NvencEncoder::encode()` → `PeerLink::ecrire_image()`,
//! avec le débit réellement piloté par `Pacer::target_bps()`.

use std::borrow::Cow;
use std::time::{Duration, Instant};

use sky_capture::wgc::WgcCapture;
use sky_capture::CapturedFrame;
use sky_compte::{deposer, relever, Coffre, Config, ErreurCompte, Etat};
use sky_crypto::Identity;
use sky_encode::{nvenc::NvencEncoder, Codec};
use sky_net::{ErreurEnvoi, LinkEvent, MessageControle, Pacer, PeerLink};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

use crate::arret::{synchroniser_sauf_arret, Arret, ErreurAttente, HorlogeArretable};
use crate::etablissement::{etablir, Etablissement};
use crate::evenement::{Bilan, BilanEnvoi, ErreurPartage, Evenement, Fin, Mesures, Quantiles};
use crate::rendez_vous::{echec_local, interroger, offres_recevables, CADENCE, FENETRE_HOTE};

/// Cadence visée pour l'encodage. `sky-probe` la reprend (`cmd_encode::FPS`).
pub const FPS: u32 = 60;

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

/// Temps pendant lequel le lien est encore servi après l'annonce de l'arrêt,
/// pour que le message ait le temps de partir — voir `annoncer_l_arret`, qui
/// abrège ce délai dès que le lien est déclaré perdu. Un aller simple sur un
/// chemin direct se compte en dizaines de millisecondes ; ce délai s'ajoute une
/// seule fois, à la toute fin d'un partage.
const DRAINAGE_ARRET: Duration = Duration::from_millis(50);

/// Budget de relance pour une image déjà encodée que la piste média refuse
/// encore — voir `EnvoiVideo::envoyer_image`. Le seul refus possible y est
/// `TropDImagesEnAttente`, et il se résorbe d'un `poll` par place ; ce budget
/// n'est donc atteint que si le correspondant ne consomme plus rien du tout.
/// Mesuré sur le chemin média : 0 refus sur 2593 écritures à 12 Mbps et 0 sur
/// 21552 à 100 Mbps quand la boucle sert le réseau entre deux images.
pub const BUDGET_RETRY_ENVOI: Duration = Duration::from_millis(300);

/// Ce que la boucle d'envoi attend du lien pair-à-pair. `PeerLink` en est la
/// seule implémentation réelle ; `doublure::LienFactice` l'imite dans les
/// tests, où il n'y a ni réseau ni correspondant.
pub trait LienVideo {
    fn ecrire_image(&mut self, unite: &[u8], horodatage_ms: u64) -> Result<(), ErreurEnvoi>;
    fn envoyer_controle(&mut self, message: &MessageControle) -> Result<(), ErreurEnvoi>;
    fn poll(&mut self) -> anyhow::Result<LinkEvent>;
}

impl LienVideo for PeerLink {
    fn ecrire_image(&mut self, unite: &[u8], horodatage_ms: u64) -> Result<(), ErreurEnvoi> {
        PeerLink::ecrire_image(self, unite, horodatage_ms)
    }

    fn envoyer_controle(&mut self, message: &MessageControle) -> Result<(), ErreurEnvoi> {
        PeerLink::envoyer_controle(self, message)
    }

    fn poll(&mut self) -> anyhow::Result<LinkEvent> {
        PeerLink::poll(self)
    }
}

/// Les deux gestes de l'encodeur dont la boucle d'envoi a besoin pour qu'un
/// spectateur puisse prendre le flux en cours de route — ou le reprendre.
///
/// L'encodage lui-même reste sur `NvencEncoder` : il consomme une texture
/// Direct3D, que rien ne double. Ces deux appels-ci n'en ont pas besoin.
pub trait Reprise {
    fn entetes_de_sequence(&self) -> anyhow::Result<Vec<u8>>;
    fn forcer_image_cle(&mut self);
}

impl Reprise for NvencEncoder {
    fn entetes_de_sequence(&self) -> anyhow::Result<Vec<u8>> {
        NvencEncoder::entetes_de_sequence(self)
    }

    fn forcer_image_cle(&mut self) {
        NvencEncoder::forcer_image_cle(self)
    }
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

    en_annoncant_l_arret(&mut link, |lien| diffuser(lien, &mut pacer, p, arret, evenements))
}

/// Enveloppe une diffusion pour que le spectateur apprenne **toujours** qu'elle
/// cesse : fin normale, arrêt demandé, file de paquetisation restée pleine, ou
/// erreur remontée par un `?` — périphérique de capture perdu, écran figé,
/// encodage refusé. Ces erreurs-là laissent le lien VIVANT : sans annonce, le
/// spectateur reste devant une image figée sans explication, et « une fenêtre
/// noire muette est un défaut, pas un état ».
///
/// Un seul appel pour toutes les sorties, plutôt qu'un appel sur chacune, et
/// **sans aucune exception** : ajouter demain une sortie à la boucle ne peut plus
/// oublier l'annonce, et aucune déduction sur l'état du lien ne peut la
/// supprimer à tort (voir le commentaire du corps).
fn en_annoncant_l_arret<L: LienVideo>(
    lien: &mut L,
    diffusion: impl FnOnce(&mut L) -> Result<Fin, ErreurPartage>,
) -> Result<Fin, ErreurPartage> {
    let issue = diffusion(lien);
    // Sans exception, et c'est délibéré. La ronde 1 exemptait `Fin::LienTombe`,
    // sous l'idée qu'il n'y a plus personne à qui parler — mais `Fin::LienTombe`
    // couvre aussi les refus de la PISTE MÉDIA (`PisteFermee`,
    // `CodecNonNegocie`, `EcritureRefusee`), et le canal de données qui porte
    // `PartageArrete` peut alors être parfaitement vivant. L'exception
    // réintroduisait donc, par une déduction fausse, le défaut même qu'elle
    // accompagnait.
    //
    // Annoncer toujours coûte presque rien et ne déduit rien : sur un lien
    // vraiment mort, `envoyer_controle` échoue d'emblée (`ecrire` ne trouve plus
    // le canal, donc `CanalFerme`) et le drainage n'a même pas lieu ; si malgré
    // tout il avait lieu, il s'arrête au premier `LinkEvent::Failed`.
    //
    // L'annonce reste un service rendu au spectateur, jamais une garantie : son
    // échec ne change pas l'issue du partage.
    let _ = annoncer_l_arret(lien);
    issue
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
    let mut envoi = EnvoiVideo::nouveau();
    let mut images_encodees = 0u64;
    let mut images_sautees = 0u64;
    let mut echantillons_encode_us: Vec<u64> = Vec::new();
    let mut dernier_feedback = Instant::now();
    let mut dernier_affichage = Instant::now();
    let mut octets_precedent = 0u64;

    loop {
        if arret.est_demande() {
            // L'annonce de l'arrêt est faite par `en_annoncant_l_arret`, pour
            // cette sortie comme pour toutes les autres.
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
                    if let Some(raison) = envoi.servir(link, &mut enc)? {
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
                    if let Some(raison) = envoi.servir(link, &mut enc)? {
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
            if envoi.sauter_ce_tour(budget_octets) {
                // Budget épuisé : on saute la CAPTURE→ENCODAGE, jamais un
                // paquet déjà produit (GOP infini : le flux resterait cohérent).
                images_sautees += 1;
            } else if let Some(pkt) = enc.encode(&image)? {
                images_encodees += 1;
                budget_octets -= pkt.data.len() as f64;
                echantillons_encode_us.push(pkt.encode_us);

                // Une unité d'accès ENTIÈRE. Le découpage en paquets est le
                // travail du paquetiseur RFC 7798 de `str0m` (décision D1) :
                // l'en-tête maison de 9 octets et la relance morceau par morceau
                // n'ont plus d'objet, puisqu'il n'y a plus de tampon d'émission
                // à saturer — mesuré, 0 refus d'écriture sur 21552 envois à
                // 100 Mbps, contre 16 % sur le canal de données au jalon 0.
                //
                // L'horodatage est une durée depuis le début de la diffusion, en
                // millisecondes : c'est ce que porte l'horloge RTP, et le
                // spectateur n'a aucun besoin de notre heure système.
                let horodatage_ms = t0.elapsed().as_millis() as u64;
                match envoi.envoyer_image(link, &mut enc, &pkt.data, horodatage_ms)? {
                    IssueEnvoi::Envoyee => {}
                    IssueEnvoi::FilePleine => {
                        // Une image est désormais une unité indivisible : « le
                        // morceau 1 sur 1 » est la seule lecture honnête de ces
                        // deux champs, hérités du découpage maison. Ce sont eux
                        // qu'il faudra renommer, pas cet appel.
                        return Ok(Fin::TamponSature { morceau: 1, morceaux: 1 });
                    }
                    // `Fin::LienTombe` est la seule fin existante pour un flux
                    // qui s'interrompt ; la rebaptiser traverse `sky-app`
                    // (tâche 10). L'annonce de l'arrêt, elle, est tentée dans
                    // tous les cas par `en_annoncant_l_arret`.
                    IssueEnvoi::FluxInterrompu(raison) => return Ok(Fin::LienTombe(raison)),
                }
            }
        }

        // 3. Un dernier service réseau après l'encodage/envoi.
        if let Some(raison) = envoi.servir(link, &mut enc)? {
            return Ok(Fin::LienTombe(raison));
        }

        // 4. Nourrir le régulateur de débit à ~10 Hz.
        if dernier_feedback.elapsed() >= Duration::from_millis(100) {
            let perte_pct = if envoi.fenetre_images > 0 {
                envoi.fenetre_refus as f32 / envoi.fenetre_images as f32 * 100.0
            } else {
                0.0
            };
            // LIMITE CONNUE, ÉCRITE EXPRÈS. Ce que le régulateur reçoit ici, et
            // ce que ça vaut — en distinguant le mesuré du possible :
            //   — `rtt_ms` : 0, passé en dur. Celui-là est bel et bien MORT :
            //     plus aucun écho d'horodatage ne revient à l'hôte (la tâche 5 a
            //     retiré l'écho du canal de données, et la piste média ne remonte
            //     rien ici). `SEUIL_RTT_MS` ne peut donc jamais être franchi.
            //   — `perte_pct` : SILENCIEUSE, pas morte. Elle ne compte que les
            //     `TropDImagesEnAttente` rattrapés, et la sonde du 27/09/2026 en
            //     a MESURÉ zéro sur 21552 écritures à 100 Mbps quand la boucle
            //     sert le réseau entre deux images. Ce zéro est une mesure, pas
            //     une propriété du code : `fenetre_refus` s'incrémente dès qu'un
            //     refus est rattrapé, et `SEUIL_PERTE` valant 2 %, un seul refus
            //     dans une fenêtre de moins de cinquante images suffit à déclarer
            //     la congestion — à 60 i/s une fenêtre de 100 ms en contient
            //     environ six, donc un refus y pèse ~16 %. Une file arriérée fait
            //     donc réagir le régulateur.
            // Conséquence, à lire comme telle : SUR LE CHEMIN NOMINAL MESURÉ,
            // aucune des deux entrées ne signale rien, la cible monte de 8 % par
            // tic jusqu'au plafond et y reste — le débit du jalon 2 est le
            // plafond choisi par l'utilisateur, et `enable_bwe` n'étant pas
            // activé, le pacer de `str0m` est un `NullPacer` : rien d'autre ne
            // régule en dessous. Ce n'est pas « toujours » : c'est ce qui a été
            // observé, et seule la perte peut encore le démentir. Acceptable pour
            // un jalon qui vise le premier pixel ; la source à retrouver est
            // RTCP, présent sur la piste média et lu par personne (écart 7).
            pacer.on_feedback(perte_pct, 0, dernier_feedback.elapsed());
            envoi.fenetre_images = 0;
            envoi.fenetre_refus = 0;
            dernier_feedback = Instant::now();
        }

        // 5. Une mesure par seconde.
        if dernier_affichage.elapsed() >= Duration::from_secs(1) {
            let delta = envoi.octets - octets_precedent;
            let ecoule = dernier_affichage.elapsed().as_secs_f64();
            evenements(Evenement::Mesures(Mesures::Envoi {
                debit_mbps: delta as f64 * 8.0 / ecoule / 1e6,
                cible_mbps: pacer.target_bps() as f64 / 1e6,
                images_sautees,
                // Sans écho, aucun RTT à annoncer — 0 dit « pas de mesure »,
                // comme `rtt_ms: None` dans le bilan final.
                rtt_ms: 0.0,
            }));
            octets_precedent = envoi.octets;
            dernier_affichage = Instant::now();
        }
    }

    let duree_s = t0.elapsed().as_secs_f64();
    echantillons_encode_us.sort_unstable();
    let encodage_ms = (!echantillons_encode_us.is_empty()).then(|| Quantiles {
        p50: percentile_u64(&echantillons_encode_us, 50) as f64 / 1000.0,
        p99: percentile_u64(&echantillons_encode_us, 99) as f64 / 1000.0,
        echantillons: echantillons_encode_us.len(),
    });
    let (_, vers_internet) = link.destinations();
    Ok(Fin::DureeEcoulee(Box::new(Bilan::Envoi(BilanEnvoi {
        duree_s,
        images_encodees,
        images_sautees,
        encodage_ms,
        envoyes_octets: envoi.octets,
        cible_finale_bps: pacer.target_bps(),
        // Deux champs sans source côté hôte depuis que la vidéo est passée sur
        // la piste média : le spectateur ne nous renvoie plus l'horodatage, donc
        // ni retour à compter ni RTT à mesurer. Les laisser vides plutôt que de
        // les remplir d'un zéro déguisé en mesure.
        retours: 0,
        refus_absorbes: envoi.refus_absorbes,
        images_ecrites: envoi.images,
        rtt_ms: None,
        vers_internet,
    }))))
}

/// Issue d'une écriture d'image sur la piste média.
enum IssueEnvoi {
    /// Partie — au premier coup, ou après avoir laissé la file se dépiler.
    Envoyee,
    /// La file de paquetisation est restée pleine au-delà de
    /// `BUDGET_RETRY_ENVOI` : le correspondant ne consomme plus rien.
    FilePleine,
    /// Plus rien ne peut être écrit : soit le lien est tombé
    /// (`LinkEvent::Failed`), soit la piste média ne prend plus d'image
    /// (`PisteFermee`, `CodecNonNegocie`, `EcritureRefusee`).
    ///
    /// Les deux cas ne se valent pas — dans le second, le canal de données peut
    /// rester vivant — et c'est pourquoi la variante ne s'appelle pas
    /// « LienTombe » : ce nom a déjà fait déduire à tort qu'il n'y avait plus
    /// personne à prévenir. Raison déjà rédigée pour l'utilisateur, sans adresse.
    FluxInterrompu(String),
}

/// L'émission vidéo vers un spectateur : ce qu'il lui reste à apprendre du
/// flux, ce que ses demandes déclenchent, et ce que l'envoi a coûté.
///
/// Le découpage n'est plus ici : une unité d'accès part ENTIÈRE, et c'est le
/// paquetiseur RFC 7798 de `str0m` qui la met en paquets RTP (décision D1).
struct EnvoiVideo {
    /// Vrai jusqu'à ce que les en-têtes de séquence aient accompagné une image.
    ///
    /// Sous rafraîchissement intra progressif (GOP et `idrPeriod` infinis),
    /// l'encodeur n'émet VPS, SPS et PPS qu'avec son tout premier IDR — que ce
    /// spectateur-ci n'a pas reçu s'il arrive après. Sans eux, mesuré à la
    /// tâche 7 : 0 image décodée sur 9 paquets ; avec eux, au moins une.
    entetes_a_joindre: bool,
    /// Vrai depuis une demande d'image clé jusqu'à l'encodage qui la satisfait.
    ///
    /// Ce n'est pas un doublon du drapeau de `NvencEncoder` : celui-ci sait
    /// qu'un IDR est dû, mais l'hôte doit le savoir AUSSI pour ne pas sauter la
    /// capture qui le produirait.
    image_cle_due: bool,
    /// Octets d'unités d'accès réellement acceptés par la piste.
    octets: u64,
    /// Images écrites avec succès, et refus absorbés en les réécrivant.
    images: u64,
    refus_absorbes: u64,
    /// Les deux mêmes, sur la fenêtre de 100 ms du `Pacer`.
    fenetre_images: u64,
    fenetre_refus: u64,
}

impl EnvoiVideo {
    fn nouveau() -> EnvoiVideo {
        EnvoiVideo {
            entetes_a_joindre: true,
            image_cle_due: false,
            octets: 0,
            images: 0,
            refus_absorbes: 0,
            fenetre_images: 0,
            fenetre_refus: 0,
        }
    }

    /// Écrit une unité d'accès ENTIÈRE sur la piste média.
    ///
    /// Le seul refus possible sur ce chemin est `TropDImagesEnAttente`, et il
    /// est RÉCUPÉRABLE : le geste attendu est de poller puis de réécrire la
    /// MÊME image. Mesuré : un `poll` ne libère qu'une place, pas la file —
    /// plusieurs refus d'affilée sont donc normaux quand on a du retard, et ne
    /// sont pas un signe d'échec. Une image déjà encodée DOIT partir : sous GOP
    /// infini elle est chaînée sur la précédente, et l'abandonner casserait la
    /// chaîne pour tout le reste du flux (« ref POC introuvable » en cascade).
    ///
    /// Tout autre refus dit que la piste n'est plus écrivable : rien ne se
    /// rattrape en réessayant, l'appel rend `LienTombe`.
    fn envoyer_image(
        &mut self,
        lien: &mut dyn LienVideo,
        encodeur: &mut dyn Reprise,
        unite: &[u8],
        horodatage_ms: u64,
    ) -> anyhow::Result<IssueEnvoi> {
        // Les en-têtes voyagent DANS la première unité d'accès, et non dans une
        // écriture à part : c'est exactement ce que NVENC produit lui-même devant
        // un IDR forcé (`repeatSPSPPS`), et cela évite de confier au paquetiseur
        // une unité d'accès sans tranche, que rien n'oblige un décodeur à
        // accepter.
        let a_ecrire: Cow<[u8]> = if self.entetes_a_joindre {
            let mut avec_entetes = encodeur.entetes_de_sequence()?;
            avec_entetes.extend_from_slice(unite);
            Cow::Owned(avec_entetes)
        } else {
            Cow::Borrowed(unite)
        };

        let debut = Instant::now();
        let mut refuse = false;
        loop {
            match lien.ecrire_image(&a_ecrire, horodatage_ms) {
                Ok(()) => {
                    // Après l'acceptation, jamais avant : une image refusée est
                    // réécrite telle quelle, en-têtes compris.
                    self.entetes_a_joindre = false;
                    // L'image qui part est celle qu'une éventuelle demande
                    // attendait : la dette d'image clé est éteinte ici, et non à
                    // l'encodage. Un tour plus tard, donc, que le drapeau de
                    // NVENC — mais aucune décision de saut ne se prend entre
                    // l'encodage et cette écriture, et la rabaisser ICI est ce
                    // qui rend le câblage atteignable par un test : la retirer
                    // laisserait le budget désactivé à vie, et chaque image
                    // deviendrait un IDR.
                    self.image_cle_due = false;
                    self.octets += a_ecrire.len() as u64;
                    self.images += 1;
                    self.fenetre_images += 1;
                    if refuse {
                        // Retard réel, résorbé : signal légitime pour le Pacer,
                        // même si l'image est finalement partie.
                        self.refus_absorbes += 1;
                        self.fenetre_refus += 1;
                    }
                    // `ecrire_image` ne fait que confier l'unité au paquetiseur :
                    // les octets ne partent sur le socket que pendant `poll`.
                    // Servir le lien après chaque image est aussi ce qui évite
                    // le refus ci-dessous (mesuré : 0 refus sur 21552 écritures
                    // à 100 Mbps avec ce service, des refus sans lui).
                    if let Some(raison) = self.servir(lien, encodeur)? {
                        return Ok(IssueEnvoi::FluxInterrompu(raison));
                    }
                    return Ok(IssueEnvoi::Envoyee);
                }
                Err(ErreurEnvoi::TropDImagesEnAttente) => {
                    if debut.elapsed() >= BUDGET_RETRY_ENVOI {
                        return Ok(IssueEnvoi::FilePleine);
                    }
                    refuse = true;
                    // Un `poll` par place à libérer, et la même image réécrite.
                    if let Some(raison) = self.servir(lien, encodeur)? {
                        return Ok(IssueEnvoi::FluxInterrompu(raison));
                    }
                    std::thread::sleep(GRANULARITE_SERVICE_RESEAU);
                }
                Err(autre) => return Ok(IssueEnvoi::FluxInterrompu(autre.to_string())),
            }
        }
    }

    /// Un tour de service réseau : `str0m` est sans-IO, rien n'avance sans cet
    /// appel — voir `GRANULARITE_SERVICE_RESEAU` pour pourquoi sa fréquence
    /// compte. Traite au passage ce que le spectateur nous dit.
    ///
    /// Rend `Some(raison)` si le lien est tombé ; l'appelant doit alors arrêter.
    fn servir(
        &mut self,
        lien: &mut dyn LienVideo,
        encodeur: &mut dyn Reprise,
    ) -> anyhow::Result<Option<String>> {
        match lien.poll()? {
            LinkEvent::Failed(raison) => Ok(Some(raison)),
            LinkEvent::Controle(MessageControle::DemandeImageCle) => {
                // Sans délai, et devant tout budget de débit ou saut d'image :
                // sous rafraîchissement intra progressif (GOP et `idrPeriod`
                // infinis), un spectateur sans image clé reçoit des images mais
                // affiche du faux SANS le savoir — le décodeur ne signale rien.
                // Cette demande est sa seule sortie de secours.
                //
                // L'encodeur fait le reste : la prochaine image sort en IDR,
                // précédée de ses en-têtes, et plusieurs demandes rapprochées ne
                // coûtent qu'un IDR.
                encodeur.forcer_image_cle();
                self.image_cle_due = true;
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Faut-il sauter la capture et l'encodage de ce tour ?
    ///
    /// Le budget du `Pacer` le demande dès qu'il est à découvert — sauter une
    /// image entière est sa seule façon de réduire le débit (décision D3).
    ///
    /// **Sauf si une image clé est due.** Le drapeau de NVENC ne retombe qu'à un
    /// encodage réel : sauter la capture ne perd pas l'IDR, elle le REPORTE — et
    /// pendant ce report le spectateur continue d'afficher du faux sans le
    /// savoir, ce que la demande existe justement pour clore. Et le report n'est
    /// pas théorique : l'encodeur est configuré au plafond et jamais reconfiguré
    /// (écart 5), le budget tourne donc autour de zéro dès que la cible atteint
    /// ce plafond — ce qu'elle fait sur le chemin mesuré. Une image clé passe
    /// donc devant le budget : au pire une image de plus que prévu, une fois par
    /// demande.
    fn sauter_ce_tour(&self, budget_octets: f64) -> bool {
        budget_octets < 0.0 && !self.image_cle_due
    }
}

/// Annonce au spectateur que le partage s'arrête, puis sert le lien le temps que
/// l'annonce quitte réellement la machine : `envoyer_controle` ne fait que
/// déposer le message dans le tampon de `str0m`, dont les octets ne partent sur
/// le socket que pendant `poll`. Sans ce drainage, l'annonce mourrait avec le
/// processus — et le spectateur resterait devant une image figée, sans savoir
/// pourquoi.
///
/// Ne dépend d'aucun état d'émission : c'est une fonction libre, appelable sur
/// n'importe quelle sortie (voir `en_annoncant_l_arret`).
fn annoncer_l_arret(lien: &mut dyn LienVideo) -> Result<(), ErreurEnvoi> {
    lien.envoyer_controle(&MessageControle::PartageArrete)?;
    let jusqu_a = Instant::now() + DRAINAGE_ARRET;
    while Instant::now() < jusqu_a {
        // Une demande d'image clé ne change plus rien à ce stade ; en revanche un
        // lien déclaré perdu met fin au drainage sur-le-champ — rien ne partira
        // plus, et c'est ce qui borne le coût d'une annonce tentée sur un lien
        // mourant.
        match lien.poll() {
            Ok(LinkEvent::Failed(_)) | Err(_) => break,
            Ok(_) => std::thread::sleep(GRANULARITE_SERVICE_RESEAU),
        }
    }
    Ok(())
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
    use crate::doublure::{LienFactice, RepriseFactice};
    use std::sync::mpsc;

    /// Trois NAL Annex-B de types 32, 33 et 34 — VPS, SPS, PPS — réduits à leur
    /// en-tête de deux octets. La boucle d'envoi ne lit jamais leur contenu ;
    /// seul leur type est observable, et c'est ce que les tests vérifient.
    fn entetes_factices() -> Vec<u8> {
        let mut octets = Vec::new();
        for type_nal in [32u8, 33, 34] {
            octets.extend_from_slice(&[0, 0, 0, 1, type_nal << 1, 1]);
        }
        octets
    }

    /// Une unité d'accès : un seul NAL de type 1 (tranche non-IDR).
    fn unite_factice() -> Vec<u8> {
        vec![0, 0, 0, 1, 1 << 1, 1, 0xAB, 0xCD]
    }

    /// Les types de NAL d'un flux Annex-B, dans l'ordre. Reconnaît les
    /// délimiteurs de 3 et de 4 octets ; le type est sur 6 bits, décalé de 1
    /// dans le premier octet de l'en-tête HEVC (ITU-T H.265, 7.3.1.2).
    fn types_de_nal(flux: &[u8]) -> Vec<u8> {
        let mut types = Vec::new();
        let mut i = 0;
        while i + 4 <= flux.len() {
            let debut = if flux[i..i + 3] == [0, 0, 1] {
                Some(i + 3)
            } else if i + 4 < flux.len() && flux[i..i + 4] == [0, 0, 0, 1] {
                Some(i + 4)
            } else {
                None
            };
            match debut {
                Some(d) => {
                    types.push((flux[d] >> 1) & 0x3F);
                    i = d + 1;
                }
                None => i += 1,
            }
        }
        types
    }

    #[test]
    fn les_entetes_de_sequence_precedent_la_premiere_image() {
        // Sous rafraîchissement intra progressif, un spectateur qui n'a pas reçu
        // VPS/SPS/PPS ne décode rien : mesuré à la tâche 7, 0 image sur 9
        // paquets sans eux. Ils doivent donc partir AVANT toute image, et pas
        // seulement « un jour ».
        let mut lien = LienFactice::nouveau();
        let mut encodeur = RepriseFactice::avec_entetes(entetes_factices());
        let mut envoi = EnvoiVideo::nouveau();

        envoi
            .envoyer_image(&mut lien, &mut encodeur, &unite_factice(), 0)
            .expect("première image");

        let premiere = lien.ecritures().first().cloned().expect("une écriture");
        let types = types_de_nal(&premiere.unite);
        assert!(
            types.contains(&32) && types.contains(&33) && types.contains(&34),
            "la première écriture doit porter VPS, SPS et PPS : {types:?}"
        );
    }

    #[test]
    fn les_entetes_ne_sont_joints_qu_une_fois() {
        // Les rejoindre à chaque image gonflerait le débit sans rien apporter :
        // le spectateur les a déjà.
        let mut lien = LienFactice::nouveau();
        let mut encodeur = RepriseFactice::avec_entetes(entetes_factices());
        let mut envoi = EnvoiVideo::nouveau();

        for _ in 0..2 {
            envoi
                .envoyer_image(&mut lien, &mut encodeur, &unite_factice(), 0)
                .expect("image");
        }

        let seconde = &lien.ecritures()[1];
        assert_eq!(
            types_de_nal(&seconde.unite),
            vec![1],
            "la seconde écriture ne porte que la tranche"
        );
    }

    #[test]
    fn une_demande_d_image_cle_force_un_idr() {
        // C'est la seule sortie de secours d'un spectateur qui affiche du faux
        // sans le savoir : sous GOP infini, le décodeur ne signale RIEN.
        let mut lien = LienFactice::nouveau();
        let mut encodeur = RepriseFactice::avec_entetes(entetes_factices());
        let mut envoi = EnvoiVideo::nouveau();

        lien.injecter(LinkEvent::Controle(MessageControle::DemandeImageCle));
        let tombe = envoi.servir(&mut lien, &mut encodeur).expect("service");

        assert!(tombe.is_none(), "une demande d'image clé n'est pas une chute du lien");
        assert_eq!(encodeur.images_cle_forcees(), 1);
    }

    #[test]
    fn l_arret_annonce_le_partage_arrete() {
        let mut lien = LienFactice::nouveau();

        annoncer_l_arret(&mut lien).expect("arrêt");

        assert!(
            lien.messages_envoyes().contains(&MessageControle::PartageArrete),
            "le spectateur doit apprendre l'arrêt : {:?}",
            lien.messages_envoyes()
        );
    }

    #[test]
    fn une_erreur_pendant_la_diffusion_annonce_quand_meme_l_arret() {
        // LE test qui manquait : les sorties par `?` de la boucle — périphérique
        // de capture perdu, écran figé, encodage refusé — laissent le lien
        // VIVANT. Sans annonce, le spectateur reste devant une image figée sans
        // explication. Aucune de ces sorties n'annonçait l'arrêt, et rien ne
        // l'avait vu parce que rien ne couvrait la sortie de `diffuser`.
        let mut lien = LienFactice::nouveau();

        let issue = en_annoncant_l_arret(&mut lien, |_| {
            Err(ErreurPartage::Autre(anyhow::anyhow!("écran figé")))
        });

        assert!(matches!(issue, Err(ErreurPartage::Autre(_))), "l'erreur remonte intacte");
        assert!(
            lien.messages_envoyes().contains(&MessageControle::PartageArrete),
            "une erreur de diffusion doit quand même annoncer l'arrêt : {:?}",
            lien.messages_envoyes()
        );
    }

    #[test]
    fn toute_sortie_de_diffusion_tente_d_annoncer_l_arret_sans_exception() {
        // Ce test disait l'inverse à la ronde 1 : « un lien tombé ne l'annonce
        // pas ». L'exception a été retirée, et c'est ce qu'il garantit désormais.
        // `Fin::LienTombe` ne veut pas dire que le lien est mort — elle couvre
        // aussi les refus de la piste média, canal de données vivant. Déduire de
        // cette fin qu'il n'y a plus personne à prévenir laissait le spectateur
        // figé sans explication.
        let mut apres_arret = LienFactice::nouveau();
        let issue = en_annoncant_l_arret(&mut apres_arret, |_| Ok(Fin::Arrete));
        assert_eq!(issue.ok(), Some(Fin::Arrete));
        assert!(apres_arret.messages_envoyes().contains(&MessageControle::PartageArrete));

        let mut apres_interruption = LienFactice::nouveau();
        let _ = en_annoncant_l_arret(&mut apres_interruption, |_| {
            Ok(Fin::LienTombe("la piste vidéo s'est refermée".to_string()))
        });
        assert!(
            apres_interruption.messages_envoyes().contains(&MessageControle::PartageArrete),
            "un flux interrompu doit lui aussi tenter l'annonce : {:?}",
            apres_interruption.messages_envoyes()
        );
    }

    #[test]
    fn le_drainage_de_l_arret_s_arrete_des_que_le_lien_est_declare_perdu() {
        // C'est ce qui borne le coût de l'annonce désormais sans exception : sur
        // un lien mourant, le drainage ne consomme pas ses 50 ms.
        let mut lien = LienFactice::nouveau();
        lien.injecter(LinkEvent::Failed("le lien a été perdu".to_string()));
        let debut = Instant::now();

        annoncer_l_arret(&mut lien).expect("annonce déposée");

        assert!(
            debut.elapsed() < DRAINAGE_ARRET,
            "le drainage devait s'arrêter au premier échec : {:?}",
            debut.elapsed()
        );
        assert_eq!(lien.messages_envoyes(), [MessageControle::PartageArrete]);
    }

    #[test]
    fn une_image_cle_due_passe_devant_le_budget_puis_lui_rend_la_main() {
        // Deux moitiés indissociables, dans un seul test pour qu'elles ne
        // divergent pas.
        //
        // Sauter la capture ne perd pas l'IDR, elle le REPORTE (le drapeau de
        // NVENC ne retombe qu'à un encodage réel) — et pendant ce report le
        // spectateur affiche du faux sans le savoir : le budget ne doit donc pas
        // pouvoir retarder l'image clé.
        //
        // Et la dette doit s'éteindre quand l'image part : sans cela le budget
        // serait désactivé à vie et CHAQUE image deviendrait un IDR, ce qui
        // ferait exploser le débit sans qu'aucun signal ne l'annonce. C'est
        // l'envoi qui l'éteint, précisément pour que ce câblage-là soit
        // atteignable sans GPU.
        let mut lien = LienFactice::nouveau();
        let mut encodeur = RepriseFactice::avec_entetes(entetes_factices());
        let mut envoi = EnvoiVideo::nouveau();
        const A_DECOUVERT: f64 = -1.0;

        assert!(
            envoi.sauter_ce_tour(A_DECOUVERT),
            "sans image clé due, un budget à découvert fait sauter le tour"
        );

        lien.injecter(LinkEvent::Controle(MessageControle::DemandeImageCle));
        envoi.servir(&mut lien, &mut encodeur).expect("service");
        assert_eq!(encodeur.images_cle_forcees(), 1);
        assert!(
            !envoi.sauter_ce_tour(A_DECOUVERT),
            "une image clé due passe devant le budget"
        );

        envoi
            .envoyer_image(&mut lien, &mut encodeur, &unite_factice(), 0)
            .expect("l'image clé part");
        assert!(
            envoi.sauter_ce_tour(A_DECOUVERT),
            "l'image clé envoyée, le budget reprend la main"
        );
    }

    #[test]
    fn une_file_pleine_se_resorbe_en_pollant_et_en_reecrivant_la_meme_image() {
        // Mesuré : un `poll` ne libère qu'UNE place. Deux images de retard
        // demandent donc deux tours, et ces refus ne sont pas un échec — la même
        // image doit finir par partir, intacte et une seule fois.
        let mut lien = LienFactice::nouveau();
        let mut encodeur = RepriseFactice::avec_entetes(entetes_factices());
        let mut envoi = EnvoiVideo::nouveau();
        lien.saturer(2);

        let unite = unite_factice();
        let issue = envoi
            .envoyer_image(&mut lien, &mut encodeur, &unite, 7)
            .expect("envoi");

        assert!(matches!(issue, IssueEnvoi::Envoyee), "l'image doit finir par partir");
        assert_eq!(lien.ecritures().len(), 1, "une seule fois, pas deux");
        assert_eq!(lien.ecritures()[0].horodatage_ms, 7, "le même horodatage qu'au premier essai");
        assert!(lien.polls() >= 2, "un poll par place à libérer : {}", lien.polls());
        assert_eq!(envoi.refus_absorbes, 1, "un retard résorbé, signalé une fois au Pacer");
    }

    #[test]
    fn une_piste_fermee_n_est_pas_retentee() {
        // `PisteFermee` ne se résorbe pas : réessayer 300 ms ne ferait que
        // retarder le diagnostic.
        struct PisteMorte;
        impl LienVideo for PisteMorte {
            fn ecrire_image(&mut self, _: &[u8], _: u64) -> Result<(), ErreurEnvoi> {
                Err(ErreurEnvoi::PisteFermee)
            }
            fn envoyer_controle(&mut self, _: &MessageControle) -> Result<(), ErreurEnvoi> {
                Ok(())
            }
            fn poll(&mut self) -> anyhow::Result<LinkEvent> {
                Ok(LinkEvent::Idle)
            }
        }
        let mut encodeur = RepriseFactice::avec_entetes(entetes_factices());
        let mut envoi = EnvoiVideo::nouveau();
        let debut = Instant::now();

        let issue = envoi
            .envoyer_image(&mut PisteMorte, &mut encodeur, &unite_factice(), 0)
            .expect("envoi");

        assert!(
            matches!(issue, IssueEnvoi::FluxInterrompu(_)),
            "la piste fermée interrompt le flux — sans pour autant dire que le lien est mort"
        );
        assert!(debut.elapsed() < BUDGET_RETRY_ENVOI, "rendu sans consommer le budget de relance");
    }

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
