//! Ce que `heberger` et `regarder` font savoir à qui les appelle. Aucune
//! phrase ici : les textes vivent chez l'appelant (`sky-probe` : terminal ;
//! `sky-app` : interface). Aucun champ ne porte d'adresse.

use std::time::Duration;

use sky_compte::ErreurCompte;
use sky_encode::Codec;

#[derive(Debug, Clone, PartialEq)]
pub enum Evenement {
    /// Hôte : paramètres validés, avant tout accès au coffre ou au réseau.
    Pret,
    /// Hôte : disponible, une demande sera honorée pendant `fenetre`.
    Disponible { fenetre: Duration },
    /// Hôte : une offre recevable refusée par `PeerLink::repondant` (contenu du bloc).
    DemandeEcartee { raison: String },
    /// Hôte : `repondant` a échoué pour une cause LOCALE ; la demande est perdue.
    EchecLocal { raison: String },
    /// Hôte : une demande d'ami retenue.
    DemandeRecue { expediteur_device_id: i64, apres: Duration, synchronisations: u32 },
    /// Spectateur : offre déposée pour `deposes` des `appareils` de l'ami.
    DemandeEnvoyee { nom: String, deposes: usize, appareils: usize, attente: Duration },
    /// Spectateur : réponse de l'ami relevée.
    ReponseRecue { apres: Duration, synchronisations: u32 },
    /// Négociation en cours (hôte : réponse déposée ; spectateur : réponse acceptée).
    Negociation,
    /// Canal de données ouvert.
    Connecte { en: Duration, depuis_le_lancement: Option<Duration> },
    /// Hôte : la diffusion commence.
    Diffusion { largeur: u32, hauteur: u32, codec: Codec, plancher_mbps: u32, plafond_mbps: u32 },
    /// Une fois par seconde pendant le flux.
    Mesures(Mesures),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mesures {
    Envoi { debit_mbps: f64, cible_mbps: f64, images_sautees: u64, rtt_ms: f64 },
    /// `images_par_s` : images arrivées depuis la mesure précédente (~1 s).
    Reception { debit_mbps: f64, images_par_s: u64, gigue_ms: f64 },
}

/// Médiane et 99e centile, en millisecondes.
#[derive(Debug, Clone, PartialEq)]
pub struct Quantiles {
    pub p50: f64,
    pub p99: f64,
    pub echantillons: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BilanEnvoi {
    pub duree_s: f64,
    pub images_encodees: u64,
    pub images_sautees: u64,
    pub encodage_ms: Option<Quantiles>,
    pub envoyes_octets: u64,
    pub cible_finale_bps: u32,
    pub retours: u64,
    pub echecs_envoi: u64,
    pub tentatives_envoi: u64,
    pub rtt_ms: Option<Quantiles>,
    pub vers_internet: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BilanReception {
    pub duree_s: f64,
    pub images: u64,
    pub octets: u64,
    pub transit_ms: Option<Quantiles>,
    pub gigue_ms: f64,
    pub vers_internet: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Bilan {
    Envoi(BilanEnvoi),
    Reception(BilanReception),
}

/// Ce que `etablir` a OBSERVÉ quand le canal ne s'est pas ouvert à temps —
/// jamais une cause déduite (leçon du 23/08 : un diagnostic n'énonce que ce
/// qu'il a mesuré).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// ICE a trouvé un chemin : c'est la poignée de main chiffrée qui a échoué.
    pub ice_connecte: bool,
    pub emis: u64,
    pub recus: u64,
    pub erreurs: u64,
    pub vers_local: u64,
    pub vers_internet: u64,
    pub erreurs_socket: u64,
    pub delai: Duration,
}

/// Les fins NORMALES d'un partage — tout ce qui, dans `sky-probe`, se
/// terminait par un message puis `return Ok(())`.
#[derive(Debug, Clone, PartialEq)]
pub enum Fin {
    Arrete,
    /// `duree_max` atteinte (`sky-probe --seconds`). Jamais dans l'application.
    DureeEcoulee(Box<Bilan>),
    AucunAppareilLocal,
    /// Hôte : `FENETRE_HOTE` écoulée sans demande.
    AucuneDemande,
    /// Hôte : le site a refusé la réponse.
    ReponseRefusee,
    AucunAppareilChezLAmi { nom: String },
    /// Spectateur : le site a refusé l'offre pour tous les appareils de l'ami.
    DemandeRefusee { nom: String, appareils: usize },
    /// Spectateur : `ATTENTE_SPECTATEUR` écoulée sans réponse.
    PasDeReponse { nom: String },
    /// `etablir` : le lien a signalé un échec.
    NegociationRompue(String),
    /// `etablir` : canal non ouvert en `DELAI_ETABLISSEMENT`.
    EtablissementEchoue(Diagnostic),
    /// Hôte : tampon d'émission plein plus de `BUDGET_RETRY_ENVOI` —
    /// « la connexion était trop lente pour la vidéo » (écart 7).
    TamponSature { morceau: usize, morceaux: usize },
    /// Le lien est tombé pendant le flux.
    LienTombe(String),
}

/// Les fins ANORMALES : tout ce qui, dans `sky-probe`, remontait par `?`.
#[derive(Debug)]
pub enum ErreurPartage {
    Compte(ErreurCompte),
    Autre(anyhow::Error),
}

impl From<anyhow::Error> for ErreurPartage {
    fn from(e: anyhow::Error) -> ErreurPartage {
        ErreurPartage::Autre(e)
    }
}
