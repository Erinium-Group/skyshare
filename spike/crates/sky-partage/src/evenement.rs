//! Ce que `heberger` et `regarder` font savoir à qui les appelle. Aucune
//! phrase ici : les textes vivent chez l'appelant (`sky-probe` : terminal ;
//! `sky-app` : interface). Aucun champ ne porte d'adresse.

use std::time::Duration;

use sky_compte::ErreurCompte;
use sky_decode::ErreurDecodeur;
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
    /// Le relevé du spectateur, tel que `Visionnage::relever_mesures` le
    /// produit : une seule source de vérité de la boucle jusqu'à l'interface.
    Reception(MesuresVisionnage),
}

/// Ce que le visionnage remonte une fois par seconde. Aucun champ ne porte
/// d'adresse, et c'est vérifié par un test : cette structure traverse
/// `sky-app` jusqu'à l'interface, où la promesse « aucune adresse IP n'est
/// jamais journalisée » doit encore tenir.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MesuresVisionnage {
    /// Images ARRIVÉES depuis le relevé précédent, ramenées à la seconde.
    pub images_par_seconde: f32,
    pub debit_kbps: u32,
    pub latence_decodage_ms: f32,
    /// Cumulé depuis le début : les images décodées puis jetées faute d'être
    /// dignes de confiance, et celles que le décodeur a refusées.
    pub images_abandonnees: u32,
    pub gigue_ms: f32,
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
    /// Toujours 0 côté hôte depuis que la vidéo est passée sur la piste média :
    /// le spectateur ne renvoie plus l'horodatage, donc plus aucun retour n'est
    /// reçu. `rtt_ms` est vide pour la même raison.
    pub retours: u64,
    /// Images que la piste a d'abord refusées (`TropDImagesEnAttente`) et qui
    /// sont parties après un `poll` : du retard rattrapé, pas des envois perdus.
    pub refus_absorbes: u64,
    pub images_ecrites: u64,
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
    /// Hôte : la **file de paquetisation** de la piste média est restée pleine
    /// plus de `BUDGET_RETRY_ENVOI` — « la connexion était trop lente pour la
    /// vidéo » (écart 7). Le correspondant ne dépile plus rien.
    ///
    /// S'appelait `TamponSature { morceau, morceaux }` jusqu'à la tâche 10 du
    /// jalon 2 : le découpage maison et son tampon d'émission ont disparu à la
    /// tâche 8, une image est une unité indivisible, et il n'y a plus rien à
    /// compter.
    FileDePaquetisationPleine,
    /// Le flux s'est interrompu pendant la diffusion ou le visionnage, pour une
    /// raison déjà rédigée, sans adresse.
    ///
    /// LE NOM DIT MOINS QUE LA VARIANTE. Elle couvre bien un lien tombé
    /// (`LinkEvent::Failed`), mais aussi, côté hôte, une piste média qui ne
    /// prend plus d'image (`PisteFermee`, `CodecNonNegocie`, `EcritureRefusee`)
    /// alors que le canal de données peut être parfaitement vivant. Ne pas en
    /// déduire qu'il n'y a plus personne à qui parler : `hote.rs` a déjà payé
    /// cette erreur une fois (`en_annoncant_l_arret`).
    LienTombe(String),
    /// Spectateur : l'hôte a annoncé l'arrêt de son partage
    /// (`MessageControle::PartageArrete`), puis le lien s'est refermé. C'est
    /// une fin NORMALE, pas un échec : avant cette variante, elle sortait en
    /// `LienTombe`, et `sky-probe view` imprimait « ÉCHEC » pour un arrêt
    /// ordinaire.
    PartageArrete,
}

/// Pourquoi le visionnage s'arrête en erreur, typé pour que l'appelant puisse
/// donner à chaque cause son message honnête (spec §7).
///
/// Elle voyage dans sa propre variante, [`ErreurPartage::Visionnage`]. Jusqu'à
/// la tâche 10 elle passait par `ErreurPartage::Autre` et se retrouvait par
/// `downcast_ref` — un chemin qu'un changement de type à l'insertion aurait
/// cassé en silence, la compilation passant quand même.
#[derive(Debug)]
pub enum ErreurVisionnage {
    /// Le décodeur refuse : à l'ouverture (pas de carte, pas de 4:4:4,
    /// résolution trop grande…) ou `ECHECS_DECODAGE_AVANT_ABANDON` fois de
    /// suite pendant le flux. Les cinq variantes sont intactes.
    Decodeur(ErreurDecodeur),
    /// Aucune image clé en `ATTENTE_IMAGE_CLE_MAX` malgré les demandes.
    ImageIrreconstituable,
}

impl std::fmt::Display for ErreurVisionnage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurVisionnage::Decodeur(erreur) => erreur.fmt(f),
            // Le texte de la spec §7, mot pour mot : c'est ce que lit
            // l'utilisateur de `sky-probe view`. L'application, elle, ne lit
            // jamais ce texte — elle se branche sur la variante (`fin_vue`).
            ErreurVisionnage::ImageIrreconstituable => f.write_str(
                "L'image ne peut pas être reconstituée. Demandez à la personne qui partage de \
                 relancer son partage.",
            ),
        }
    }
}

impl std::error::Error for ErreurVisionnage {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            // Transparente, comme `#[error(transparent)]` : l'affichage reprend
            // déjà celui du décodeur, la chaîne ne doit pas le répéter.
            ErreurVisionnage::Decodeur(erreur) => erreur.source(),
            ErreurVisionnage::ImageIrreconstituable => None,
        }
    }
}

/// Les fins ANORMALES : tout ce qui, dans `sky-probe`, remontait par `?`.
#[derive(Debug)]
pub enum ErreurPartage {
    Compte(ErreurCompte),
    /// Le spectateur n'a pas pu décoder ou reconstituer l'image. Typée jusqu'à
    /// l'appelant : `sky-app` donne à chaque cause son message, sans jamais lire
    /// un texte d'erreur (celui d'`AucuneCarteNvidia` vient de `libloading` et
    /// peut être LOCALISÉ selon la langue de Windows).
    Visionnage(ErreurVisionnage),
    Autre(anyhow::Error),
}

impl From<anyhow::Error> for ErreurPartage {
    fn from(e: anyhow::Error) -> ErreurPartage {
        ErreurPartage::Autre(e)
    }
}

impl From<ErreurVisionnage> for ErreurPartage {
    fn from(e: ErreurVisionnage) -> ErreurPartage {
        ErreurPartage::Visionnage(e)
    }
}
