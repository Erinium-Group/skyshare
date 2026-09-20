//! Le cœur de l'application : le SEUL endroit qui tient l'état de la machine
//! (spec §3). Aucune dépendance à Tauri : la fenêtre, l'icône et les
//! événements passent par `Coquille`, ce qui rend chaque commande testable
//! sans fenêtre, contre le serveur double.
//!
//! ORDRE DES VERROUS : `synchro` puis `donnees`, jamais l'inverse. `donnees`
//! n'est JAMAIS tenu pendant un appel réseau, un accès au trousseau ni un
//! appel à la coquille.
//!
//! RESYNCHRONISATION COMPLÈTE APRÈS CHAQUE COMMANDE : le site calcule la
//! version comme le MAX des `updated_at` des lignes qui RESTENT (`etat.ts`).
//! Retirer un ami ou supprimer une liste supprime une ligne ; régénérer le
//! code écrit une colonne hors du calcul. La version peut ne pas bouger, et
//! `?version=` rendrait `inchange` en gardant l'ancien état à l'écran.
//! `resynchro_complete` porte ce besoin ; la connexion la pose déjà, et la
//! tâche qui apportera les commandes d'annuaire (8) y branchera son
//! `apres_commande`.
//!
//! ARBITRAGE DU CONTRÔLEUR (tâche 7) : ce module n'écrit AUCUNE méthode sans
//! appelant. Le brief en posait plusieurs dont le premier appelant n'arrive
//! qu'aux tâches 8, 10 ou 11 (`apres_commande`, `exiger_connexion`,
//! `modifier_partage`, `definir_ecrans`, les accesseurs `config` et
//! `reveil`) : elles appartiennent à la tâche qui les branche.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use sky_compte::{Coffre, Config, ErreurCompte, Etat, Jetons};

use crate::cadence::{cadence, Phase, PLANCHER_ENTRE_SYNCHROS};
use crate::coquille::Coquille;
use crate::materiel::nom_d_appareil;
use crate::reveil::{Horloge, Reveil, Sommeil};
use crate::vue::{
    AmiVue, AppareilVue, Connexion, DemandeVue, EcranVue, Instantane, ListeVue, PartageVue,
};

pub type Connecteur = Box<dyn Fn(&Config, &Coffre) -> Result<Jetons, ErreurCompte> + Send + Sync>;

pub struct Branchements {
    pub coquille: Box<dyn Coquille>,
    /// `sky_compte::connecter` en production (ouvre le navigateur, attend au
    /// plus 5 minutes) ; une fermeture sans navigateur dans les tests.
    pub connecter: Connecteur,
    /// `COMPUTERNAME` en production.
    pub nom_machine: Option<String>,
    /// `HorlogeReelle` en production ; une horloge que le test avance à la main.
    pub horloge: Box<dyn Horloge>,
}

pub const MESSAGE_PENDANT_PARTAGE: &str =
    "Impossible pendant un partage ou une attente : arrête-le d'abord.";
pub const MESSAGE_SESSION_EXPIREE: &str = "Session expirée — reconnecte-toi";
/// Ronde de correction 2 : une déconnexion demandée pendant l'authentification
/// l'emporte sur elle. Un message, pas un `Ok` silencieux — la connexion
/// demandée n'a pas eu lieu — et pas une alarme : l'utilisateur a eu ce qu'il
/// a demandé.
pub const MESSAGE_CONNEXION_ANNULEE: &str =
    "Connexion annulée : tu t'es déconnecté pendant l'authentification.";
/// Ronde de correction 3 : cette connexion-ci a été dépassée par une AUTRE
/// connexion, qui détient désormais le coffre. Rien à oublier, rien à alarmer.
pub const MESSAGE_CONNEXION_REMPLACEE: &str =
    "Connexion annulée : une autre connexion l'a remplacée.";
/// Ce que rend `synchroniser` quand la session a changé pendant sa requête.
pub const MESSAGE_SESSION_CHANGEE: &str =
    "session changée pendant la synchronisation : l'état reçu a été abandonné";

/// Longueur maximale, en caractères, du détail d'erreur recopié à l'écran.
/// Assez pour un message de validation du site, trop peu pour un corps de
/// réponse entier.
const LONGUEUR_MAX_DETAIL: usize = 200;

/// Ramène un détail d'erreur à ce qu'on peut afficher sans risque : tout ce
/// qui suit `Bearer ` est coupé, et le reste est tronqué.
///
/// Découpe en `chars` et non en octets : trancher un `&str` sur une frontière
/// arbitraire paniquerait dès qu'un accent tombe au mauvais endroit, et les
/// messages du site sont en français.
fn detail_borne(detail: &str) -> String {
    let avant_jeton = match detail.find("Bearer ") {
        Some(debut) => &detail[..debut],
        None => detail,
    };
    let mut borne: String = avant_jeton.chars().take(LONGUEUR_MAX_DETAIL).collect();
    if borne.chars().count() < avant_jeton.chars().count() || avant_jeton.len() < detail.len() {
        borne.push_str(" […]");
    }
    borne
}

/// Le message montré pour une erreur de compte.
///
/// RONDE DE CORRECTION 1 (Mineur 2). Le commentaire précédent affirmait
/// qu'aucune variante d'`ErreurCompte` ne porte de jeton. C'est vrai des seuls
/// chemins qui passent par `ErreurCompte::depuis_statut`, qui filtre le corps
/// (`corps_sans_en_tete`, `erreur.rs`) — UN site sur huit. `annuaire.rs` (cinq
/// occurrences), `listes.rs` et `boite.rs` construisent
/// `Protocole(format!("statut {statut} inattendu : {corps}"))` SANS ce filtre,
/// et ce sont précisément les fonctions que la tâche 8 branchera à l'interface.
/// La garantie vient donc de `depuis_statut`, pas du type : le détail est borné
/// ICI, au dernier point avant l'écran.
pub fn message_erreur(e: &ErreurCompte) -> String {
    match e {
        ErreurCompte::Refuse => MESSAGE_SESSION_EXPIREE.to_string(),
        ErreurCompte::Reseau(_) => {
            "Le site SkyShare ne répond pas — vérifie ta connexion internet.".to_string()
        }
        // Le `Display` d'`ErreurCompte::Protocole` préfixe « réponse inattendue
        // du serveur », faux pour une entrée refusée AVANT le réseau (listes,
        // nom d'appareil) : seul le détail est montré.
        ErreurCompte::Protocole(detail) => detail_borne(detail),
        ErreurCompte::Coffre(detail) => {
            format!("Gestionnaire d'identifiants de Windows : {}", detail_borne(detail))
        }
    }
}

/// Spec §6 : un `login` révoque puis réenregistre l'appareil (C2). Pendant un
/// partage ou une attente, les enveloppes adressées à l'ancien seraient
/// perdues : l'application ne se (dé)connecte jamais à ce moment-là.
pub fn permis_hors_partage(phase: Phase) -> Result<(), String> {
    if phase == Phase::Inactive {
        Ok(())
    } else {
        Err(MESSAGE_PENDANT_PARTAGE.to_string())
    }
}

struct Donnees {
    connexion: Connexion,
    nom: Option<String>,
    /// Dernier état reçu, enveloppes vidées : le précédent du tour suivant.
    etat: Option<Etat>,
    appareil_courant: Option<i64>,
    /// Le prochain tour synchronisera SANS précédent (voir l'en-tête).
    resynchro_complete: bool,
    /// Génération de la session affichée — incrémentée par CHAQUE changement
    /// de session (connexion, déconnexion). Voir `synchroniser`.
    generation: u64,
    /// Début de la dernière synchronisation lancée, pour le plancher de
    /// `tour` (`PLANCHER_ENTRE_SYNCHROS`). `None` : aucune encore.
    derniere_synchro: Option<Instant>,
    visible: bool,
    partage: PartageVue,
    nvenc: bool,
    ecrans: Vec<EcranVue>,
    demarrage_automatique: bool,
}

pub struct Noyau {
    config: Config,
    coffre: Coffre,
    branchements: Branchements,
    donnees: Mutex<Donnees>,
    /// Sérialise les synchronisations : une seule à la fois, qu'elle vienne
    /// de la boucle, d'une commande ou d'un partage. Il ne couvre PAS
    /// `connexion`/`deconnexion` — voir la garde de génération dans
    /// `synchroniser`, et la raison de ce choix.
    synchro: Mutex<()>,
    reveil: Reveil,
}

impl Noyau {
    pub fn nouveau(config: Config, coffre: Coffre, branchements: Branchements) -> Noyau {
        let connecte = matches!(coffre.jetons(), Ok(Some(_)));
        Noyau {
            config,
            coffre,
            branchements,
            donnees: Mutex::new(Donnees {
                connexion: if connecte { Connexion::Connecte } else { Connexion::Deconnecte },
                nom: None,
                etat: None,
                appareil_courant: None,
                resynchro_complete: true,
                generation: 0,
                derniere_synchro: None,
                visible: true,
                partage: PartageVue::Inactif,
                nvenc: false,
                ecrans: Vec::new(),
                demarrage_automatique: false,
            }),
            synchro: Mutex::new(()),
            reveil: Reveil::default(),
        }
    }

    pub fn coffre(&self) -> &Coffre {
        &self.coffre
    }

    fn donnees(&self) -> MutexGuard<'_, Donnees> {
        self.donnees.lock().expect("verrou des données empoisonné")
    }

    pub fn instantane(&self) -> Instantane {
        let d = self.donnees();
        let etat = d.etat.as_ref();
        Instantane {
            connexion: d.connexion,
            nom: d.nom.clone(),
            code: etat.map(|e| e.code.clone()),
            amis: etat
                .map(|e| {
                    e.amis
                        .iter()
                        .map(|a| AmiVue {
                            id: a.id,
                            friendship_id: a.friendship_id,
                            nom: a.discord_name.clone(),
                            appareils: a.appareils.len(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            demandes: etat
                .map(|e| {
                    e.demandes
                        .iter()
                        .map(|dm| DemandeVue {
                            friendship_id: dm.friendship_id,
                            nom: dm.discord_name.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            listes: etat
                .map(|e| {
                    e.listes
                        .iter()
                        .map(|l| ListeVue {
                            id: l.id,
                            nom: l.nom.clone(),
                            couleur: l.couleur.clone(),
                            emoji: l.emoji.clone(),
                            membres: l.membres.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            appareils: etat
                .map(|e| {
                    e.appareils
                        .iter()
                        .map(|a| AppareilVue {
                            id: a.id,
                            nom: a.nom.clone(),
                            courant: d.appareil_courant == Some(a.id),
                            revoque: a.revoked_at.is_some(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            partage: d.partage.clone(),
            nvenc: d.nvenc,
            ecrans: d.ecrans.clone(),
            demarrage_automatique: d.demarrage_automatique,
        }
    }

    pub(crate) fn publier(&self) {
        let instantane = self.instantane();
        self.branchements.coquille.publier_etat(&instantane);
    }

    pub fn phase(&self) -> Phase {
        Phase::de(&self.donnees().partage)
    }

    pub fn definir_visible(&self, visible: bool) {
        self.donnees().visible = visible;
        self.reveil.sonner();
    }

    pub fn definir_demarrage_automatique_connu(&self, actif: bool) {
        self.donnees().demarrage_automatique = actif;
        self.publier();
    }

    #[cfg(test)]
    pub(crate) fn forcer_partage(&self, partage: PartageVue) {
        self.donnees().partage = partage;
    }

    /// La SEULE synchronisation de l'application : la boucle, les commandes
    /// et le partage passent tous par elle. Transmet l'état précédent (ou
    /// aucun, après une commande), garde le nouvel état enveloppes vidées,
    /// publie, et rend l'état complet — enveloppes comprises — à l'appelant :
    /// pendant une attente, c'est le partage.
    pub fn synchroniser(&self) -> Result<Etat, ErreurCompte> {
        let _une_a_la_fois = self.synchro.lock().expect("verrou de synchronisation empoisonné");
        let (precedent, generation) = {
            let mut d = self.donnees();
            d.derniere_synchro = Some(self.branchements.horloge.maintenant());
            let precedent = if d.resynchro_complete { None } else { d.etat.clone() };
            (precedent, d.generation)
        };
        let issue = sky_compte::synchroniser(&self.config, &self.coffre, precedent.as_ref());
        let appareil_courant = self.coffre.identifiant_appareil().ok().flatten();
        {
            let mut d = self.donnees();
            // RONDE DE CORRECTION 1 (Important 1) : une réponse d'une session
            // qui n'est plus la session courante n'écrit RIEN. `synchro` ne
            // protège pas ce chemin — `donnees` est délibérément relâché
            // pendant l'appel réseau (jusqu'à 5 s), et prendre `synchro` dans
            // `connexion` y bloquerait la boucle le temps d'une connexion
            // Discord (5 minutes). La génération est la seule garde qui tienne
            // sans contredire l'ordre des verrous : sans elle, une déconnexion
            // acceptée pendant la requête était annulée par la branche `Ok`,
            // qui réaffichait code ami, amis et listes d'un compte dont les
            // jetons n'existaient plus.
            if d.generation != generation {
                drop(d);
                self.publier();
                // RONDE DE CORRECTION 2 (Mineur) : ne rien ÉCRIRE ne suffit
                // pas — rendre `Ok(etat)` remettrait à l'appelant un état
                // ENVELOPPES COMPRISES, d'une session qui n'existe plus. Sans
                // effet aujourd'hui (`boucle` jette la valeur), mais à la
                // tâche 11 le partage consommerait les enveloppes d'un autre
                // compte, que le serveur a déjà effacées en les livrant : elles
                // seraient perdues pour leur vrai destinataire. Une erreur
                // explicite, jamais un état vide qui passerait pour une
                // réponse normale.
                return Err(ErreurCompte::Protocole(MESSAGE_SESSION_CHANGEE.to_string()));
            }
            match &issue {
                Ok(etat) => {
                    d.etat = Some(Etat { enveloppes: Vec::new(), ..etat.clone() });
                    d.resynchro_complete = false;
                    d.connexion = Connexion::Connecte;
                    d.appareil_courant = appareil_courant;
                }
                Err(ErreurCompte::Refuse) if d.connexion == Connexion::Connecte => {
                    d.connexion = Connexion::SessionExpiree;
                }
                Err(_) => {}
            }
        }
        self.publier();
        issue
    }

    /// Traduit une erreur de compte en message, et passe en « session
    /// expirée » sur un refus.
    pub(crate) fn apres_erreur(&self, e: &ErreurCompte) -> String {
        if matches!(e, ErreurCompte::Refuse) {
            self.donnees().connexion = Connexion::SessionExpiree;
            self.publier();
        }
        message_erreur(e)
    }

    /// Au lancement : si une session existe, s'assure qu'un appareil est
    /// enregistré (spec §4), lit le nom Discord et synchronise.
    pub fn demarrer(&self) {
        if self.donnees().connexion != Connexion::Connecte {
            self.publier();
            return;
        }
        let nom = sky_compte::moi(&self.config, &self.coffre).ok().map(|m| m.discord_name);
        self.donnees().nom = nom;
        if let Err(e) = self.assurer_appareil(false) {
            self.apres_erreur(&e);
        }
        let _ = self.synchroniser();
    }

    /// Aucun appareil dans le coffre : l'enregistrer sous le nom de la
    /// machine. Un appareil, juste après une connexion : le rattacher à la
    /// nouvelle session (C2, revue finale I1 — sans quoi plus aucune
    /// enveloppe ne lui serait livrée).
    fn assurer_appareil(&self, apres_connexion: bool) -> Result<(), ErreurCompte> {
        match self.coffre.identifiant_appareil()? {
            Some(_) if apres_connexion => {
                sky_compte::rattacher_appareil(&self.config, &self.coffre).map(|_| ())
            }
            Some(_) => Ok(()),
            None => {
                let nom = nom_d_appareil(self.branchements.nom_machine.as_deref());
                let cle = self.coffre.identite()?.public_key();
                sky_compte::enregistrer_appareil(&self.config, &self.coffre, &nom, &cle).map(|_| ())
            }
        }
    }

    /// La session a-t-elle changé depuis le début de cette connexion ?
    fn session_changee(&self, generation: u64) -> bool {
        self.donnees().generation != generation
    }

    /// Cette connexion a été dépassée : elle n'écrit rien, et n'oublie les
    /// jetons que si c'est une DÉCONNEXION qui l'a dépassée.
    ///
    /// RONDE DE CORRECTION 2. Ne rien afficher ne suffit pas : `connecter`
    /// range les jetons frais dans le coffre AVANT de rendre la main. Les
    /// laisser ferait croire au prochain lancement qu'il est connecté sous la
    /// session que l'utilisateur vient de refuser — pire que le défaut
    /// d'origine, qui ne réaffichait que des données.
    ///
    /// RONDE DE CORRECTION 3 (Important 2). Oublier sur TOUT changement de
    /// génération effaçait les jetons de quelqu'un d'autre. Rien n'interdit une
    /// seconde connexion pendant qu'une première est `EnCours` — chaque
    /// commande part sur son propre fil, et `permis_hors_partage` ne regarde
    /// que la phase de partage. Enchaînement : « Se connecter » (A, navigateur
    /// ouvert) → « Se déconnecter » → « Se connecter » (B, rapide, jetons
    /// rangés, écran « Connecté ») → l'onglet de A aboutit → A effaçait les
    /// jetons VALIDES de B, laissant l'écran « Connecté » sur un coffre vide.
    ///
    /// La décision se lit donc sur l'état affiché, sous le verrou :
    /// - `Deconnecte` — la dernière volonté exprimée est « déconnecté », et les
    ///   jetons du coffre sont les orphelins de CETTE connexion : les oublier.
    /// - `Connecte` — une autre connexion a abouti, ses jetons sont valides :
    ///   ne jamais y toucher.
    /// - `EnCours` — une autre connexion est en route ; sa propre queue réglera
    ///   le coffre, qu'elle écrasera de toute façon : ne rien faire.
    ///
    /// Un simple compteur de déconnexions NE SUFFIRAIT PAS : dans
    /// l'enchaînement ci-dessus il a bel et bien été incrémenté, et A effacerait
    /// encore les jetons de B. C'est « qui détient le coffre maintenant » qu'il
    /// faut lire, pas « une déconnexion a-t-elle eu lieu ».
    ///
    /// CE QUI REND CETTE LECTURE SÛRE : `Connecte` ne peut avoir été écrit que
    /// par QUELQU'UN D'AUTRE, puisque notre propre écriture est gardée par la
    /// génération. Retirer cette garde casse aussi celle-ci — mesuré : sans
    /// elle, la connexion abandonnée écrit `Connecte`, se prend pour une autre
    /// session, et n'oublie plus rien. Les deux tiennent ensemble.
    fn abandonner_connexion(&self) -> Result<(), String> {
        if self.donnees().connexion != Connexion::Deconnecte {
            // Dépassée par une AUTRE connexion : le coffre ne nous appartient
            // plus.
            self.publier();
            return Err(MESSAGE_CONNEXION_REMPLACEE.to_string());
        }
        let oubli = self.coffre.oublier();
        self.publier();
        match oubli {
            Ok(()) => Err(MESSAGE_CONNEXION_ANNULEE.to_string()),
            // RONDE DE CORRECTION 3 (Mineur 3) : dire le fait DANGEREUX, pas
            // seulement la cause technique. Des jetons restés dans le trousseau
            // feront croire au prochain lancement qu'il est connecté.
            Err(e) => Err(format!(
                "Connexion annulée, mais les jetons n'ont pas pu être retirés du trousseau \
                 ({}) : le prochain lancement se croira connecté. Refais la déconnexion.",
                message_erreur(&e)
            )),
        }
    }

    /// RONDE DE CORRECTION 2 (Important) : `deconnexion` n'est bloquée que par
    /// `permis_hors_partage`, donc elle est autorisée pendant les cinq minutes
    /// que peut durer `connecter` (navigateur Discord). La génération capturée
    /// ici est donc relue avant toute écriture. Refuser `deconnexion` pendant
    /// `EnCours` aurait été plus simple mais piégerait l'utilisateur cinq
    /// minutes s'il ferme l'onglet Discord : c'est la déconnexion qui gagne,
    /// jamais l'inverse.
    ///
    /// RONDE DE CORRECTION 3 : il y avait TROIS relectures ; il en reste DEUX.
    /// La première, placée juste après `connecter`, s'est révélée
    /// indétectable — la retirer seule laissait les 13 tests du noyau verts,
    /// parce que la relecture sous le verrou d'écriture (ci-dessous) rattrape
    /// exactement les mêmes cas. C'est la classe de défaut que `CLAUDE.md`
    /// disqualifie : deux gardes redondantes répondent l'une pour l'autre, et
    /// aucune n'est prouvée. Elle n'achetait qu'un aller-retour `moi()`
    /// économisé dans une course rare — pas une propriété. Elle a donc été
    /// SUPPRIMÉE plutôt que garnie d'un test taillé sur mesure.
    ///
    /// Les deux qui restent couvrent des fenêtres disjointes, et chacune a un
    /// test qui rougit quand on la retire SEULE :
    /// - sous le verrou qui écrit (après `moi()`, jusqu'à 5 s) — sans elle, la
    ///   connexion écrit `Connecte` puis part enregistrer un appareil sur un
    ///   compte que l'utilisateur vient de quitter ;
    /// - après `assurer_appareil` et `synchroniser` — sans elle, les jetons
    ///   frais restent dans le coffre.
    pub fn connexion(&self) -> Result<(), String> {
        permis_hors_partage(self.phase())?;
        let generation = {
            // La session change : toute synchronisation déjà partie sur le
            // réseau appartient à la précédente et ne doit rien réécrire.
            let mut d = self.donnees();
            d.connexion = Connexion::EnCours;
            d.generation += 1;
            d.generation
        };
        self.publier();
        if let Err(e) = (self.branchements.connecter)(&self.config, &self.coffre) {
            let mut d = self.donnees();
            // Même garde sur l'échec : une déconnexion survenue pendant la
            // tentative a déjà posé `Deconnecte`, ne pas la réécrire.
            if d.generation == generation {
                d.connexion = Connexion::Deconnecte;
                drop(d);
                self.publier();
            }
            return Err(message_erreur(&e));
        }
        let nom = sky_compte::moi(&self.config, &self.coffre).ok().map(|m| m.discord_name);
        {
            let mut d = self.donnees();
            // PREMIER POINT DE CONTRÔLE, relu SOUS le verrou qui va écrire —
            // jamais avant, sinon la fenêtre entre la lecture et l'écriture
            // reste ouverte (TOCTOU). `connecter` puis `moi` viennent de durer
            // jusqu'à cinq minutes et cinq secondes.
            if d.generation != generation {
                drop(d);
                return self.abandonner_connexion();
            }
            d.nom = nom;
            d.connexion = Connexion::Connecte;
            d.resynchro_complete = true;
        }
        let appareil = self.assurer_appareil(true);
        let synchronisation = self.synchroniser();
        // SECOND POINT DE CONTRÔLE : `assurer_appareil` et `synchroniser` sont
        // deux appels réseau de plus, pendant lesquels une déconnexion reste
        // acceptée. Sans lui, les jetons frais resteraient dans le coffre.
        if self.session_changee(generation) {
            return self.abandonner_connexion();
        }
        appareil.map_err(|e| {
            format!(
                "Connecté, mais cet appareil n'a pas pu être rattaché ({}) : il ne recevra aucune \
                 demande de partage. Reconnecte-toi.",
                message_erreur(&e)
            )
        })?;
        synchronisation.map(|_| ()).map_err(|e| message_erreur(&e))
    }

    pub fn deconnexion(&self) -> Result<(), String> {
        permis_hors_partage(self.phase())?;
        self.coffre.oublier().map_err(|e| message_erreur(&e))?;
        {
            let mut d = self.donnees();
            d.connexion = Connexion::Deconnecte;
            d.nom = None;
            d.etat = None;
            d.appareil_courant = None;
            d.resynchro_complete = true;
            // Même raison qu'en connexion : une réponse en vol appartient à la
            // session qu'on vient d'oublier.
            d.generation += 1;
        }
        self.publier();
        Ok(())
    }

    /// Un tour de boucle : synchronise (sauf pendant une attente, et sauf si le
    /// plancher n'est pas franchi), rend la durée du prochain sommeil.
    pub fn tour(&self) -> Duration {
        let (connecte, phase, restant) = {
            let d = self.donnees();
            let restant = d.derniere_synchro.and_then(|debut| {
                PLANCHER_ENTRE_SYNCHROS
                    .checked_sub(self.branchements.horloge.maintenant().duration_since(debut))
            });
            (d.connexion == Connexion::Connecte, Phase::de(&d.partage), restant)
        };
        // Pendant une attente, c'est le partage qui synchronise : la boucle lui
        // volerait les enveloppes, que le serveur efface en les livrant.
        if connecte && phase != Phase::Attente {
            // Plancher (Mineur 4) : trop tôt, on rend le temps qui reste comme
            // durée de sommeil plutôt que la cadence — la synchronisation part
            // dès qu'il est franchi, pas au prochain battement.
            if let Some(restant) = restant.filter(|r| !r.is_zero()) {
                return restant;
            }
            let _ = self.synchroniser();
        }
        let d = self.donnees();
        cadence(d.visible, Phase::de(&d.partage))
    }

    /// La boucle de synchronisation unique (spec §3). `tours` : `None` en
    /// production, un nombre dans les tests.
    pub fn boucle(&self, sommeil: &mut dyn Sommeil, tours: Option<usize>) {
        let mut n = 0usize;
        loop {
            let duree = self.tour();
            n += 1;
            if tours.is_some_and(|t| n >= t) {
                return;
            }
            sommeil.dormir(duree, &self.reveil);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cadence::{CADENCE_PARTAGE, CADENCE_REDUITE, CADENCE_VISIBLE};
    use crate::essais::{
        contexte, contexte_bloquant, contexte_connexion_bloquante, contexte_connexion_en_pause,
        HorlogeDeTest,
    };
    use std::sync::Arc;

    struct SommeilEspion {
        noyau: Arc<Noyau>,
        /// Dormir fait AVANCER l'horloge injectée d'autant : sans cela, la
        /// boucle se heurterait au plancher `PLANCHER_ENTRE_SYNCHROS` dès son
        /// second tour, alors qu'elle a bel et bien laissé passer sa cadence.
        horloge: Arc<HorlogeDeTest>,
        durees: Vec<Duration>,
    }

    impl Sommeil for SommeilEspion {
        fn dormir(&mut self, duree: Duration, _reveil: &Reveil) {
            self.durees.push(duree);
            self.horloge.avancer(duree);
            if self.durees.len() == 1 {
                self.noyau.definir_visible(false);
            }
        }
    }

    #[test]
    fn la_boucle_transmet_l_etat_precedent_et_passe_a_5_minutes_fenetre_reduite() {
        // Neutralisations : (1) passer toujours `None` à `synchroniser` —
        // `syncs_recues` vaut [None, None, None] ; (2) ignorer `visible` dans
        // `tour` — la seconde durée reste 30 s.
        let c = contexte("sky-test-app-boucle", true);
        c.serveur.etat_mut().version = 4;
        let mut sommeil = SommeilEspion {
            noyau: Arc::clone(&c.noyau),
            horloge: Arc::clone(&c.horloge),
            durees: Vec::new(),
        };
        c.noyau.boucle(&mut sommeil, Some(3));
        assert_eq!(c.serveur.etat_mut().syncs_recues, vec![None, Some(4), Some(4)]);
        assert_eq!(sommeil.durees, vec![CADENCE_VISIBLE, CADENCE_REDUITE]);
    }

    #[test]
    fn pendant_une_attente_la_boucle_ne_synchronise_pas_et_bat_toutes_les_2_s() {
        // Sinon la boucle consommerait l'offre d'un ami, que le serveur efface
        // en la livrant. Neutralisation : retirer `phase != Phase::Attente` de
        // `tour` — une synchronisation apparaît.
        let c = contexte("sky-test-app-attente", true);
        c.noyau.forcer_partage(PartageVue::Disponible { debut_ms: 0, fenetre_s: 1800, ecran: 0 });
        let mut sommeil = SommeilEspion {
            noyau: Arc::clone(&c.noyau),
            horloge: Arc::clone(&c.horloge),
            durees: Vec::new(),
        };
        c.noyau.boucle(&mut sommeil, Some(2));
        assert!(c.serveur.etat_mut().syncs_recues.is_empty());
        assert_eq!(sommeil.durees, vec![CADENCE_PARTAGE]);

        // CONTRÔLE POSITIF (ronde de correction 1, Mineur 3) : l'assertion
        // ci-dessus est négative et passerait aussi si le double était
        // injoignable, ou si la boucle ne synchronisait jamais. Hors attente,
        // le MÊME noyau et le MÊME double doivent produire une
        // synchronisation — sans quoi l'absence ne prouve rien.
        c.noyau.forcer_partage(PartageVue::Inactif);
        c.horloge.avancer(PLANCHER_ENTRE_SYNCHROS);
        c.noyau.tour();
        assert_eq!(
            c.serveur.etat_mut().syncs_recues.len(),
            1,
            "hors attente, la boucle synchronise bien : l'absence ci-dessus vient de l'attente"
        );
    }

    #[test]
    fn une_rafale_de_reveils_ne_produit_qu_une_synchronisation_par_plancher() {
        // Mineur 4 : chaque réveil (alt-tab, fermeture, réduction) coupe le
        // sommeil et fait refaire un tour. Sans plancher, une rafale produit
        // autant de requêtes, hors du budget que les cadences bornent.
        // Neutralisation : retirer le `return restant` de `tour` — deux
        // synchronisations au lieu d'une, et la durée rendue devient 30 s.
        let c = contexte("sky-test-app-plancher", true);

        assert_eq!(c.noyau.tour(), CADENCE_VISIBLE);
        assert_eq!(c.serveur.etat_mut().syncs_recues.len(), 1);

        // Alt-tab 200 ms plus tard : le réveil a coupé le sommeil.
        c.horloge.avancer(Duration::from_millis(200));
        let restant = c.noyau.tour();
        assert_eq!(
            c.serveur.etat_mut().syncs_recues.len(),
            1,
            "le plancher a retenu la seconde synchronisation"
        );
        assert_eq!(
            restant,
            PLANCHER_ENTRE_SYNCHROS - Duration::from_millis(200),
            "le tour rend le temps restant, pas la cadence : la synchronisation reportée \
             part dès que le plancher est franchi"
        );

        // Plancher franchi : elle part.
        c.horloge.avancer(restant);
        assert_eq!(c.noyau.tour(), CADENCE_VISIBLE);
        assert_eq!(c.serveur.etat_mut().syncs_recues.len(), 2);
    }

    #[test]
    fn au_premier_lancement_l_appareil_s_enregistre_sous_le_nom_de_la_machine() {
        // Spec §4. Neutralisation : rendre `Ok(())` dans la branche `None` de
        // `assurer_appareil` — aucun appareil n'est enregistré.
        let c = contexte("sky-test-app-premier-lancement", true);
        c.noyau.demarrer();
        let appareils = c.serveur.etat_mut().appareils.clone();
        assert_eq!(appareils.len(), 1);
        assert_eq!(appareils[0].nom, "MACHINE-DE-TEST");
        let vue = c.noyau.instantane();
        assert!(vue.appareils[0].courant, "l'appareil enregistré est celui de cette machine");
        assert_eq!(vue.connexion, Connexion::Connecte);
    }

    #[test]
    fn la_connexion_enregistre_l_appareil_et_synchronise() {
        let c = contexte("sky-test-app-connexion", false);
        assert_eq!(c.noyau.instantane().connexion, Connexion::Deconnecte);
        c.noyau.connexion().unwrap();
        assert_eq!(*c.connexions.lock().unwrap(), 1);
        assert_eq!(c.noyau.instantane().connexion, Connexion::Connecte);
        assert_eq!(c.serveur.etat_mut().appareils.len(), 1);
        assert!(c.noyau.instantane().code.is_some(), "l'état a été synchronisé");
    }

    #[test]
    fn pendant_un_partage_ni_connexion_ni_deconnexion() {
        // Spec §6 : un login révoque puis réenregistre l'appareil ; pendant une
        // attente, les enveloppes adressées à l'ancien seraient perdues.
        // Neutralisation : retirer `permis_hors_partage` de `connexion` — le
        // connecteur est appelé (compteur à 1).
        let c = contexte("sky-test-app-pas-pendant-partage", true);
        c.noyau.forcer_partage(PartageVue::Demande { ami: "bob".into(), debut_ms: 0 });
        assert_eq!(c.noyau.connexion(), Err(MESSAGE_PENDANT_PARTAGE.to_string()));
        assert_eq!(*c.connexions.lock().unwrap(), 0);
        assert_eq!(c.noyau.deconnexion(), Err(MESSAGE_PENDANT_PARTAGE.to_string()));
        assert!(c.noyau.coffre().jetons().unwrap().is_some(), "la session est intacte");
    }

    #[test]
    fn une_deconnexion_pendant_une_synchronisation_en_vol_n_est_pas_annulee() {
        // RONDE DE CORRECTION 1, Important 1. `synchro` ne couvre pas
        // `deconnexion` (le prendre dans `connexion` bloquerait la boucle 5
        // minutes) : c'est la génération qui garde le chemin. Sans elle, la
        // branche `Ok` réécrivait INCONDITIONNELLEMENT `Connecte` et l'état,
        // réaffichant code ami, amis et listes d'un compte dont les jetons
        // n'existent plus — jusqu'à 5 minutes, et avec le mauvais message.
        //
        // Aucune horloge réelle : le serveur retient sa réponse jusqu'à ce que
        // le test la libère, ce qui place la déconnexion exactement pendant la
        // requête en vol.
        //
        // Neutralisation : retirer le `if d.generation != generation` de
        // `synchroniser` — la réponse en vol ressuscite la session.
        let c = contexte_bloquant("sky-test-app-deconnexion-en-vol", "/api/sky/sync");

        let noyau_du_fil = Arc::clone(&c.noyau);
        let fil = std::thread::spawn(move || noyau_du_fil.synchroniser());

        c.serveur.attendre_la_requete();
        // La requête est partie, la réponse n'est pas encore écrite.
        c.noyau.deconnexion().unwrap();
        assert_eq!(c.noyau.instantane().connexion, Connexion::Deconnecte);

        c.serveur.liberer_la_reponse();
        let issue = fil.join().expect("le fil de synchronisation a paniqué");
        // RONDE DE CORRECTION 2 (Mineur) : ne rien écrire ne suffisait pas —
        // l'état complet, ENVELOPPES COMPRISES, repartait chez l'appelant.
        // Neutralisation : rendre `issue` au lieu de l'erreur — cette
        // assertion-ci rougit, les autres restent vertes.
        match issue {
            Err(ErreurCompte::Protocole(detail)) => assert_eq!(detail, MESSAGE_SESSION_CHANGEE),
            Ok(etat) => panic!(
                "l'état d'une session périmée est rendu à l'appelant, avec {} enveloppe(s) que le \
                 serveur a déjà effacées en les livrant",
                etat.enveloppes.len()
            ),
            Err(autre) => panic!("erreur inattendue : {autre}"),
        }

        let vue = c.noyau.instantane();
        assert_eq!(vue.connexion, Connexion::Deconnecte, "la déconnexion tient");
        assert!(vue.code.is_none(), "aucune donnée du compte déconnecté n'est réaffichée");
        assert!(c.noyau.coffre().jetons().unwrap().is_none(), "les jetons restent oubliés");
    }

    #[test]
    fn une_deconnexion_pendant_une_connexion_gagne_et_oublie_les_jetons_frais() {
        // RONDE DE CORRECTION 2, Important. `deconnexion` n'est bloquée que par
        // `permis_hors_partage` : elle est donc autorisée pendant les cinq
        // minutes que peut durer `connecter`. Deux clics ordinaires suffisent —
        // « Se connecter », le navigateur s'ouvre, « Se déconnecter ». La queue
        // de `connexion` écrivait alors `Connecte` avec une génération
        // concordante, ET les jetons frais que `connecter` venait de ranger
        // restaient dans le coffre : la déconnexion était annulée, jetons
        // compris.
        //
        // Aucune horloge réelle : le connecteur retient sa réponse sur un canal.
        //
        // RONDE DE CORRECTION 3 : ce test exerce le PREMIER point de contrôle
        // (sous le verrou d'écriture), par la fenêtre de `connecter`. Le
        // second a le sien plus bas. Le contrôle intermédiaire que la ronde 2
        // avait placé juste après `connecter` a été supprimé : le retirer seul
        // ne faisait rougir aucun test — voir l'en-tête de `connexion`.
        //
        // Neutralisation : retirer le SEUL premier point de contrôle (le
        // `d.generation != generation` sous le verrou) — ce test rougit.
        let c = contexte_connexion_bloquante("sky-test-app-deconnexion-pendant-connexion");
        assert_eq!(c.noyau.instantane().connexion, Connexion::Deconnecte);

        let noyau_du_fil = Arc::clone(&c.noyau);
        let fil = std::thread::spawn(move || noyau_du_fil.connexion());

        c.attendre_le_connecteur();
        assert_eq!(c.noyau.instantane().connexion, Connexion::EnCours);
        // Le navigateur est ouvert : l'utilisateur clique « Se déconnecter ».
        c.noyau.deconnexion().unwrap();
        assert_eq!(c.noyau.instantane().connexion, Connexion::Deconnecte);

        // Le navigateur rend la main : `connecter` range les jetons et réussit.
        c.liberer_le_connecteur();
        let issue = fil.join().expect("le fil de connexion a paniqué");

        // LE FAIT DANGEREUX D'ABORD, le libellé ensuite.
        assert!(
            c.noyau.coffre().jetons().unwrap().is_none(),
            "les jetons rangés par le connecteur sont oubliés : ne pas les AFFICHER ne suffit pas, \
             le prochain lancement se croirait connecté"
        );
        assert!(
            c.serveur.etat_mut().appareils.is_empty(),
            "aucun appareil n'a été rattaché à la session refusée"
        );
        assert_eq!(c.noyau.instantane().connexion, Connexion::Deconnecte, "la déconnexion gagne");
        assert_eq!(issue, Err(MESSAGE_CONNEXION_ANNULEE.to_string()));

        // CONTRÔLE POSITIF (ronde 3, Mineur 4) : l'assertion ci-dessus passerait
        // trivialement — l'abandon sort avant tout appel réseau, et elle
        // resterait verte avec un double injoignable. Une connexion SANS
        // déconnexion, sur le MÊME noyau et le MÊME double, doit bien
        // enregistrer un appareil. Le connecteur ne bloque que son premier
        // appel : celle-ci n'attend rien.
        c.noyau.connexion().expect("une connexion non interrompue doit réussir");
        assert_eq!(
            c.serveur.etat_mut().appareils.len(),
            1,
            "le double enregistre bien un appareil : l'absence ci-dessus vient de l'abandon"
        );
        assert!(c.noyau.coffre().jetons().unwrap().is_some());
    }

    #[test]
    fn une_deconnexion_pendant_la_lecture_du_nom_n_ecrase_pas_l_etat_deconnecte() {
        // RONDE DE CORRECTION 3, Important 1 — PREMIER point de contrôle, par
        // la fenêtre de `moi()` (jusqu'à 5 s), qui n'avait aucune couverture.
        //
        // CE QUI DISCRIMINE, MESURÉ : l'ÉTAT AFFICHÉ. Sans ce contrôle,
        // `connexion` écrit `Connecte` PAR-DESSUS le `Deconnecte` que
        // l'utilisateur vient de demander, et plus rien ne le corrige : le
        // second point de contrôle, atteint ensuite, lit cet état et croit
        // qu'une AUTRE connexion détient le coffre. L'utilisateur clique « Se
        // déconnecter » et l'écran finit sur « Connecté ».
        //
        // Le compteur d'appareils ci-dessous est un INVARIANT, pas le
        // discriminant : les jetons ayant déjà été oubliés par `deconnexion`,
        // `enregistrer_appareil` échoue avant d'émettre la moindre requête,
        // avec ou sans ce contrôle. Il a son contrôle positif quand même.
        //
        // Neutralisation : retirer le SEUL premier point de contrôle (le
        // `d.generation != generation` sous le verrou qui écrit `nom`) — ce
        // test rougit.
        let c = contexte_connexion_en_pause("sky-test-app-deco-pendant-moi", "/api/auth/me");

        let noyau_du_fil = Arc::clone(&c.noyau);
        let fil = std::thread::spawn(move || noyau_du_fil.connexion());

        // `connecter` a déjà rendu la main ; `moi()` est en vol.
        c.serveur.attendre_la_requete();
        c.noyau.deconnexion().unwrap();
        c.serveur.liberer_la_reponse();
        let issue = fil.join().expect("le fil de connexion a paniqué");

        // LE FAIT D'ABORD, le libellé ensuite.
        assert_eq!(
            c.noyau.instantane().connexion,
            Connexion::Deconnecte,
            "l'utilisateur a cliqué « Se déconnecter » : l'écran ne doit pas finir sur « Connecté »"
        );
        assert!(c.noyau.coffre().jetons().unwrap().is_none(), "aucun jeton ne subsiste");
        // Invariant, pas discriminant — voir le commentaire en tête.
        assert_eq!(c.serveur.appareils_crees(), 0);
        assert_eq!(issue, Err(MESSAGE_CONNEXION_ANNULEE.to_string()));

        // CONTRÔLE POSITIF : le même point d'arrêt, sans déconnexion, enregistre
        // bien un appareil — l'absence ci-dessus n'est pas celle d'un serveur
        // muet.
        c.noyau.connexion().expect("une connexion non interrompue doit réussir");
        assert_eq!(c.serveur.appareils_crees(), 1);
    }

    #[test]
    fn une_deconnexion_pendant_l_enregistrement_de_l_appareil_annule_la_connexion() {
        // RONDE DE CORRECTION 3, Important 1 — SECOND point de contrôle. La
        // fenêtre d'`assurer_appareil` et de `synchroniser` (deux appels
        // réseau) n'avait aucune couverture : le premier point de contrôle est
        // déjà passé quand la déconnexion arrive.
        //
        // CE QUI DISCRIMINE, MESURÉ : l'ISSUE, pas le coffre. Sans ce contrôle,
        // `connexion` rend « Session expirée — reconnecte-toi », un message
        // FAUX et alarmant pour quelqu'un qui vient simplement de cliquer « Se
        // déconnecter » ; et la connexion se termine sans jamais passer par
        // `abandonner_connexion`.
        //
        // L'assertion sur le coffre ci-dessous est un INVARIANT, pas le
        // discriminant : ici les jetons ont été rangés AVANT la déconnexion,
        // donc `deconnexion` les a déjà oubliés et ils sont absents dans les
        // deux cas. L'`oublier()` de ce contrôle ne sert que si un
        // renouvellement de jeton a reposé des jetons frais entre-temps
        // (`avec_jeton_valide` en range sur un 401) — chemin que je n'ai pas su
        // mettre en scène ; c'est dit tel quel dans le rapport.
        //
        // Neutralisation : retirer le SEUL second point de contrôle (le
        // `session_changee` qui suit `assurer_appareil`/`synchroniser`) — ce
        // test rougit, et lui seul.
        let c =
            contexte_connexion_en_pause("sky-test-app-deco-pendant-appareil", "/api/sky/devices");

        let noyau_du_fil = Arc::clone(&c.noyau);
        let fil = std::thread::spawn(move || noyau_du_fil.connexion());

        // `connecter` et `moi()` sont passés ; l'enregistrement est en vol.
        c.serveur.attendre_la_requete();
        c.noyau.deconnexion().unwrap();
        c.serveur.liberer_la_reponse();
        let issue = fil.join().expect("le fil de connexion a paniqué");

        assert_eq!(issue, Err(MESSAGE_CONNEXION_ANNULEE.to_string()));
        // Invariant, pas discriminant — voir le commentaire en tête.
        assert!(c.noyau.coffre().jetons().unwrap().is_none(), "aucun jeton ne subsiste");
        assert_eq!(c.noyau.instantane().connexion, Connexion::Deconnecte, "la déconnexion gagne");
        assert_eq!(
            c.serveur.appareils_crees(),
            1,
            "l'appareil a bien été enregistré avant la déconnexion : c'est la fenêtre visée"
        );
    }

    #[test]
    fn une_connexion_depassee_n_efface_pas_les_jetons_d_une_autre() {
        // RONDE DE CORRECTION 3, Important 2. `abandonner_connexion` oubliait
        // sur TOUT changement de génération. Enchaînement atteignable : « Se
        // connecter » (A, navigateur ouvert) → « Se déconnecter » → « Se
        // connecter » (B, rapide, aboutit) → l'onglet de A aboutit enfin, et A
        // effaçait les jetons VALIDES de B. L'écran restait « Connecté » sur un
        // coffre vide, et chaque requête suivante était refusée en silence.
        // Ni la déconnexion ni B ne sont fautives.
        //
        // Neutralisation : retirer le `if self.donnees().connexion !=
        // Connexion::Deconnecte` de `abandonner_connexion` — ce test rougit, et
        // lui seul.
        let c = contexte_connexion_bloquante("sky-test-app-connexion-depassee");

        // A : « Se connecter », le navigateur s'ouvre et n'a pas encore rendu.
        let noyau_du_fil = Arc::clone(&c.noyau);
        let a = std::thread::spawn(move || noyau_du_fil.connexion());
        c.attendre_le_connecteur();

        // L'utilisateur se déconnecte, puis relance une connexion B. Le
        // connecteur ne bloque QUE son premier appel : B aboutit tout de suite,
        // sans aucune attente réelle, pendant que A est toujours retenu.
        c.noyau.deconnexion().unwrap();
        c.noyau.connexion().expect("la connexion B doit réussir");
        assert_eq!(c.noyau.instantane().connexion, Connexion::Connecte);
        assert!(c.noyau.coffre().jetons().unwrap().is_some(), "B a bien rangé des jetons");

        // L'onglet de A aboutit enfin.
        c.liberer_le_connecteur();
        let issue = a.join().expect("le fil de connexion A a paniqué");

        // LE FAIT DANGEREUX D'ABORD : c'est lui que la neutralisation doit
        // faire tomber, pas le libellé du message.
        //
        // La propriété prouvée est « le coffre n'est pas VIDÉ », pas « ce sont
        // exactement les jetons de B » : en reprenant la main, le connecteur de
        // A range les siens par-dessus, comme le vrai `sky_compte::connecter`.
        // A et B sont le même compte Discord, donc les deux sessions sont
        // valides et peu importe laquelle l'emporte. Ce qui n'est PAS
        // acceptable, c'est le coffre vide : l'écran reste « Connecté » et
        // chaque requête suivante est refusée en silence.
        assert!(
            c.noyau.coffre().jetons().unwrap().is_some(),
            "A a vidé le coffre alors qu'une autre connexion l'avait pris : l'écran reste \
             « Connecté » sur un coffre vide"
        );
        assert_eq!(c.noyau.instantane().connexion, Connexion::Connecte, "B tient");
        assert_eq!(issue, Err(MESSAGE_CONNEXION_REMPLACEE.to_string()));
    }

    #[test]
    fn le_detail_d_une_erreur_de_protocole_est_borne_avant_l_ecran() {
        // Mineur 2 : un seul des huit sites de construction de
        // `Protocole` filtre le corps de la réponse. Neutralisation : rendre
        // `detail.clone()` dans `message_erreur` — le jeton et les 1000
        // caractères arrivent tels quels à l'interface.
        let corps = format!(
            "statut 500 inattendu : {{\"echo\":\"Authorization: Bearer JETON-DE-SESSION\"}}{}",
            "x".repeat(1000)
        );
        let message = message_erreur(&ErreurCompte::Protocole(corps));
        assert!(!message.contains("JETON-DE-SESSION"), "le jeton est coupé");
        assert!(message.chars().count() <= LONGUEUR_MAX_DETAIL + 4, "le détail est borné");
        assert!(message.starts_with("statut 500 inattendu :"), "le début reste lisible");
    }

    #[test]
    fn une_session_refusee_passe_en_session_expiree_et_publie() {
        // Neutralisation : ne pas changer `connexion` sur `Refuse` — reste Connecte.
        let c = contexte("sky-test-app-session-expiree", true);
        c.serveur.etat_mut().refuser_tout = true;
        assert!(matches!(c.noyau.synchroniser(), Err(ErreurCompte::Refuse)));
        assert_eq!(c.noyau.instantane().connexion, Connexion::SessionExpiree);
        let publies = c.coquille.etats.lock().unwrap();
        assert_eq!(publies.last().map(|i| i.connexion), Some(Connexion::SessionExpiree));
    }
}
