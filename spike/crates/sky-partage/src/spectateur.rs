//! Côté spectateur : demander le partage d'un ami, intégrer sa réponse, puis
//! recevoir, décoder, AFFICHER et mesurer le flux. Déplacé de
//! `sky-probe/src/cmd_view.rs` (C2). Le flux n'est jamais ENREGISTRÉ par défaut
//! (spec D2) : chaque image est affichée puis lâchée, et seul un appelant qui
//! fournit un puits (`sky-probe view` et son fichier) en garde les octets.

use std::io::Write;
use std::time::{Duration, Instant};

use sky_compte::{deposer, relever, resoudre_ami, Ami, Coffre, Config, ErreurCompte, Etat};
use sky_crypto::Identity;
use sky_decode::{Decodeur, ErreurDecodeur, ImageDecodee};
use sky_net::{ErreurEnvoi, LinkEvent, MessageControle, PeerLink};
use sky_rendu::{EtatVisionnage, EvenementFenetre, Fenetre, ImageAAfficher};

use crate::arret::{synchroniser_sauf_arret, Arret, ErreurAttente, HorlogeArretable};
use crate::etablissement::{etablir, Etablissement};
use crate::evenement::{
    Bilan, BilanReception, ErreurPartage, ErreurVisionnage, Evenement, Fin, Mesures,
    MesuresVisionnage,
};
use crate::reception::Reception;
use crate::rendez_vous::{interroger, reponse_a_l_offre, session_de, ATTENTE_SPECTATEUR, CADENCE};

/// Où écrire le flux reçu, si quelqu'un le veut.
pub type Puits = Box<dyn Write>;

/// Délai minimum entre deux demandes d'image clé.
///
/// Sans cette limitation, un flux inintelligible provoquerait une avalanche de
/// demandes, donc une avalanche d'images clés — chacune bien plus grosse qu'une
/// image ordinaire — qui saturerait la liaison exactement quand elle va déjà
/// mal. Une seconde suffit : l'hôte force l'IDR sans délai (voir
/// `hote::EnvoiVideo::servir`), et plusieurs demandes rapprochées ne lui
/// coûteraient qu'un seul IDR de toute façon.
const DELAI_ENTRE_DEMANDES: Duration = Duration::from_secs(1);

/// Nombre d'unités d'accès refusées de suite par le décodeur au-delà duquel on
/// renonce.
///
/// Une image clé répare un trou dans le flux ; elle ne répare pas un décodeur
/// qui refuse tout — carte débranchée, contexte CUDA perdu, flux qui n'est pas
/// du 4:4:4. Comme `ErreurDecodeur` ne distingue pas ces deux situations, c'est
/// la PERSISTANCE qui les sépare : deux secondes de refus d'affilée à 60 im/s
/// ne sont plus un trou. Sans ce plafond, le spectateur demanderait une image
/// clé par seconde jusqu'à la fin des temps en montrant « En attente de
/// l'image… ».
const ECHECS_DECODAGE_AVANT_ABANDON: u32 = 120;

/// Temps pendant lequel le spectateur attend une image clé, demandes
/// comprises, avant de renoncer avec `ErreurVisionnage::ImageIrreconstituable`.
///
/// **Valeur CHOISIE, pas mesurée.** Dix secondes, c'est dix demandes au rythme
/// de `DELAI_ENTRE_DEMANDES` : assez pour qu'une image clé perdue à son tour, ou
/// deux, soient redemandées ; assez peu pour qu'un spectateur ne contemple pas
/// « En attente de l'image… » sans fin. Ce plafond couvre ce que
/// `ECHECS_DECODAGE_AVANT_ABANDON` ne voit pas : un décodeur qui ne REFUSE rien
/// mais ne rend rien d'affichable — en-têtes de séquence perdus (`Ok(None)` à
/// chaque unité), ou hôte qui n'honore pas les demandes.
const ATTENTE_IMAGE_CLE_MAX: Duration = Duration::from_secs(10);

/// Pause sur `LinkEvent::Idle` dans la boucle de réception.
///
/// `PeerLink::poll` ne bloque jamais (socket non bloquant) : sans pause, la
/// boucle brûlerait un cœur entier, là où le jalon 0 mesurait 0,10 % de CPU pour
/// toute la chaîne. On ne dort QUE sur `Idle` — tant que le lien a quelque chose
/// à rendre, on le vide sans attendre.
///
/// **Sous Windows, une milliseconde demandée dure plutôt ~15 ms** (résolution
/// par défaut de l'ordonnanceur, mesuré à la tâche 5). Une unité d'accès arrivée
/// au début d'une pause peut donc attendre jusqu'à ~15 ms avant d'être décodée.
/// **L'effet sur la latence d'affichage n'est pas mesuré** — à mesurer à l'essai
/// réel à deux machines.
const PAUSE_SUR_INACTIVITE: Duration = Duration::from_millis(1);

/// Ce que le spectateur annonce au décodeur comme taille maximale.
///
/// La vraie résolution vient du rappel de séquence de NVDEC, pas d'ici : le
/// décodeur prend `max(cette annonce, taille codée du flux)` pour son
/// `ulMaxWidth` (voir `sky-decode`), donc un hôte en 4K fonctionne quand même.
/// Ces deux valeurs ne sont qu'un plancher de marge, et ce sont celles du jalon
/// 0 — 2560×1440, la résolution réellement mesurée.
const LARGEUR_ANNONCEE: u32 = 2560;
const HAUTEUR_ANNONCEE: u32 = 1440;

/// Taille d'ouverture de la fenêtre. Elle n'impose rien au flux : le rendu met
/// l'image à l'échelle, et l'utilisateur redimensionne.
const LARGEUR_FENETRE: u32 = 1280;
const HAUTEUR_FENETRE: u32 = 720;

/// Ce que la boucle du spectateur attend du lien pair-à-pair : écouter, et
/// demander une image clé. Elle n'écrit jamais de vidéo — c'est ce qui la
/// distingue de `hote::LienVideo`.
pub trait LienSpectateur {
    fn envoyer_controle(&mut self, message: &MessageControle) -> Result<(), ErreurEnvoi>;
    fn poll(&mut self) -> anyhow::Result<LinkEvent>;
    /// Paquets émis vers une adresse publique : un COMPTE, jamais une adresse.
    /// Seul le bilan de `sky-probe view` le lit, pour dire si la mesure est
    /// celle d'un vrai réseau.
    fn vers_internet(&self) -> u64;
}

impl LienSpectateur for PeerLink {
    fn envoyer_controle(&mut self, message: &MessageControle) -> Result<(), ErreurEnvoi> {
        PeerLink::envoyer_controle(self, message)
    }

    fn poll(&mut self) -> anyhow::Result<LinkEvent> {
        PeerLink::poll(self)
    }

    fn vers_internet(&self) -> u64 {
        self.destinations().1
    }
}

/// Ce que la boucle du spectateur attend de l'écran.
///
/// `sky_rendu::Fenetre` en est la seule implémentation réelle. Le trait vit ici
/// et non dans `sky-rendu` pour une raison simple : il n'y a qu'un appelant, et
/// le trait est local, donc l'implémenter pour un type étranger est licite.
/// Sans lui, aucun test ne serait possible — une `Fenetre` réelle ouvre une
/// fenêtre réelle.
pub trait Afficheur {
    fn afficher(&mut self, image: &dyn ImageAAfficher) -> anyhow::Result<()>;
    fn afficher_etat(&mut self, etat: EtatVisionnage) -> anyhow::Result<()>;
    /// Traite les messages en attente et rend ce que l'utilisateur a fait de la
    /// fenêtre. À appeler à chaque tour : une fenêtre qu'on ne pompe pas est
    /// déclarée « ne répond pas » par Windows.
    fn evenements(&mut self) -> Vec<EvenementFenetre>;
    fn basculer_plein_ecran(&mut self) -> anyhow::Result<()>;
}

impl Afficheur for Fenetre {
    fn afficher(&mut self, image: &dyn ImageAAfficher) -> anyhow::Result<()> {
        Fenetre::afficher(self, image)
    }

    fn afficher_etat(&mut self, etat: EtatVisionnage) -> anyhow::Result<()> {
        Fenetre::afficher_etat(self, etat)
    }

    fn evenements(&mut self) -> Vec<EvenementFenetre> {
        Fenetre::pompe_messages(self)
    }

    fn basculer_plein_ecran(&mut self) -> anyhow::Result<()> {
        Fenetre::basculer_plein_ecran(self)
    }
}

/// Le geste du décodeur dont la boucle a besoin.
///
/// Un type associé plutôt que `ImageDecodee` en dur : `ImageDecodee::nouvelle`
/// est `pub(crate)` dans `sky-decode` et exige une session NVDEC vivante, donc
/// aucune doublure ne saurait en rendre une.
pub trait Decodage {
    type Image: ImageAAfficher;
    fn decoder(
        &mut self,
        unite: &[u8],
        horodatage_ms: u64,
    ) -> Result<Option<Self::Image>, ErreurDecodeur>;
}

impl Decodage for Decodeur {
    type Image = ImageDecodee;

    fn decoder(
        &mut self,
        unite: &[u8],
        horodatage_ms: u64,
    ) -> Result<Option<ImageDecodee>, ErreurDecodeur> {
        Decodeur::decoder(self, unite, horodatage_ms)
    }
}

/// Ce que `Visionnage::traiter` dit à la boucle de faire ensuite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suite {
    Continuer,
    LienPerdu(String),
    /// Le lien s'est refermé APRÈS que l'hôte a annoncé l'arrêt : une fin
    /// normale, qui devient `Fin::PartageArrete`.
    PartageArrete,
}

/// La décision du spectateur : quoi décoder, quoi afficher, quand redemander
/// une image clé.
///
/// # Quand a-t-on le droit d'afficher
///
/// **Jamais avant une image clé, et plus du tout après un trou.** Ce n'est pas
/// une précaution théorique : le flux est en rafraîchissement intra progressif
/// (GOP et `idrPeriod` infinis, période de 2 s), et dans ce régime un
/// spectateur qui n'a pas reçu d'image clé obtient **des images, mais fausses,
/// sans que le décodeur signale la moindre erreur**. Le défaut ne se dénonce
/// pas : il n'y a pas « rien à l'écran », il y a une image plausible et
/// mensongère. Mesuré pendant ce jalon.
///
/// La règle est donc : on n'affiche qu'une image dont toute la chaîne de
/// références est arrivée. Concrètement `cle_vue` s'allume sur une image clé et
/// s'éteint dès qu'un trou est signalé (`sans_perte == false`) ou que le
/// décodeur refuse une unité ; tant qu'il est éteint, l'écran montre
/// `EtatVisionnage::EnAttente` et l'hôte reçoit une demande d'image clé.
///
/// L'arbitrage écarté était de **garder la dernière bonne image à l'écran**
/// pendant la réparation, ce qui serait plus doux à regarder. Deux raisons de
/// ne pas le faire ici : il faudrait retenir l'`ImageDecodee`, or le décodeur
/// n'a que quatre surfaces de sortie et `cuvidDecodePicture` **bloque le fil
/// appelant** quand elles sont épuisées — garder une image d'un tour sur
/// l'autre, c'est risquer de bloquer la réception. Et un gel muet est à son
/// tour un défaut qui ne se dénonce pas, alors que « En attente de l'image… »
/// se lit. Le coût est borné par un aller-retour de demande d'image clé.
pub struct Visionnage<D, A> {
    decodeur: D,
    afficheur: A,
    reception: Reception,
    /// L'origine des horodatages d'arrivée. Une durée, jamais une heure.
    origine: Instant,
    /// Le droit d'afficher. Voir la note de la structure.
    cle_vue: bool,
    /// Ce qui est à l'écran, pour ne pas le repeindre soixante fois par
    /// seconde. `None` quand c'est une image.
    etat_affiche: Option<EtatVisionnage>,
    derniere_demande: Option<Instant>,
    /// Depuis quand on attend une image clé sans rien pouvoir afficher.
    /// Effacé à chaque image affichée : c'est la durée d'UNE attente que borne
    /// `ATTENTE_IMAGE_CLE_MAX`, pas l'âge du visionnage.
    en_attente_depuis: Option<Instant>,
    /// L'hôte a annoncé l'arrêt. Le `Failed` qui suit en est la conséquence,
    /// pas une panne : il ne doit pas repeindre « Connexion perdue » par-dessus.
    partage_arrete: bool,
    echecs_consecutifs: u32,
    images_abandonnees: u32,
    // La fenêtre de mesure, remise à zéro à chaque relevé.
    debut_fenetre: Instant,
    octets_precedent: u64,
    images_precedent: u64,
    latence_us_fenetre: u64,
    images_decodees_fenetre: u64,
}

impl<D: Decodage, A: Afficheur> Visionnage<D, A> {
    pub fn nouveau(decodeur: D, afficheur: A, maintenant: Instant) -> Visionnage<D, A> {
        Visionnage {
            decodeur,
            afficheur,
            reception: Reception::default(),
            origine: maintenant,
            cle_vue: false,
            etat_affiche: None,
            derniere_demande: None,
            en_attente_depuis: None,
            partage_arrete: false,
            echecs_consecutifs: 0,
            images_abandonnees: 0,
            debut_fenetre: maintenant,
            octets_precedent: 0,
            images_precedent: 0,
            latence_us_fenetre: 0,
            images_decodees_fenetre: 0,
        }
    }

    /// Montre l'attente avant la première image. Le canal est ouvert, l'hôte
    /// n'a encore rien envoyé : une fenêtre noire muette serait un défaut.
    pub fn commencer(&mut self) -> anyhow::Result<()> {
        self.montrer(EtatVisionnage::EnAttente)
    }

    /// Sert la fenêtre et dit si l'utilisateur a demandé sa fermeture.
    ///
    /// Le `match` est exhaustif : un événement de fenêtre ajouté demain sans
    /// destinataire ne compile pas. C'est ce qui manquait à F11, produit par
    /// `sky-rendu` depuis la tâche 3 et lu par personne jusqu'à la tâche 10.
    pub fn servir_fenetre(&mut self) -> anyhow::Result<bool> {
        let mut fermeture = false;
        for evenement in self.afficheur.evenements() {
            match evenement {
                EvenementFenetre::FermetureDemandee => fermeture = true,
                EvenementFenetre::PleinEcranBascule => self.afficheur.basculer_plein_ecran()?,
            }
        }
        Ok(fermeture)
    }

    pub fn reception(&mut self) -> &mut Reception {
        &mut self.reception
    }

    pub fn afficheur(&self) -> &A {
        &self.afficheur
    }

    pub fn afficheur_mut(&mut self) -> &mut A {
        &mut self.afficheur
    }

    pub fn decodeur(&self) -> &D {
        &self.decodeur
    }

    pub fn traiter(
        &mut self,
        lien: &mut dyn LienSpectateur,
        evenement: LinkEvent,
        maintenant: Instant,
    ) -> Result<Suite, ErreurPartage> {
        match evenement {
            LinkEvent::Image { donnees, horodatage_ms, cle, sans_perte } => {
                self.sur_image(lien, &donnees, horodatage_ms, cle, sans_perte, maintenant)?;
            }
            LinkEvent::Controle(MessageControle::PartageArrete) => {
                self.partage_arrete = true;
                self.montrer(EtatVisionnage::PartageArrete)?;
            }
            // L'hôte ne nous en demande pas : c'est NOUS qui en envoyons.
            LinkEvent::Controle(MessageControle::DemandeImageCle) => {}
            LinkEvent::Failed(raison) => {
                // Après un arrêt annoncé, l'hôte raccroche : ce `Failed` est la
                // CONSÉQUENCE de l'arrêt, pas une panne. L'écran garde la vraie
                // cause, et la fin est normale — pas un « ÉCHEC ».
                if self.partage_arrete {
                    return Ok(Suite::PartageArrete);
                }
                self.montrer(EtatVisionnage::ConnexionPerdue)?;
                return Ok(Suite::LienPerdu(raison));
            }
            LinkEvent::Connected | LinkEvent::Idle => {}
        }
        // Vérifié APRÈS l'événement : une image clé arrivée pile au terme du
        // délai est affichée, pas refusée. Et sur tout événement, `Idle`
        // compris : le délai court aussi quand l'hôte ne dit plus rien. Après un
        // arrêt annoncé, plus rien n'est attendu — on n'y renonce pas.
        if let Some(depuis) = self.en_attente_depuis {
            if !self.partage_arrete
                && maintenant.saturating_duration_since(depuis) >= ATTENTE_IMAGE_CLE_MAX
            {
                return Err(ErreurPartage::Visionnage(ErreurVisionnage::ImageIrreconstituable));
            }
        }
        Ok(Suite::Continuer)
    }

    fn sur_image(
        &mut self,
        lien: &mut dyn LienSpectateur,
        donnees: &[u8],
        horodatage_ms: u64,
        cle: bool,
        sans_perte: bool,
        maintenant: Instant,
    ) -> Result<(), ErreurPartage> {
        let arrivee_us = maintenant.saturating_duration_since(self.origine).as_micros() as u64;
        self.reception.compter_image(donnees.len(), horodatage_ms * 1_000, arrivee_us);

        // Le trou s'est produit AVANT cette image : il invalide les références
        // accumulées, mais pas une image clé, qui n'en utilise aucune. D'où
        // l'ordre — on perd le droit d'afficher, puis une image clé le rend.
        if !sans_perte {
            self.cle_vue = false;
        }
        if cle {
            self.cle_vue = true;
        }

        // Sans droit d'afficher, on demande une image clé QUEL QUE SOIT le
        // résultat du décodage — et donc AVANT lui. Le cas qui l'exige : des
        // en-têtes de séquence perdus. L'hôte ne les joint qu'une fois ; sans
        // eux NVDEC rend `Ok(None)` à chaque unité (0 image sur 9 paquets,
        // mesuré à la tâche 7), ne refuse rien, et aucun autre chemin ne
        // demanderait quoi que ce soit : « En attente » à vie. Une image clé
        // forcée porte ses propres en-têtes (mesuré à la tâche 7), donc la
        // demander suffit. Même raisonnement après un trou : un trou est un
        // trou, qu'il produise une image ou non.
        if !self.cle_vue {
            self.attendre_une_image_cle(lien, maintenant)?;
        }

        let debut = Instant::now();
        let decodee = match self.decodeur.decoder(donnees, horodatage_ms) {
            Ok(decodee) => decodee,
            Err(erreur) => {
                self.echecs_consecutifs += 1;
                if self.echecs_consecutifs >= ECHECS_DECODAGE_AVANT_ABANDON {
                    // Typée jusqu'à la sortie : `sky-app` donne un message
                    // propre à chacune des cinq variantes du décodeur.
                    return Err(ErreurPartage::Visionnage(ErreurVisionnage::Decodeur(erreur)));
                }
                // Un refus isolé peut venir d'une unité tronquée ; une image clé
                // repart de zéro et la répare.
                self.cle_vue = false;
                return Ok(self.abandonner(lien, maintenant)?);
            }
        };
        // Le plafond compte les refus CONSÉCUTIFS : des refus épars sur une
        // liaison qui marche ne doivent pas s'additionner jusqu'à l'abandon.
        self.echecs_consecutifs = 0;

        // `Ok(None)` n'est pas une erreur : le décodeur a avalé des en-têtes de
        // séquence. Rien à afficher ; s'il manquait quelque chose, la demande est
        // déjà partie plus haut.
        let Some(image) = decodee else {
            return Ok(());
        };
        self.latence_us_fenetre += debut.elapsed().as_micros() as u64;
        self.images_decodees_fenetre += 1;

        if !self.cle_vue {
            // Décodée sans erreur, et pourtant fausse : voir la note de
            // `Visionnage`. C'est ici, et nulle part ailleurs, que le spectateur
            // refuse d'afficher du faux sans le savoir.
            return Ok(self.abandonner(lien, maintenant)?);
        }
        self.afficheur.afficher(&image)?;
        self.etat_affiche = None;
        self.en_attente_depuis = None;
        // `image` tombe ici : une des quatre surfaces de sortie de NVDEC est
        // rendue tout de suite. La retenir d'un tour sur l'autre bloquerait
        // `cuvidDecodePicture` dès qu'elles seraient épuisées.
        Ok(())
    }

    /// Cette image ne sera pas montrée : on le compte, on montre l'attente, et
    /// on demande à l'hôte l'image clé qui remettra le flux d'aplomb.
    fn abandonner(
        &mut self,
        lien: &mut dyn LienSpectateur,
        maintenant: Instant,
    ) -> anyhow::Result<()> {
        self.images_abandonnees = self.images_abandonnees.saturating_add(1);
        self.attendre_une_image_cle(lien, maintenant)
    }

    /// Rien d'affichable : on le dit à l'écran, on démarre le délai d'attente
    /// s'il ne court pas déjà, et on demande une image clé (limitée).
    fn attendre_une_image_cle(
        &mut self,
        lien: &mut dyn LienSpectateur,
        maintenant: Instant,
    ) -> anyhow::Result<()> {
        self.montrer(EtatVisionnage::EnAttente)?;
        self.en_attente_depuis.get_or_insert(maintenant);
        self.demander_image_cle(lien, maintenant)
    }

    /// Demande une image clé à l'hôte, au plus une par `DELAI_ENTRE_DEMANDES`.
    fn demander_image_cle(
        &mut self,
        lien: &mut dyn LienSpectateur,
        maintenant: Instant,
    ) -> anyhow::Result<()> {
        if self
            .derniere_demande
            .is_some_and(|precedente| maintenant.saturating_duration_since(precedente) < DELAI_ENTRE_DEMANDES)
        {
            return Ok(());
        }
        // Un refus d'envoi n'interrompt PAS le visionnage, et c'est délibéré :
        // le seul refus plausible ici est un canal qui se ferme, et dans ce cas
        // le `poll` du tour suivant rend `Failed`, qui donne à l'utilisateur
        // « Connexion perdue » au lieu d'une erreur brute. Rien n'est tu pour
        // autant — l'écran montre déjà l'attente et `images_abandonnees` monte.
        // L'horodatage n'est retenu que si la demande est bien partie, pour que
        // la limitation ne consomme pas le quota d'une demande perdue.
        if lien.envoyer_controle(&MessageControle::DemandeImageCle).is_ok() {
            self.derniere_demande = Some(maintenant);
        }
        Ok(())
    }

    /// Repeint un état, et seulement s'il change.
    fn montrer(&mut self, etat: EtatVisionnage) -> anyhow::Result<()> {
        if self.etat_affiche == Some(etat) {
            return Ok(());
        }
        self.afficheur.afficher_etat(etat)?;
        self.etat_affiche = Some(etat);
        Ok(())
    }

    /// Clôt la fenêtre de mesure et rend ce qu'elle a vu.
    pub fn relever_mesures(&mut self, maintenant: Instant) -> MesuresVisionnage {
        let secondes = maintenant.saturating_duration_since(self.debut_fenetre).as_secs_f64();
        // Un relevé à durée nulle ne se divise pas ; il n'arrive qu'appelé deux
        // fois dans la même microseconde, donc jamais dans la boucle.
        let par_seconde = |quantite: f64| if secondes > 0.0 { quantite / secondes } else { 0.0 };
        let octets = self.reception.octets() - self.octets_precedent;
        let images = self.reception.images() - self.images_precedent;
        let latence_ms = if self.images_decodees_fenetre > 0 {
            self.latence_us_fenetre as f64 / self.images_decodees_fenetre as f64 / 1000.0
        } else {
            0.0
        };

        self.octets_precedent = self.reception.octets();
        self.images_precedent = self.reception.images();
        self.latence_us_fenetre = 0;
        self.images_decodees_fenetre = 0;
        self.debut_fenetre = maintenant;

        MesuresVisionnage {
            images_par_seconde: par_seconde(images as f64) as f32,
            debit_kbps: (par_seconde(octets as f64 * 8.0) / 1000.0) as u32,
            latence_decodage_ms: latence_ms as f32,
            images_abandonnees: self.images_abandonnees,
            gigue_ms: self.reception.gigue_ms() as f32,
        }
    }
}

/// Comment l'appelant désigne l'ami à regarder.
pub enum Designation<'a> {
    /// Nom Discord exact ou identifiant écrit en texte (`sky-probe view`) :
    /// passe par `resoudre_ami`, qui refuse toute ambiguïté.
    Texte(&'a str),
    /// Identifiant d'utilisateur (l'application) : jamais confondu avec un nom.
    Identifiant(i64),
}

pub fn trouver_ami<'e>(etat: &'e Etat, designation: &Designation<'_>) -> Result<&'e Ami, ErreurCompte> {
    match designation {
        Designation::Texte(texte) => resoudre_ami(etat, texte),
        Designation::Identifiant(id) => etat
            .amis
            .iter()
            .find(|ami| ami.id == *id)
            .ok_or_else(|| ErreurCompte::Protocole(format!("aucun ami d'identifiant {id}"))),
    }
}

pub struct ParametresSpectateur<'a> {
    pub ami: Designation<'a>,
    /// `None` : jusqu'à l'arrêt (application). `Some` : `sky-probe --seconds`.
    pub duree_max: Option<Duration>,
    /// D'où se mesurent « réponse reçue après » et « depuis le lancement » :
    /// `sky-probe view` le prend AVANT de lire le coffre, comme au C2 (spec
    /// §8, « exactement comme à l'essai réel »).
    pub lancement: Instant,
}

pub fn regarder(
    config: &Config,
    coffre: &Coffre,
    mut synchroniser: impl FnMut(Option<&Etat>) -> Result<Etat, ErreurCompte>,
    p: ParametresSpectateur<'_>,
    ouvrir_puits: impl FnOnce() -> anyhow::Result<Option<Puits>>,
    arret: &Arret,
    evenements: &mut dyn FnMut(Evenement),
) -> Result<Fin, ErreurPartage> {
    let lancement = p.lancement;
    // Avant tout réseau : sans appareil, `deposer` refuserait de toute façon.
    if coffre.identifiant_appareil().map_err(ErreurPartage::Compte)?.is_none() {
        return Ok(Fin::AucunAppareilLocal);
    }
    // Le décodeur AVANT toute négociation, et c'est voulu (spec §5 : les
    // capacités s'interrogent « avant ») : une machine sans carte NVIDIA, ou dont
    // le décodeur ne prend pas le 4:4:4, l'apprend en une fraction de seconde au
    // lieu de traverser la boîte aux lettres et ICE — et l'hôte ne démarre ni
    // capture ni encodage pour un spectateur qui n'aurait rien pu en faire. Après
    // la vérification d'appareil, qui est locale et donne un message plus utile.
    // Le contexte CUDA tenu pendant l'attente ne coûte rien de mesurable ; il est
    // lié à ce fil, qui est aussi celui de la boucle de réception.
    let decodeur = Decodeur::nouveau(LARGEUR_ANNONCEE, HAUTEUR_ANNONCEE)
        .map_err(|e| ErreurPartage::Visionnage(ErreurVisionnage::Decodeur(e)))?;
    // L'identité DURABLE : c'est pour sa clé d'annuaire que l'hôte scelle
    // l'enveloppe de sa réponse. La clé de l'offre, elle, est éphémère.
    let identite = coffre.identite().map_err(ErreurPartage::Compte)?;

    let etat = synchroniser(None).map_err(ErreurPartage::Compte)?;
    let ami = trouver_ami(&etat, &p.ami).map_err(ErreurPartage::Compte)?.clone();
    if ami.appareils.is_empty() {
        return Ok(Fin::AucunAppareilChezLAmi { nom: ami.discord_name });
    }

    let (mut link, offre) = PeerLink::offrant(Identity::generate())?;
    let session = session_de(&offre)?;
    let deposes = deposer(config, coffre, &ami.appareils, offre.as_bytes()).map_err(ErreurPartage::Compte)?;
    if deposes == 0 {
        return Ok(Fin::DemandeRefusee { nom: ami.discord_name, appareils: ami.appareils.len() });
    }
    evenements(Evenement::DemandeEnvoyee {
        nom: ami.discord_name.clone(),
        deposes,
        appareils: ami.appareils.len(),
        attente: ATTENTE_SPECTATEUR,
    });

    // Le mapping NAT du port annoncé dans l'offre doit survivre à l'attente.
    let garde = link.maintenir_mapping()?;
    let mut synchronisations = 1u32; // celle qui a résolu l'ami
    let mut horloge = HorlogeArretable::demarrer(arret);
    let attente = interroger(
        Some(etat),
        synchroniser_sauf_arret(arret, |precedent| {
            synchronisations += 1;
            synchroniser(precedent)
        }),
        |etat| reponse_a_l_offre(relever(etat, &identite), session, &ami.appareils),
        &mut horloge,
        CADENCE,
        ATTENTE_SPECTATEUR,
    );
    let reponse = match attente {
        Ok(Some(reponse)) => reponse,
        Ok(None) => return Ok(Fin::PasDeReponse { nom: ami.discord_name }),
        Err(ErreurAttente::Arrete) => return Ok(Fin::Arrete),
        Err(ErreurAttente::Compte(e)) => return Err(ErreurPartage::Compte(e)),
    };
    evenements(Evenement::ReponseRecue { apres: lancement.elapsed(), synchronisations });

    // La négociation produit désormais son propre trafic.
    drop(garde);
    link.accepter_reponse(&reponse)?;
    evenements(Evenement::Negociation);

    let duree = match etablir(&mut link, arret)? {
        Etablissement::Ouvert(duree) => duree,
        Etablissement::Rompu(raison) => return Ok(Fin::NegociationRompue(raison)),
        Etablissement::Delai(diagnostic) => return Ok(Fin::EtablissementEchoue(diagnostic)),
        Etablissement::Arrete => return Ok(Fin::Arrete),
    };
    evenements(Evenement::Connecte { en: duree, depuis_le_lancement: Some(lancement.elapsed()) });

    // Ouvert APRÈS la connexion, comme le fichier de `view` au C2 : jamais de
    // fichier vide laissé par une négociation ratée.
    let puits = ouvrir_puits()?;
    recevoir(&mut link, decodeur, &ami.discord_name, puits, p.duree_max, arret, evenements)
}

/// Ouvre la fenêtre, puis passe la main à `boucle`.
///
/// Le décodeur (créé en tête de `regarder`) et la fenêtre sont fabriqués dans ce
/// module plutôt que reçus de l'appelant, pour que ni `ParametresSpectateur` ni
/// la signature de `regarder` ne changent. Cette fonction n'est que du câblage
/// vers des types réels ; tout ce qui se décide est dans `boucle` et
/// `Visionnage`, éprouvés par doublures.
fn recevoir(
    link: &mut PeerLink,
    decodeur: Decodeur,
    nom_de_l_hote: &str,
    puits: Option<Puits>,
    duree_max: Option<Duration>,
    arret: &Arret,
    evenements: &mut dyn FnMut(Evenement),
) -> Result<Fin, ErreurPartage> {
    let fenetre = Fenetre::ouvrir(
        &format!("SkyShare — écran de {nom_de_l_hote}"),
        LARGEUR_FENETRE,
        HAUTEUR_FENETRE,
    )?;
    let visionnage = Visionnage::nouveau(decodeur, fenetre, Instant::now());
    boucle(link, visionnage, puits, duree_max, arret, evenements)
}

/// La boucle de réception : décoder, afficher, mesurer — jusqu'à une fin.
///
/// Elle PREND le visionnage, donc la fenêtre : quelle que soit la sortie, la
/// fenêtre tombe avec elle. « Fermée à l'arrêt » (décision D4) ne dépend ainsi
/// d'aucun geste à ne pas oublier.
///
/// UN SEUL CHEMIN D'ARRÊT. La croix de la fenêtre ne sort pas de la boucle par
/// elle-même : elle LÈVE le même signal que le bouton « Arrêter » de
/// l'interface (`Noyau::arreter`), et c'est la vérification de ce signal, en
/// tête de tour, qui sort. Deux chemins de sortie distincts auraient fini par
/// diverger — l'un des deux laissant, un jour, un fil ou une fenêtre vivants.
fn boucle<D: Decodage, A: Afficheur>(
    lien: &mut dyn LienSpectateur,
    mut visionnage: Visionnage<D, A>,
    mut puits: Option<Puits>,
    duree_max: Option<Duration>,
    arret: &Arret,
    evenements: &mut dyn FnMut(Evenement),
) -> Result<Fin, ErreurPartage> {
    let t0 = Instant::now();
    visionnage.commencer()?;
    let mut dernier_releve = t0;

    let fin = loop {
        // À chaque tour, et AVANT le signal d'arrêt : une fenêtre qu'on ne
        // pompe pas est déclarée « ne répond pas » par Windows, et la croix
        // doit être vue au tour même où elle est cliquée.
        if visionnage.servir_fenetre()? {
            arret.demander();
        }
        if arret.est_demande() {
            break Fin::Arrete;
        }
        if duree_max.is_some_and(|d| t0.elapsed() >= d) {
            break Fin::DureeEcoulee(Box::new(bilan(&*lien, &mut visionnage, t0)));
        }

        let evenement = lien.poll()?;
        let inactif = matches!(evenement, LinkEvent::Idle);
        // Le puits reçoit l'unité d'accès telle qu'elle est arrivée, avant tout
        // décodage : c'est ce que `sky-probe view` écrit dans son fichier.
        if let (LinkEvent::Image { donnees, .. }, Some(p)) = (&evenement, puits.as_mut()) {
            p.write_all(donnees).map_err(anyhow::Error::from)?;
        }
        match visionnage.traiter(lien, evenement, Instant::now())? {
            Suite::Continuer => {}
            Suite::LienPerdu(raison) => break Fin::LienTombe(raison),
            Suite::PartageArrete => break Fin::PartageArrete,
        }

        let maintenant = Instant::now();
        if maintenant.duration_since(dernier_releve) >= Duration::from_secs(1) {
            dernier_releve = maintenant;
            evenements(Evenement::Mesures(Mesures::Reception(
                visionnage.relever_mesures(maintenant),
            )));
        }

        // Sur `Idle` seulement : voir `PAUSE_SUR_INACTIVITE` (~15 ms réels sous
        // Windows, effet sur la latence non mesuré).
        if inactif {
            std::thread::sleep(PAUSE_SUR_INACTIVITE);
        }
    };

    // Même sur un arrêt ou un lien tombé : ce qui est déjà reçu est écrit en
    // entier, et un fichier tronqué en silence serait un faux témoignage.
    if let Some(p) = puits.as_mut() {
        p.flush().map_err(anyhow::Error::from)?;
    }
    Ok(fin)
}

fn bilan<D: Decodage, A: Afficheur>(
    lien: &dyn LienSpectateur,
    visionnage: &mut Visionnage<D, A>,
    t0: Instant,
) -> Bilan {
    let vers_internet = lien.vers_internet();
    let reception = visionnage.reception();
    Bilan::Reception(BilanReception {
        duree_s: t0.elapsed().as_secs_f64(),
        images: reception.images(),
        octets: reception.octets(),
        transit_ms: reception.transit_ms(),
        gigue_ms: reception.gigue_ms(),
        vers_internet,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sky_compte::Ami;

    use crate::doublure::{DecodeurFactice, FenetreFactice, Geste, Issue, LienFactice};

    /// Une unité d'accès de `taille` octets, telle que la piste média la rend.
    fn image_de(taille: usize, cle: bool, sans_perte: bool) -> LinkEvent {
        LinkEvent::Image { donnees: vec![7u8; taille], horodatage_ms: 0, cle, sans_perte }
    }

    fn image(cle: bool, sans_perte: bool) -> LinkEvent {
        image_de(3, cle, sans_perte)
    }

    fn visionnage(
        issues: &[Issue],
        defaut: Issue,
        maintenant: Instant,
    ) -> Visionnage<DecodeurFactice, FenetreFactice> {
        Visionnage::nouveau(
            DecodeurFactice::puis(issues, defaut),
            FenetreFactice::nouvelle(),
            maintenant,
        )
    }

    fn demandes(lien: &LienFactice) -> usize {
        lien.messages_envoyes().iter().filter(|m| **m == MessageControle::DemandeImageCle).count()
    }

    #[test]
    fn une_demande_d_image_cle_est_limitee_a_une_par_seconde() {
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Echec, t0);

        // Dix refus de décodage au MÊME instant : rien ne peut s'être écoulé.
        for _ in 0..10 {
            v.traiter(&mut lien, image(false, true), t0).expect("traitement");
        }

        assert_eq!(demandes(&lien), 1, "dix échecs au même instant ne font qu'une demande");
    }

    #[test]
    fn une_demande_d_image_cle_repart_apres_le_delai() {
        // Le pendant du test précédent : la limitation retient, elle ne bloque
        // pas. Sans lui, remplacer le délai par l'éternité resterait vert.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Echec, t0);

        v.traiter(&mut lien, image(false, true), t0).expect("traitement");
        let juste_avant = t0 + DELAI_ENTRE_DEMANDES - Duration::from_millis(1);
        v.traiter(&mut lien, image(false, true), juste_avant).expect("traitement");
        assert_eq!(demandes(&lien), 1, "à 999 ms, le délai n'est pas écoulé");

        v.traiter(&mut lien, image(false, true), t0 + DELAI_ENTRE_DEMANDES).expect("traitement");
        assert_eq!(demandes(&lien), 2, "le délai écoulé, une nouvelle demande part");
    }

    #[test]
    fn les_mesures_ne_contiennent_aucune_adresse() {
        let mesures = MesuresVisionnage {
            images_par_seconde: 60.0,
            debit_kbps: 12_400,
            latence_decodage_ms: 1.57,
            images_abandonnees: 0,
            gigue_ms: 5.0,
        };
        // La promesse « aucune adresse IP n'est jamais journalisée » doit tenir
        // jusque dans ce qui remonte à l'interface. Ce test rougit si quelqu'un
        // ajoute un champ d'adresse à la structure.
        let rendu = format!("{mesures:?}").to_lowercase();
        for interdit in ["addr", "adresse", "ip", "socket", "peer", "pair"] {
            assert!(!rendu.contains(interdit), "« {interdit} » apparaît dans les mesures : {rendu}");
        }
    }

    #[test]
    fn la_perte_de_connexion_affiche_un_etat_et_pas_une_fenetre_noire() {
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Image, t0);

        let suite = v
            .traiter(&mut lien, LinkEvent::Failed("le lien est tombé".to_string()), t0)
            .expect("traitement");

        assert_eq!(v.afficheur().dernier_etat(), Some(EtatVisionnage::ConnexionPerdue));
        assert_eq!(suite, Suite::LienPerdu("le lien est tombé".to_string()));
    }

    #[test]
    fn le_partage_arrete_affiche_son_etat() {
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Image, t0);

        v.traiter(&mut lien, LinkEvent::Controle(MessageControle::PartageArrete), t0)
            .expect("traitement");

        assert_eq!(v.afficheur().dernier_etat(), Some(EtatVisionnage::PartageArrete));
    }

    #[test]
    fn aucune_image_ne_s_affiche_avant_une_image_cle() {
        // LE test de ce jalon. Sous rafraîchissement intra progressif, une image
        // décodée sans sa chaîne de références est FAUSSE et le décodeur ne dit
        // rien : le décodeur rend ici `Ok(Some(image))`, tout va bien de son
        // point de vue, et c'est pourtant exactement ce qu'il ne faut pas
        // montrer. Neutralisation : retirer la garde `cle_vue` de `sur_image`.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Image, t0);

        v.traiter(&mut lien, image(false, true), t0).expect("traitement");

        assert_eq!(v.afficheur().images_affichees(), 0, "une image non clé ne s'affiche pas");
        assert_eq!(v.afficheur().dernier_etat(), Some(EtatVisionnage::EnAttente));
        assert_eq!(demandes(&lien), 1, "et l'hôte est prévenu qu'il manque une image clé");
    }

    #[test]
    fn une_image_cle_ouvre_l_affichage_et_celles_qui_suivent_passent() {
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Image, t0);

        v.traiter(&mut lien, image(true, true), t0).expect("traitement");
        v.traiter(&mut lien, image(false, true), t0).expect("traitement");

        assert_eq!(
            v.afficheur().journal(),
            [
                Geste::Image { largeur: 1920, hauteur: 1080 },
                Geste::Image { largeur: 1920, hauteur: 1080 },
            ],
            "l'image clé ouvre l'affichage, et aucun état ne s'intercale"
        );
        assert_eq!(demandes(&lien), 0, "rien ne manque : aucune demande");
    }

    #[test]
    fn un_trou_dans_le_flux_referme_l_affichage_et_demande_une_image_cle() {
        // `sans_perte == false` est, sous GOP infini, le SEUL signal qu'un
        // paquet a manqué — le décodeur n'en dira rien.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Image, t0);

        v.traiter(&mut lien, image(true, true), t0).expect("traitement");
        v.traiter(&mut lien, image(false, false), t0).expect("traitement");

        assert_eq!(v.afficheur().images_affichees(), 1, "l'image d'après le trou ne s'affiche pas");
        assert_eq!(v.afficheur().dernier_etat(), Some(EtatVisionnage::EnAttente));
        assert_eq!(demandes(&lien), 1);
    }

    #[test]
    fn une_image_cle_apres_un_trou_s_affiche_quand_meme() {
        // Le trou précède l'image ; si celle-ci est clé, elle ne dépend
        // d'aucune référence perdue. L'écarter ferait attendre pour rien.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Image, t0);

        v.traiter(&mut lien, image(true, false), t0).expect("traitement");

        assert_eq!(v.afficheur().images_affichees(), 1);
        assert_eq!(demandes(&lien), 0, "l'image clé est déjà là : rien à demander");
    }

    #[test]
    fn un_refus_de_decodage_demande_une_image_cle_et_compte_un_abandon() {
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[Issue::Image], Issue::Echec, t0);

        v.traiter(&mut lien, image(true, true), t0).expect("traitement");
        v.traiter(&mut lien, image(false, true), t0).expect("un refus n'est pas une erreur fatale");

        assert_eq!(v.afficheur().dernier_etat(), Some(EtatVisionnage::EnAttente));
        assert_eq!(demandes(&lien), 1);
        assert_eq!(v.relever_mesures(t0 + Duration::from_secs(1)).images_abandonnees, 1);
    }

    #[test]
    fn des_refus_sans_fin_finissent_par_remonter_une_erreur() {
        // Une image clé répare un trou, pas un décodeur qui refuse tout. Sans
        // ce plafond, le spectateur demanderait une image clé par seconde
        // indéfiniment en montrant « En attente de l'image… ».
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Echec, t0);

        for numero in 1..ECHECS_DECODAGE_AVANT_ABANDON {
            v.traiter(&mut lien, image(false, true), t0)
                .unwrap_or_else(|e| panic!("le refus {numero} ne doit pas encore abandonner : {e:?}"));
        }
        let erreur = v
            .traiter(&mut lien, image(false, true), t0)
            .expect_err("au-delà du plafond, le refus remonte");

        // L'erreur TYPÉE du décodeur doit survivre jusqu'à la sortie, DANS SA
        // VARIANTE d'`ErreurPartage` : `sky-app` donne à chacune des cinq
        // variantes un message différent (spec §7), en se branchant sur le type
        // et jamais sur un texte. Neutralisation (tâche 10) : la faire voyager
        // dans `ErreurPartage::Autre(anyhow::Error::new(..))`, comme avant —
        // ce test rougit.
        assert!(
            matches!(
                erreur,
                ErreurPartage::Visionnage(ErreurVisionnage::Decodeur(
                    ErreurDecodeur::QuatreQuatreQuatreNonPris
                ))
            ),
            "la variante du décodeur est préservée : {erreur:?}"
        );
    }

    #[test]
    fn sans_en_tetes_un_flux_qui_ne_rend_rien_demande_une_image_cle() {
        // C1. L'hôte ne joint les en-têtes de séquence qu'UNE fois. S'ils se
        // perdent, NVDEC n'a pas de SPS et rend `Ok(None)` à chaque unité (0 image
        // sur 9 paquets, mesuré à la tâche 7) : aucun refus, donc aucun plafond
        // atteint. Une image clé forcée porte ses propres en-têtes — la demander
        // suffit à réparer. Ici aucun trou n'est signalé : seul le fait de n'avoir
        // jamais vu d'image clé doit déclencher la demande.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Avalee, t0);

        for _ in 0..5 {
            v.traiter(&mut lien, image(false, true), t0).expect("traitement");
        }

        assert_eq!(demandes(&lien), 1, "des unités avalées sans image clé déclenchent UNE demande");
        assert_eq!(v.afficheur().dernier_etat(), Some(EtatVisionnage::EnAttente));
    }

    #[test]
    fn un_trou_demande_une_image_cle_meme_si_rien_n_est_decode() {
        // C1, second volet : un trou est un trou, qu'il produise une image ou
        // non. Sans cela, la dernière image affichée resterait figée à l'écran
        // — un gel muet — et rien ne partirait vers l'hôte.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[Issue::Image], Issue::Avalee, t0);

        v.traiter(&mut lien, image(true, true), t0).expect("traitement");
        v.traiter(&mut lien, image(false, false), t0).expect("traitement");

        assert_eq!(demandes(&lien), 1, "le trou déclenche la demande, même sans image décodée");
        assert_eq!(
            v.afficheur().journal(),
            [Geste::Image { largeur: 1920, hauteur: 1080 }, Geste::Etat(EtatVisionnage::EnAttente)],
            "l'image d'avant le trou ne reste pas figée sans le dire"
        );
    }

    #[test]
    fn apres_un_trou_l_image_cle_suivante_rouvre_l_affichage() {
        // I1 — la propriété centrale de l'arbitrage : la reprise. Clé → image →
        // trou → image → CLÉ → image. Neutralisation : « seule la première image
        // clé ouvre l'affichage » — ce test doit rougir, et lui seul.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Image, t0);

        for (cle, sans_perte) in
            [(true, true), (false, true), (false, false), (false, true), (true, true), (false, true)]
        {
            v.traiter(&mut lien, image(cle, sans_perte), t0).expect("traitement");
        }

        let image_affichee = Geste::Image { largeur: 1920, hauteur: 1080 };
        assert_eq!(
            v.afficheur().journal(),
            [
                image_affichee,
                image_affichee,
                Geste::Etat(EtatVisionnage::EnAttente),
                image_affichee,
                image_affichee,
            ],
            "quatre images sur six, un seul « En attente » entre elles"
        );
        assert_eq!(demandes(&lien), 1);
    }

    #[test]
    fn sans_image_cle_malgre_les_demandes_le_visionnage_renonce() {
        // I2. Le décodeur ne refuse rien — il rend des images, fausses — mais
        // l'image clé demandée n'arrive jamais. Au-delà d'`ATTENTE_IMAGE_CLE_MAX`,
        // on renonce avec la cause de la spec §7. Le dernier événement est un
        // `Idle` : le délai court même quand l'hôte ne dit plus rien.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Image, t0);

        v.traiter(&mut lien, image(false, true), t0).expect("traitement");
        let juste_avant = t0 + ATTENTE_IMAGE_CLE_MAX - Duration::from_millis(1);
        v.traiter(&mut lien, image(false, true), juste_avant).expect("le délai n'est pas écoulé");
        let erreur = v
            .traiter(&mut lien, LinkEvent::Idle, t0 + ATTENTE_IMAGE_CLE_MAX)
            .expect_err("le délai écoulé, le visionnage renonce");

        assert!(
            matches!(erreur, ErreurPartage::Visionnage(ErreurVisionnage::ImageIrreconstituable)),
            "la cause est typée : {erreur:?}"
        );
        assert!(demandes(&lien) >= 2, "plusieurs demandes sont parties avant de renoncer");
    }

    #[test]
    fn une_image_cle_arrivee_a_temps_remet_le_delai_d_attente_a_zero() {
        // Le pendant d'I2 : le délai mesure UNE attente, pas l'âge du
        // visionnage. Neutralisation : ne pas l'effacer à l'affichage — le
        // dernier `traiter` rend `Err`. La première unité est avalée (en-têtes
        // perdus) plutôt que décodée puis jetée : ce test éprouve le délai, et
        // ne doit pas dépendre de la reprise, qu'éprouve
        // `apres_un_trou_l_image_cle_suivante_rouvre_l_affichage`.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[Issue::Avalee], Issue::Image, t0);

        v.traiter(&mut lien, image(false, true), t0).expect("traitement");
        let a_temps = t0 + ATTENTE_IMAGE_CLE_MAX - Duration::from_millis(1);
        v.traiter(&mut lien, image(true, true), a_temps).expect("l'image clé arrive à temps");
        v.traiter(&mut lien, LinkEvent::Idle, t0 + ATTENTE_IMAGE_CLE_MAX * 2)
            .expect("on affiche : aucune attente en cours");

        assert_eq!(v.afficheur().images_affichees(), 1);
    }

    #[test]
    fn la_fin_du_lien_apres_un_arret_annonce_ne_masque_pas_l_arret() {
        // I3. L'hôte annonce l'arrêt, puis raccroche : le `Failed` qui suit est
        // la CONSÉQUENCE de l'arrêt, pas une panne. Le repeindre en « Connexion
        // perdue » montrerait une fausse cause. Neutralisation : repeindre
        // quand même — `dernier_etat` vaut `ConnexionPerdue`.
        //
        // Et la boucle sort en fin NORMALE (tâche 10, D3) : avant, elle sortait
        // en `LienPerdu`, et `sky-probe view` imprimait « ÉCHEC ».
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Image, t0);

        v.traiter(&mut lien, LinkEvent::Controle(MessageControle::PartageArrete), t0)
            .expect("traitement");
        let suite = v
            .traiter(&mut lien, LinkEvent::Failed("le lien est tombé".to_string()), t0)
            .expect("traitement");

        assert_eq!(v.afficheur().dernier_etat(), Some(EtatVisionnage::PartageArrete));
        assert_eq!(suite, Suite::PartageArrete, "un arrêt annoncé n'est pas un lien perdu");
    }

    #[test]
    fn un_canal_de_controle_ferme_n_interrompt_pas_le_visionnage() {
        // Course réelle : le spectateur détecte un trou à l'instant où l'hôte
        // raccroche. Faire remonter ce refus d'envoi donnerait à l'utilisateur
        // une erreur brute là où le `Failed` du tour suivant lui dit
        // « Connexion perdue ». Neutralisation : remettre le `?` sur
        // `envoyer_controle` — le premier `traiter` rend `Err`.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        lien.refuser_les_controles();
        let mut v = visionnage(&[], Issue::Echec, t0);

        v.traiter(&mut lien, image(false, true), t0).expect("un canal fermé n'est pas fatal");
        assert_eq!(v.afficheur().dernier_etat(), Some(EtatVisionnage::EnAttente));

        // Et la demande perdue n'a pas consommé le quota : dès que le canal
        // revient, elle repart sans attendre une seconde.
        let mut lien = LienFactice::nouveau();
        v.traiter(&mut lien, image(false, true), t0).expect("traitement");
        assert_eq!(demandes(&lien), 1, "une demande perdue ne compte pas comme envoyée");
    }

    #[test]
    fn une_unite_decodee_remet_le_compteur_de_refus_a_zero() {
        // Trouvé par une neutralisation INOPÉRANTE : retirer la remise à zéro
        // laissait les 51 tests verts. Sans elle, des refus ÉPARS — un paquet
        // perdu de temps en temps sur une liaison qui marche — s'additionnent
        // jusqu'au plafond, et le visionnage s'arrête sur un flux parfaitement
        // sain. Le plafond doit compter les refus CONSÉCUTIFS, pas leur total.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut issues = Vec::new();
        for _ in 0..ECHECS_DECODAGE_AVANT_ABANDON + 10 {
            issues.push(Issue::Echec);
            issues.push(Issue::Image);
        }
        let mut v = visionnage(&issues, Issue::Image, t0);

        for numero in 0..issues.len() {
            v.traiter(&mut lien, image(false, true), t0)
                .unwrap_or_else(|e| panic!("l'unité {numero} ne doit pas faire abandonner : {e:?}"));
        }
    }

    #[test]
    fn les_en_tetes_de_sequence_ne_sont_pas_une_erreur() {
        // `Ok(None)` : le décodeur a avalé des VPS/SPS/PPS. Rien à afficher,
        // rien à réparer, rien à demander.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Avalee, t0);

        let suite = v.traiter(&mut lien, image(true, true), t0).expect("traitement");

        assert_eq!(suite, Suite::Continuer);
        assert_eq!(v.afficheur().images_affichees(), 0);
        assert_eq!(demandes(&lien), 0, "des en-têtes avalés ne sont pas un flux illisible");
        assert_eq!(v.relever_mesures(t0 + Duration::from_secs(1)).images_abandonnees, 0);
    }

    #[test]
    fn l_unite_poussee_dans_le_decodeur_est_celle_recue_sans_en_tete() {
        // Le découpage maison préfixait neuf octets à chaque morceau, et c'est
        // `Reception::absorber` qui les retirait. Il n'y a plus ni l'un ni
        // l'autre : l'unité d'accès va au décodeur telle quelle. Neutralisation :
        // sauter un octet à l'entrée de `sur_image` — les octets diffèrent.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Image, t0);

        v.traiter(&mut lien, image_de(5, true, true), t0).expect("traitement");

        assert_eq!(v.decodeur().unites(), [vec![7u8; 5]]);
    }

    #[test]
    fn la_fermeture_de_la_fenetre_remonte_a_la_boucle() {
        // La croix de la fenêtre doit arrêter le visionnage ; la boucle
        // n'interroge que `Visionnage::servir_fenetre`. Neutralisation :
        // rendre `false` en dur — le test rougit.
        let t0 = Instant::now();
        let mut v = visionnage(&[], Issue::Image, t0);

        assert!(!v.servir_fenetre().expect("service"), "rien n'a été demandé");
        v.afficheur_mut().demander_la_fermeture();
        assert!(v.servir_fenetre().expect("service"), "la fermeture demandée remonte");
    }

    #[test]
    fn f11_bascule_le_plein_ecran_et_ne_ferme_rien() {
        // D6 (tâche 10) : `PleinEcranBascule` était PRODUIT par `sky-rendu`
        // depuis la tâche 3 et lu par personne — la serrure posée mais jamais
        // branchée. Neutralisation : ignorer `PleinEcranBascule` dans
        // `servir_fenetre` — aucune bascule, le test rougit.
        let t0 = Instant::now();
        let mut v = visionnage(&[], Issue::Image, t0);

        v.afficheur_mut().appuyer_sur_f11();
        let fermeture = v.servir_fenetre().expect("service");

        assert_eq!(v.afficheur().bascules(), 1, "F11 bascule le plein écran");
        assert!(!fermeture, "F11 n'est pas une demande de fermeture");
    }

    /// Une boucle complète sur doublures, bornée : un défaut qui la ferait
    /// tourner sans fin rend le test rouge au lieu de le figer.
    fn boucler(
        lien: &mut LienFactice,
        v: Visionnage<DecodeurFactice, FenetreFactice>,
        arret: &Arret,
    ) -> Result<Fin, ErreurPartage> {
        boucle(lien, v, None, Some(Duration::from_secs(5)), arret, &mut |_| {})
    }

    #[test]
    fn la_croix_de_la_fenetre_leve_le_meme_arret_que_le_bouton() {
        // D8 : UN SEUL CHEMIN D'ARRÊT. La croix ne sort pas de la boucle par
        // elle-même : elle lève le signal que lève aussi le bouton
        // « Arrêter » de l'interface (`Noyau::arreter`). Neutralisation :
        // `break Fin::Arrete` directement sur la croix, sans lever le signal —
        // la fin est la même, mais `arret` reste muet et ce test rougit.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Image, t0);
        v.afficheur_mut().demander_la_fermeture();
        let fermee = v.afficheur().temoin_de_fermeture();
        let arret = Arret::nouveau();

        let fin = boucler(&mut lien, v, &arret);

        assert_eq!(fin.ok(), Some(Fin::Arrete));
        assert!(arret.est_demande(), "la croix lève le signal du bouton, pas un second chemin");
        assert!(fermee.get(), "la fenêtre est détruite à la sortie");
        assert_eq!(lien.polls(), 0, "aucun tour de réception après la croix");
    }

    #[test]
    fn le_bouton_arreter_sort_de_la_boucle_et_ferme_la_fenetre() {
        // Le chemin du bouton : le signal levé de l'extérieur. La boucle PREND
        // le visionnage, donc la fenêtre tombe avec elle — c'est le témoin qui
        // le dit, pas une supposition.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let v = visionnage(&[], Issue::Image, t0);
        let fermee = v.afficheur().temoin_de_fermeture();
        let arret = Arret::nouveau();
        arret.demander();

        let fin = boucler(&mut lien, v, &arret);

        assert_eq!(fin.ok(), Some(Fin::Arrete));
        assert!(fermee.get(), "la fenêtre est détruite à l'arrêt");
    }

    #[test]
    fn un_arret_annonce_par_l_hote_finit_en_partage_arrete() {
        // D3 (tâche 10), au niveau de la boucle : l'hôte annonce l'arrêt puis
        // raccroche. Avant, la boucle sortait en `Fin::LienTombe` et
        // `sky-probe view` imprimait « ÉCHEC » pour un arrêt normal.
        // Neutralisations mesurées : (1) traduire `Suite::PartageArrete` en
        // `Fin::LienTombe` dans `boucle` — ce test rougit seul ; (2) faire
        // rendre `Suite::LienPerdu` au `Failed` qui suit l'annonce — il rougit
        // avec `la_fin_du_lien_apres_un_arret_annonce_ne_masque_pas_l_arret`,
        // qui éprouve la même décision un étage plus bas.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        lien.injecter(LinkEvent::Controle(MessageControle::PartageArrete));
        lien.injecter(LinkEvent::Failed("le lien est tombé".to_string()));
        let v = visionnage(&[], Issue::Image, t0);

        let fin = boucler(&mut lien, v, &Arret::nouveau());

        assert_eq!(fin.ok(), Some(Fin::PartageArrete));
    }

    #[test]
    fn un_lien_qui_tombe_sans_annonce_reste_un_echec() {
        // Le pendant du précédent : sans lui, « toute fin de lien est un arrêt
        // normal » passerait. Une panne doit rester une panne.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        lien.injecter(LinkEvent::Failed("le lien est tombé".to_string()));
        let v = visionnage(&[], Issue::Image, t0);

        let fin = boucler(&mut lien, v, &Arret::nouveau());

        assert_eq!(fin.ok(), Some(Fin::LienTombe("le lien est tombé".to_string())));
    }

    #[test]
    fn les_mesures_remontent_le_debit_et_les_images_recues() {
        // Ces calculs existaient depuis le C2 et ne remontaient plus à personne.
        // Neutralisation : rendre `debit_kbps: 0` — le test rougit.
        let t0 = Instant::now();
        let mut lien = LienFactice::nouveau();
        let mut v = visionnage(&[], Issue::Image, t0);

        // 125 000 octets = 1 000 000 bits ; sur une seconde, 1 000 kbps.
        v.traiter(&mut lien, image_de(125_000, true, true), t0).expect("traitement");
        let mesures = v.relever_mesures(t0 + Duration::from_secs(1));

        assert_eq!(mesures.debit_kbps, 1_000);
        assert!((mesures.images_par_seconde - 1.0).abs() < 1e-6, "{}", mesures.images_par_seconde);
    }

    fn ami(id: i64, nom: &str) -> Ami {
        Ami { id, friendship_id: id, discord_name: nom.to_string(), appareils: Vec::new() }
    }

    #[test]
    fn un_identifiant_ne_se_confond_jamais_avec_un_nom() {
        // L'application désigne l'ami par son identifiant. `resoudre_ami`
        // refuserait ici l'ambiguïté (un ami NOMMÉ « 7 », un autre
        // D'IDENTIFIANT 7). Neutralisation : passer `Identifiant` par
        // `resoudre_ami(etat, &id.to_string())` — erreur, le test rougit.
        let etat = Etat {
            version: 1,
            code: "ABCDEFGH".to_string(),
            amis: vec![ami(3, "7"), ami(7, "bob")],
            demandes: Vec::new(),
            listes: Vec::new(),
            appareils: Vec::new(),
            enveloppes: Vec::new(),
        };
        assert_eq!(trouver_ami(&etat, &Designation::Identifiant(7)).unwrap().discord_name, "bob");
        assert!(trouver_ami(&etat, &Designation::Texte("7")).is_err());
        assert!(trouver_ami(&etat, &Designation::Identifiant(99)).is_err());
    }
}
