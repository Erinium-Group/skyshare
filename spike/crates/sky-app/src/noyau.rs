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

use sky_compte::{
    Acceptation, AjoutAmi, Blocage, Coffre, Config, ErreurCompte, Etat, Jetons, Retrait,
};

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
/// Toute commande qui parle au site exige une session.
pub const MESSAGE_NON_CONNECTE: &str = "Connecte-toi d'abord.";
pub const MESSAGE_CODE_MAL_FORME: &str =
    "Ce code ami n'a pas la bonne forme : 8 caractères, par exemple SKY-ABCD-EFGH.";
/// Le site rend le MÊME 404 pour une amitié inexistante, celle d'autrui, ou
/// celle d'un compte qui nous a bloqué — à dessein (`annuaire.rs`). Un seul
/// message, donc, qui ne prétend pas distinguer ce que le site confond.
const MESSAGE_AMI_DISPARU: &str = "Cet ami n'est plus dans ta liste.";

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
                // RONDE DE CORRECTION 4 (constat 2) : ne rien écrire et ne rien
                // rendre ne suffit toujours pas. Un 401 reçu pendant cette
                // requête a pu déclencher la reprise d'`avec_jeton_valide`, et
                // `renouveler` (`sky-compte/src/session.rs`) lit les jetons
                // AVANT son POST puis RANGE le résultat dans le coffre : des
                // jetons frais ont donc pu y atterrir APRÈS la déconnexion qui
                // l'avait vidé. Personne d'autre ne les nettoie — la boucle
                // jette la valeur rendue. Même règle qu'ailleurs : si l'état
                // affiché dit « déconnecté », ces jetons ne sont à personne.
                let oubli = self.oublier_les_orphelins();
                self.publier();
                oubli?;
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
    ///
    /// RONDE DE CORRECTION 5 (Mineur 3) : `d.nom` était la SEULE écriture
    /// d'`Instantane` du module qui ne relisait pas la génération. `moi()` dure
    /// jusqu'à 5 s au lancement ; une déconnexion pendant cet appel laissait le
    /// nom Discord posé sur un état `Deconnecte`, affiché au `publier()`
    /// suivant. Le coffre, lui, était rattrapé par la synchronisation finale —
    /// c'est bien l'affichage, et lui seul, qui restait faux.
    pub fn demarrer(&self) {
        let generation = {
            let d = self.donnees();
            if d.connexion != Connexion::Connecte {
                drop(d);
                self.publier();
                return;
            }
            d.generation
        };
        let nom = sky_compte::moi(&self.config, &self.coffre).ok().map(|m| m.discord_name);
        {
            // Relu SOUS le verrou qui va écrire, comme le premier point de
            // contrôle de `connexion` : le lire avant rouvrirait le TOCTOU.
            let mut d = self.donnees();
            if d.generation != generation {
                drop(d);
                // TÂCHE 8, correction héritée de la tâche 7 : c'était la SEULE
                // des quatre sorties gardées du module à ne pas nettoyer le
                // coffre. `moi()` passe par `avec_jeton_valide`, qui sur un 401
                // renouvelle — et `renouveler` (`sky-compte/src/session.rs`)
                // RANGE les jetons frais. Une déconnexion tombée dans cette
                // fenêtre laissait donc l'écran sur « Déconnecté » avec un
                // coffre PLEIN, et `Noyau::nouveau`, qui déduit l'état du seul
                // `coffre.jetons()`, faisait croire au lancement suivant qu'il
                // était connecté. Même remède qu'aux trois autres sorties.
                //
                // `demarrer` ne rend rien : l'échec de l'oubli passe par
                // `apres_erreur`, comme celui d'`assurer_appareil` plus bas.
                if let Err(e) = self.oublier_les_orphelins() {
                    self.apres_erreur(&e);
                }
                self.publier();
                return;
            }
            d.nom = nom;
        }
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

    /// Les jetons présents dans le coffre n'appartiennent-ils plus à PERSONNE ?
    ///
    /// La réponse se lit sur l'état affiché, jamais sur la génération : seul
    /// `Deconnecte` dit qu'aucune session vivante ne détient le coffre.
    /// `Connecte` ou `EnCours` ne peuvent y avoir été écrits que par un AUTRE
    /// chemin que celui qui pose la question — le nôtre est gardé par sa
    /// génération — donc le coffre est à lui, et y toucher effacerait des
    /// jetons valides (ronde de correction 3, Important 2).
    ///
    /// RONDE DE CORRECTION 4, corrigée à la TÂCHE 8 : ce prédicat est partagé
    /// par les CINQ chemins qui peuvent laisser des jetons orphelins —
    /// `abandonner_connexion`, la branche d'échec de `connexion`, la sortie sur
    /// génération changée de `synchroniser`, celle de `demarrer`, et celle
    /// d'`apres_commande`. Le compte annoncé était « TROIS » : la quatrième
    /// sortie existait déjà mais n'appelait rien, et la cinquième est arrivée
    /// avec les commandes d'annuaire. Le dupliquer avait déjà produit deux fois
    /// la même résurrection ; l'oublier une fois en a produit une troisième.
    ///
    /// LA RÈGLE, pour qui ajoutera la sixième : toute sortie qui renonce à
    /// écrire parce que la génération a changé DOIT passer par ici. Renoncer à
    /// écrire ne suffit jamais — l'appel réseau qu'on vient de faire a pu ranger
    /// des jetons frais par `renouveler`, et personne d'autre ne repasse.
    ///
    /// RONDE DE CORRECTION 5 : la phrase ci-dessus était fausse au moment où
    /// elle a été écrite — la branche d'échec de `connexion` faisait alors un
    /// `oublier()` INCONDITIONNEL. Elle est vraie depuis. C'est exactement le
    /// genre de commentaire qui, dans deux jalons, pousse quelqu'un à retirer
    /// l'un des oublis en croyant l'autre équivalent : le partage annoncé doit
    /// être le partage réel.
    fn jetons_orphelins(&self) -> bool {
        self.donnees().connexion == Connexion::Deconnecte
    }

    /// Vide le coffre si, et seulement si, ses jetons n'appartiennent plus à
    /// personne. L'échec remonte : des jetons refusés qui restent dans le
    /// trousseau feront croire au prochain lancement qu'il est connecté, ce ne
    /// peut pas être silencieux.
    fn oublier_les_orphelins(&self) -> Result<(), ErreurCompte> {
        if self.jetons_orphelins() {
            self.coffre.oublier()
        } else {
            Ok(())
        }
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
    /// - `EnCours` — une autre connexion est en route ; c'est elle qui réglera
    ///   le coffre, dans les DEUX issues : si elle aboutit elle écrase ces
    ///   jetons par les siens, et si elle échoue sa branche d'échec les oublie
    ///   (ronde de correction 4, constat 1). Avant cette ronde la seconde
    ///   moitié de la phrase était fausse, et ces jetons-ci survivaient à une
    ///   déconnexion : l'écran finissait sur « Déconnecté » avec un coffre
    ///   plein, donc le lancement suivant se croyait connecté.
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
        if !self.jetons_orphelins() {
            // Dépassée par une AUTRE connexion : le coffre ne nous appartient
            // plus. RONDE 4 (constat 4) : aucun `publier()` ici — rien n'a
            // changé, et l'autre connexion vient de publier l'état qu'on
            // republierait à l'identique.
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
            {
                let mut d = self.donnees();
                // Même garde sur l'échec : une déconnexion, ou une autre
                // connexion, survenue pendant la tentative a déjà posé son
                // état ; ne rien réécrire et ne toucher à aucun jeton qui ne
                // nous appartient plus.
                if d.generation != generation {
                    return Err(message_erreur(&e));
                }
                d.connexion = Connexion::Deconnecte;
            }
            // RONDE DE CORRECTION 4 (constat 1). Écrire `Deconnecte` ICI
            // signifie qu'AUCUNE session vivante ne détient de jetons : notre
            // génération est intacte, donc rien ne s'est glissé depuis le début
            // de cette connexion. Tout ce qui traîne alors dans le coffre est
            // orphelin — typiquement les jetons d'une connexion PRÉCÉDENTE qui
            // a abouti trop tard, et que `abandonner_connexion` a délibérément
            // laissés parce que cette connexion-ci était `EnCours`. Sans cet
            // oubli, `Noyau::nouveau` (qui déduit l'état du seul
            // `coffre.jetons()`) ferait croire au prochain lancement qu'il est
            // connecté, alors que l'écran affiche « Déconnecté ».
            //
            // RONDE DE CORRECTION 5 (Mineur 2) : `oublier_les_orphelins` et non
            // un `oublier()` aveugle. Le verrou vient d'être relâché ; relire
            // l'état au plus près de l'écriture coûte une lecture et ferme la
            // seule façon dont ce chemin pourrait encore effacer les jetons de
            // quelqu'un d'autre. Le commentaire de `jetons_orphelins` annonçait
            // déjà ce partage — il était faux, il est maintenant vrai.
            let oubli = self.oublier_les_orphelins();
            self.publier();
            if let Err(oubli) = oubli {
                return Err(format!(
                    "{} De plus, les jetons n'ont pas pu être retirés du trousseau ({}) : \
                     le prochain lancement se croira connecté. Déconnecte-toi.",
                    message_erreur(&e),
                    message_erreur(&oubli)
                ));
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

    /// RONDE DE CORRECTION 5 (Important) : la génération bouge AVANT l'oubli,
    /// jamais après.
    ///
    /// Dans l'ordre inverse, il existait une fenêtre entre l'oubli (T1) et
    /// l'incrément (T2) où la génération n'avait pas encore changé. Une
    /// `renouveler` en vol qui range des jetons frais dans cet intervalle
    /// (`sky-compte/src/session.rs` : lecture, POST, rangement) échappait à
    /// TOUTES les gardes : `synchroniser` relisait une génération inchangée,
    /// écrivait `Connecte`, puis T2 écrivait `Deconnecte` par-dessus. Écran
    /// « Déconnecté », coffre plein — et personne ne repassait, la boucle
    /// jetant la valeur rendue. La fenêtre est de l'ordre de la microseconde et
    /// exige une préemption : elle n'est pas ordonnançable depuis un test.
    /// C'est justement pourquoi l'argument ne doit pas rester probabiliste.
    ///
    /// L'incrément d'abord rend la propriété structurelle : TOUTE pose de
    /// jetons postérieure à l'oubli tombe forcément sous une garde.
    ///
    /// SI L'OUBLI ÉCHOUE, la génération aura bougé pour rien, et c'est sans
    /// danger : l'état affiché reste `Connecte` (il n'est écrit qu'après
    /// l'oubli réussi), donc `oublier_les_orphelins` — qui lit cet état — ne
    /// touche à aucun jeton, et le tour de boucle suivant resynchronise sous la
    /// session toujours valide. Le seul effet mesurable est une connexion
    /// concurrente qui s'abandonnerait alors sur « remplacée » : un message
    /// imparfait dans un cas doublement exceptionnel (gestionnaire
    /// d'identifiants en échec PENDANT une authentification), contre une
    /// fenêtre de résurrection réelle. Le compromis est assumé.
    pub fn deconnexion(&self) -> Result<(), String> {
        permis_hors_partage(self.phase())?;
        // Même raison qu'en connexion : une réponse en vol appartient à la
        // session qu'on s'apprête à oublier — y compris une réponse qui n'est
        // pas encore partie.
        self.donnees().generation += 1;
        self.coffre.oublier().map_err(|e| message_erreur(&e))?;
        {
            let mut d = self.donnees();
            d.connexion = Connexion::Deconnecte;
            d.nom = None;
            d.etat = None;
            d.appareil_courant = None;
            d.resynchro_complete = true;
        }
        self.publier();
        Ok(())
    }

    /// Refuse une commande qui exige une session, et rend la GÉNÉRATION de
    /// celle-ci.
    ///
    /// Les deux tiennent ensemble à dessein, sous UN SEUL verrou : toute
    /// commande d'annuaire fait son appel réseau hors verrou, et doit relire
    /// cette génération avant d'écrire quoi que ce soit — sans quoi une
    /// déconnexion demandée pendant l'appel serait annulée par la suite de la
    /// commande. Rendre la génération plutôt que `()` rend l'oubli impossible :
    /// il n'y a rien à capturer séparément, donc rien à oublier de capturer.
    fn exiger_connexion(&self) -> Result<u64, String> {
        let d = self.donnees();
        if d.connexion == Connexion::Connecte {
            Ok(d.generation)
        } else {
            Err(MESSAGE_NON_CONNECTE.to_string())
        }
    }

    /// Après une commande qui a modifié l'annuaire : rafraîchir l'affichage
    /// tout de suite — c'est ce qu'un utilisateur attend d'un clic — et SANS
    /// précédent, parce que le site calcule la version comme le MAX des
    /// `updated_at` des lignes qui RESTENT (voir l'en-tête du module) : un
    /// retrait supprime une ligne sans faire bouger la version, et `?version=`
    /// rendrait `inchange` en gardant l'ami retiré à l'écran.
    ///
    /// CINQUIÈME SORTIE GARDÉE du module, pour la même raison que les quatre
    /// autres. `ajouter_ami` et consorts passent par `avec_jeton_valide` : sur
    /// un 401, `renouveler` (`sky-compte/src/session.rs`) LIT les jetons, POSTe,
    /// puis les RANGE. Une déconnexion tombée dans cet intervalle laisse des
    /// jetons FRAIS dans un coffre qu'elle venait de vider — et sans la garde
    /// ci-dessous la resynchronisation les utiliserait, réussirait, et
    /// réafficherait le compte que l'utilisateur vient de quitter.
    ///
    /// Relire puis écrire sous LE MÊME verrou : lire avant rouvrirait le TOCTOU
    /// que les cinq rondes de correction de la tâche 7 ont fermé partout.
    fn apres_commande(&self, generation: u64) {
        {
            let mut d = self.donnees();
            if d.generation != generation {
                drop(d);
                if let Err(e) = self.oublier_les_orphelins() {
                    self.apres_erreur(&e);
                }
                self.publier();
                return;
            }
            d.resynchro_complete = true;
        }
        let _ = self.synchroniser();
    }

    /// Envoie une demande d'ami à partir de la saisie brute de l'interface.
    pub fn ajouter_ami(&self, saisie: &str) -> Result<String, String> {
        let generation = self.exiger_connexion()?;
        let code = sky_compte::normaliser_code_ami(saisie)
            .ok_or_else(|| MESSAGE_CODE_MAL_FORME.to_string())?;
        // Le site refuse de s'ajouter soi-même par un 400 sans message utile
        // (`annuaire.rs` : les 400 deviennent un `Protocole` brut) : le dire
        // ici, et n'émettre aucune requête.
        if self.donnees().etat.as_ref().is_some_and(|e| e.code == code) {
            return Err("C'est ton propre code ami.".to_string());
        }
        let issue = sky_compte::ajouter_ami(&self.config, &self.coffre, &code)
            .map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande(generation);
        match issue {
            AjoutAmi::Envoyee { .. } => Ok("Demande envoyée.".to_string()),
            // Couvre aussi « ce compte t'a bloqué » : le site ne les distingue
            // pas, l'application non plus.
            AjoutAmi::CodeIntrouvable => Err("Code ami introuvable.".to_string()),
            AjoutAmi::DejaDemandee => Err("Une demande existe déjà avec ce compte.".to_string()),
        }
    }

    pub fn accepter_ami(&self, friendship_id: i64) -> Result<String, String> {
        let generation = self.exiger_connexion()?;
        let issue = sky_compte::accepter_ami(&self.config, &self.coffre, friendship_id)
            .map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande(generation);
        match issue {
            Acceptation::Acceptee => Ok("Demande acceptée.".to_string()),
            Acceptation::Introuvable => Err("Cette demande n'existe plus.".to_string()),
        }
    }

    pub fn retirer_ami(&self, friendship_id: i64) -> Result<String, String> {
        let generation = self.exiger_connexion()?;
        let issue = sky_compte::retirer_ami(&self.config, &self.coffre, friendship_id)
            .map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande(generation);
        match issue {
            Retrait::Retire => Ok("Ami retiré.".to_string()),
            Retrait::Introuvable => Err(MESSAGE_AMI_DISPARU.to_string()),
        }
    }

    pub fn bloquer_ami(&self, friendship_id: i64) -> Result<String, String> {
        let generation = self.exiger_connexion()?;
        let issue = sky_compte::bloquer_ami(&self.config, &self.coffre, friendship_id)
            .map_err(|e| self.apres_erreur(&e))?;
        self.apres_commande(generation);
        match issue {
            Blocage::Bloque => Ok("Ami bloqué.".to_string()),
            Blocage::Introuvable => Err(MESSAGE_AMI_DISPARU.to_string()),
        }
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
        contexte, contexte_bloquant, contexte_bloquant_avec_renouvellement,
        contexte_connexion_bloquante, contexte_connexion_en_pause, contexte_deux_connexions,
        HorlogeDeTest, MESSAGE_ONGLET_FERME,
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
        // l'utilisateur vient de demander, et le second point de contrôle,
        // atteint ensuite, lit cet état et croit qu'une AUTRE connexion détient
        // le coffre : il n'oublie rien et rend « remplacée ».
        //
        // RONDE DE CORRECTION 4 (constat 3) : le récit s'arrêtait un cran trop
        // tôt. MESURÉ sans le contrôle, l'écran ne finit pas sur « Connecté »
        // mais sur `SessionExpiree` — la synchronisation qui suit part avec le
        // coffre déjà vidé par la déconnexion, prend un 401, et `synchroniser`
        // dégrade `Connecte` en `SessionExpiree`. Le défaut reste entier (ni
        // « Déconnecté » à l'écran, ni le bon message), mais c'est bien
        // `SessionExpiree` qu'il faut attendre de la neutralisation.
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
            "l'utilisateur a cliqué « Se déconnecter » : l'écran doit finir sur « Déconnecté », \
             et sur rien d'autre"
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
        // deux cas.
        //
        // NE PAS RETIRER L'`oublier()` DE CE CONTRÔLE au motif qu'aucun test ne
        // le couvre (ronde de correction 5, Mineur 4). Il est le SEUL filet
        // quand la synchronisation vient de `connexion` elle-même pendant la
        // course avec `deconnexion` : `avec_jeton_valide` range des jetons
        // frais sur un 401 (`sky-compte/src/session.rs`), et aucune autre garde
        // ne repasse sur ce chemin-là. C'est un chemin RÉEL, seulement non
        // ordonnançable depuis un test — la fenêtre se referme entre deux
        // appels réseau consécutifs du même fil. Ce que la neutralisation de
        // cette garde mesure, elle, c'est le message rendu à l'utilisateur.
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
    fn une_deconnexion_pendant_le_demarrage_n_affiche_pas_le_nom_discord() {
        // RONDE DE CORRECTION 5, Mineur 3. `demarrer` écrivait `d.nom` sans
        // relire la génération — la seule écriture d'`Instantane` du module
        // dans ce cas. `moi()` dure jusqu'à 5 s au lancement ; une déconnexion
        // pendant cet appel laissait le nom Discord de la session quittée posé
        // sur un état `Deconnecte`, et l'écran l'affichait.
        //
        // CE QUI DISCRIMINE : `nom`. Le coffre ne discrimine pas — la
        // déconnexion l'a déjà vidé, et la synchronisation finale de `demarrer`
        // ne le remplit pas.
        //
        // Aucune horloge réelle : le point d'arrêt retient `/api/auth/me`.
        //
        // Neutralisation : retirer le SEUL `if d.generation != generation` de
        // `demarrer` — ce test rougit.
        let c = contexte_bloquant("sky-test-app-deco-pendant-demarrage", "/api/auth/me");

        let noyau_du_fil = Arc::clone(&c.noyau);
        let fil = std::thread::spawn(move || noyau_du_fil.demarrer());

        c.serveur.attendre_la_requete();
        c.noyau.deconnexion().unwrap();
        c.serveur.liberer_la_reponse();
        fil.join().expect("le fil de démarrage a paniqué");

        assert_eq!(
            c.noyau.instantane().nom,
            None,
            "le nom Discord de la session quittée ne doit pas rester à l'écran"
        );
        assert_eq!(c.noyau.instantane().connexion, Connexion::Deconnecte);

        // CONTRÔLE POSITIF, sur un second point d'arrêt qui ne retient rien de
        // ce que `demarrer` demande : sans déconnexion, le nom EST écrit. Sans
        // lui, l'assertion ci-dessus passerait aussi bien si le double ne
        // rendait aucun nom, ou si `demarrer` n'en écrivait jamais.
        let temoin = contexte_bloquant("sky-test-app-demarrage-temoin", "/api/sky/envelopes");
        temoin.noyau.demarrer();
        assert_eq!(
            temoin.noyau.instantane().nom.as_deref(),
            Some("BOB"),
            "un démarrage non interrompu écrit bien le nom : l'absence ci-dessus vient de la garde"
        );
    }

    #[test]
    fn une_deconnexion_pendant_le_demarrage_ne_laisse_aucun_jeton_dans_le_coffre() {
        // TÂCHE 8, correction héritée de la tâche 7. `demarrer` détectait bien
        // la déconnexion survenue pendant `moi()` — c'est la garde que couvre
        // `une_deconnexion_pendant_le_demarrage_n_affiche_pas_le_nom_discord` —
        // mais sortait SANS nettoyer le coffre : la seule des quatre sorties
        // gardées du module à ne pas appeler `oublier_les_orphelins()`.
        //
        // L'enchaînement, qui n'a rien d'exotique au lancement :
        //   `demarrer` appelle `moi()` → 401 (session à renouveler)
        //   → `avec_jeton_valide` reprend : `renouveler` LIT les jetons, POSTe
        //   → l'utilisateur clique « Se déconnecter » (coffre vidé, écran
        //     « Déconnecté »)
        //   → la réponse du renouvellement arrive et RANGE des jetons FRAIS
        //     (`sky-compte/src/session.rs`) dans le coffre qu'on vient de vider
        //   → `moi()` réussit, `demarrer` relit la génération, sort.
        // Résultat avant la correction : écran « Déconnecté », coffre PLEIN.
        // `Noyau::nouveau` déduisant l'état du seul `coffre.jetons()`, le
        // lancement suivant se croyait connecté sous une session quittée.
        //
        // CE QUI DISCRIMINE : le COFFRE, et lui seul. L'écran finit sur
        // « Déconnecté » avec ou sans la correction — c'est précisément ce qui
        // rendait le défaut invisible.
        //
        // Aucune horloge réelle : le point d'arrêt retient la réponse du
        // renouvellement, ce qui place la déconnexion exactement dans la
        // fenêtre entre la lecture et le rangement des jetons.
        //
        // Neutralisation : retirer le SEUL `oublier_les_orphelins()` de la
        // sortie sur génération changée de `demarrer` — ce test rougit.
        let c = contexte_bloquant_avec_renouvellement(
            "sky-test-app-demarrage-orphelins",
            "/api/auth/refresh",
            "/api/auth/me",
        );

        let noyau_du_fil = Arc::clone(&c.noyau);
        let fil = std::thread::spawn(move || noyau_du_fil.demarrer());

        // `moi()` a pris son 401 ; le renouvellement est en vol.
        c.serveur.attendre_la_requete();
        c.noyau.deconnexion().unwrap();
        assert!(
            c.noyau.coffre().jetons().unwrap().is_none(),
            "la déconnexion a bien vidé le coffre AVANT que le renouvellement ne réponde"
        );

        c.serveur.liberer_la_reponse();
        fil.join().expect("le fil de démarrage a paniqué");

        // LE FAIT DANGEREUX D'ABORD.
        assert!(
            c.noyau.coffre().jetons().unwrap().is_none(),
            "les jetons frais rangés par `renouveler` pendant le démarrage sont oubliés : \
             sinon l'écran dit « Déconnecté » sur un coffre plein, et le lancement suivant \
             se croit connecté"
        );
        assert_eq!(c.noyau.instantane().connexion, Connexion::Deconnecte, "la déconnexion tient");

        // CONTRÔLE POSITIF : sans lui, l'assertion ci-dessus passerait aussi
        // bien si le renouvellement n'avait JAMAIS eu lieu — un coffre resté
        // vide la satisfait. La trace des chemins prouve que la reprise sur 401
        // a bien été jouée, donc que des jetons frais ont bien été rangés dans
        // l'intervalle. Elle prouve aussi que `demarrer` sort là où on le croit :
        // ni `/api/sky/devices` ni `/api/sky/sync` ne suivent.
        assert_eq!(
            c.serveur.chemins(),
            vec!["/api/auth/me", "/api/auth/refresh", "/api/auth/me"],
            "le 401, le renouvellement et la seconde tentative ont bien eu lieu, et \
             `demarrer` s'est arrêté juste après"
        );
    }

    #[test]
    fn une_connexion_qui_echoue_ne_laisse_pas_les_jetons_d_une_connexion_depassee() {
        // RONDE DE CORRECTION 4, constat 1. La branche `EnCours` de
        // `abandonner_connexion` s'appuyait sur « l'autre connexion réglera le
        // coffre » ; c'était faux quand cette autre connexion ÉCHOUE, sa
        // branche d'échec écrivant `Deconnecte` sans jamais oublier.
        //
        // L'enchaînement, tel que mesuré par le relecteur :
        //   A « Se connecter » (navigateur ouvert)
        //   → « Se déconnecter » (coffre vidé, écran « Déconnecté »)
        //   → B « Se connecter » (EnCours)
        //   → l'onglet de A aboutit et range SES jetons ; l'abandon de A voit
        //     que B est en cours, donc n'oublie rien — c'est correct
        //   → B échoue (onglet fermé) et écrit `Deconnecte`.
        // Résultat avant la correction : écran « Déconnecté », coffre PLEIN.
        // Or `Noyau::nouveau` déduit l'état du seul `coffre.jetons()` : le
        // lancement suivant se croyait connecté sous une session que
        // l'utilisateur avait explicitement quittée.
        //
        // Aucune horloge réelle : deux canaux indépendants, un par connexion.
        //
        // Neutralisation : retirer le SEUL `self.coffre.oublier()` de la branche
        // d'échec de `connexion` — ce test rougit.
        let c = contexte_deux_connexions("sky-test-app-echec-apres-depassement");

        // A : le navigateur s'ouvre et n'a pas encore rendu la main.
        let noyau_de_a = Arc::clone(&c.noyau);
        let a = std::thread::spawn(move || noyau_de_a.connexion());
        c.attendre_le_connecteur(0);

        // L'utilisateur se déconnecte, puis relance une connexion B.
        c.noyau.deconnexion().unwrap();
        let noyau_de_b = Arc::clone(&c.noyau);
        let b = std::thread::spawn(move || noyau_de_b.connexion());
        c.attendre_le_connecteur(1);
        assert_eq!(
            c.noyau.instantane().connexion,
            Connexion::EnCours,
            "B est bien EnCours quand l'onglet de A aboutit : c'est la fenêtre visée"
        );

        // L'onglet de A aboutit enfin : il range ses jetons, puis s'abandonne.
        c.liberer_le_connecteur(0);
        assert_eq!(a.join().expect("le fil A a paniqué"), Err(MESSAGE_CONNEXION_REMPLACEE.to_string()));
        // CONTRÔLE POSITIF, au milieu du test : sans lui, l'assertion finale
        // « le coffre est vide » passerait aussi bien si A n'avait jamais rien
        // rangé. A a bel et bien laissé des jetons, et c'était la bonne
        // décision — B les détenait.
        assert!(
            c.noyau.coffre().jetons().unwrap().is_some(),
            "A a rangé ses jetons et l'abandon les a laissés à B : c'est le point de départ du défaut"
        );

        // B échoue : l'onglet a été fermé.
        c.liberer_le_connecteur(1);
        let issue_b = b.join().expect("le fil B a paniqué");

        // LE FAIT DANGEREUX D'ABORD, le libellé ensuite.
        assert!(
            c.noyau.coffre().jetons().unwrap().is_none(),
            "l'écran dit « Déconnecté » : aucun jeton ne doit rester, sinon `Noyau::nouveau` \
             fera croire au prochain lancement qu'il est connecté"
        );
        assert_eq!(c.noyau.instantane().connexion, Connexion::Deconnecte);
        assert_eq!(issue_b, Err(MESSAGE_ONGLET_FERME.to_string()));
    }

    #[test]
    fn un_renouvellement_pendant_une_deconnexion_ne_ressuscite_pas_la_session() {
        // RONDE DE CORRECTION 4, constat 2 — même classe que le constat 1, mais
        // HORS de `connexion`, et antérieure aux trois rondes précédentes.
        //
        // Une `synchroniser` de la boucle prend un 401. `avec_jeton_valide`
        // reprend : `renouveler` LIT les jetons (`session.rs:137`), POSTe, puis
        // RANGE le résultat (`session.rs:142`). Si la déconnexion tombe entre la
        // lecture et le rangement, des jetons FRAIS atterrissent dans un coffre
        // que l'utilisateur vient de vider. La synchronisation sort ensuite sur
        // la garde de génération — sans rien oublier. Personne d'autre ne passe :
        // la boucle jette la valeur rendue.
        //
        // Aucune horloge réelle : le point d'arrêt retient la réponse du
        // renouvellement, ce qui place la déconnexion exactement dans cette
        // fenêtre.
        //
        // Neutralisation : retirer le SEUL `oublier_les_orphelins()` de la
        // sortie sur génération changée de `synchroniser` — ce test rougit.
        let c = contexte_bloquant_avec_renouvellement(
            "sky-test-app-renouvellement-en-vol",
            "/api/auth/refresh",
            "/api/sky/sync",
        );

        let noyau_du_fil = Arc::clone(&c.noyau);
        let fil = std::thread::spawn(move || noyau_du_fil.synchroniser());

        // Le sync a pris son 401 ; le renouvellement est en vol.
        c.serveur.attendre_la_requete();
        c.noyau.deconnexion().unwrap();
        assert!(
            c.noyau.coffre().jetons().unwrap().is_none(),
            "la déconnexion a bien vidé le coffre AVANT que le renouvellement ne réponde"
        );

        c.serveur.liberer_la_reponse();
        let issue = fil.join().expect("le fil de synchronisation a paniqué");

        // LE FAIT DANGEREUX D'ABORD.
        assert!(
            c.noyau.coffre().jetons().unwrap().is_none(),
            "les jetons frais rangés par `renouveler` sont oubliés : sinon le prochain \
             lancement se croit connecté sous une session que l'utilisateur a quittée"
        );
        assert_eq!(c.noyau.instantane().connexion, Connexion::Deconnecte, "la déconnexion tient");
        match issue {
            Err(ErreurCompte::Protocole(detail)) => assert_eq!(detail, MESSAGE_SESSION_CHANGEE),
            autre => panic!("issue inattendue : {autre:?}"),
        }

        // CONTRÔLE POSITIF : sans lui, toutes les assertions ci-dessus
        // passeraient si le renouvellement n'avait JAMAIS eu lieu — un coffre
        // resté vide les satisfait toutes. La trace des chemins prouve que la
        // reprise sur 401 a bien été jouée, et donc que des jetons frais ont
        // bien été rangés dans l'intervalle.
        assert_eq!(
            c.serveur.chemins(),
            vec!["/api/sky/sync", "/api/auth/refresh", "/api/sky/sync"],
            "le 401, le renouvellement et la seconde tentative ont bien eu lieu"
        );
    }

    #[test]
    fn on_ne_s_ajoute_pas_soi_meme_et_rien_ne_part() {
        // Le site refuserait par un 400 sans message utile (`ajouter_ami`,
        // `annuaire.rs` : les 400 deviennent un `Protocole` brut). Neutralisation :
        // retirer la comparaison au code de l'état — la demande part
        // (`appels_amis` à 1).
        let c = contexte("sky-test-app-propre-code", true);
        c.noyau.synchroniser().unwrap(); // code « FAUX2345 » du double
        assert_eq!(
            c.noyau.ajouter_ami("sky-faux-2345"),
            Err("C'est ton propre code ami.".to_string())
        );
        assert_eq!(c.serveur.etat_mut().appels_amis, 0);
    }

    #[test]
    fn une_saisie_mal_formee_ne_part_pas() {
        let c = contexte("sky-test-app-code-mal-forme", true);
        assert_eq!(c.noyau.ajouter_ami("pas un code"), Err(MESSAGE_CODE_MAL_FORME.to_string()));
        assert_eq!(c.serveur.etat_mut().appels_amis, 0);
    }

    #[test]
    fn retirer_un_ami_le_fait_disparaitre_meme_si_la_version_ne_bouge_pas() {
        // Le double, comme le site, ne fait pas progresser la version sur un
        // retrait : il SUPPRIME la ligne, et la version est le MAX des
        // `updated_at` de celles qui RESTENT. Neutralisation : retirer
        // `d.resynchro_complete = true` de `apres_commande` — la
        // resynchronisation part avec `?version=`, le site rend `inchange`, et
        // l'ami reste affiché.
        let c = contexte("sky-test-app-retirer", true);
        let mut ami = crate::faux_serveur::AmiFaux::sans_appareil("bob");
        ami.friendship_id = 777;
        c.serveur.etat_mut().amis.push(ami);
        c.noyau.synchroniser().unwrap();
        assert_eq!(c.noyau.instantane().amis.len(), 1);

        assert_eq!(c.noyau.retirer_ami(777), Ok("Ami retiré.".to_string()));
        assert!(c.noyau.instantane().amis.is_empty());
    }

    #[test]
    fn une_commande_d_annuaire_exige_une_session() {
        // La serrure posée ET branchée : `exiger_connexion` n'a de valeur que
        // si les quatre commandes l'appellent.
        //
        // CE QUI DISCRIMINE, MESURÉ : le MESSAGE. Sans la garde, la commande
        // descend jusqu'à `jeton_courant`, qui rend `Refuse` faute de jeton, et
        // `apres_erreur` traduit ce refus en « Session expirée —
        // reconnecte-toi » ET bascule l'état affiché en `SessionExpiree` : un
        // message alarmant et faux pour quelqu'un qui ne s'est simplement
        // jamais connecté. `appels_amis` reste à 0 dans les DEUX cas — le refus
        // vient d'avant le réseau — donc ce compteur est un invariant, pas le
        // discriminant.
        //
        // Neutralisation : remplacer `self.exiger_connexion()?` d'`accepter_ami`
        // par la seule lecture de génération — ce test rougit.
        let c = contexte("sky-test-app-annuaire-sans-session", false);
        assert_eq!(c.noyau.instantane().connexion, Connexion::Deconnecte);
        assert_eq!(c.noyau.ajouter_ami("SKY-ABCD-EFGH"), Err(MESSAGE_NON_CONNECTE.to_string()));
        assert_eq!(c.noyau.accepter_ami(1), Err(MESSAGE_NON_CONNECTE.to_string()));
        assert_eq!(c.noyau.retirer_ami(1), Err(MESSAGE_NON_CONNECTE.to_string()));
        assert_eq!(c.noyau.bloquer_ami(1), Err(MESSAGE_NON_CONNECTE.to_string()));
        assert_eq!(c.serveur.etat_mut().appels_amis, 0);

        // CONTRÔLE POSITIF : le MÊME double, avec une session, laisse bien
        // partir la demande. Sans lui, les assertions ci-dessus passeraient
        // aussi avec un serveur injoignable ou un double qui ne compte rien.
        let d = contexte("sky-test-app-annuaire-avec-session", true);
        d.serveur.etat_mut().code_ami_valide = Some(("ABCD2345".to_string(), 42));
        assert_eq!(d.noyau.ajouter_ami("SKY-ABCD-2345"), Ok("Demande envoyée.".to_string()));
        assert_eq!(d.serveur.etat_mut().appels_amis, 1);
    }

    #[test]
    fn une_deconnexion_pendant_un_ajout_d_ami_n_est_pas_annulee() {
        // La discipline de génération du module vaut AUSSI pour les commandes
        // d'annuaire : leur appel réseau se fait hors verrou, donc une
        // déconnexion peut tomber pendant.
        //
        // L'enchaînement, celui du constat 2 de la ronde 4 déplacé sur une
        // commande :
        //   `ajouter_ami` POSTe `/api/sky/friends` → 401
        //   → `avec_jeton_valide` reprend : `renouveler` LIT les jetons, POSTe
        //   → l'utilisateur clique « Se déconnecter » (coffre vidé, gén. + 1)
        //   → la réponse du renouvellement RANGE des jetons FRAIS dans ce
        //     coffre vidé, et la commande retente puis aboutit
        //   → `apres_commande` resynchronise.
        // Sans la garde, cette resynchronisation-là RÉUSSIT (les jetons frais
        // sont valides), prend sa PROPRE génération — postérieure à la
        // déconnexion, donc concordante — et écrit `Connecte` avec le code ami
        // et les amis du compte que l'utilisateur vient de quitter.
        //
        // CE QUI DISCRIMINE : l'état affiché ET le coffre, les deux.
        //
        // Aucune horloge réelle : le point d'arrêt retient la réponse du
        // renouvellement.
        //
        // Neutralisation : retirer le SEUL `if d.generation != generation`
        // d'`apres_commande` — ce test rougit.
        let c = contexte_bloquant_avec_renouvellement(
            "sky-test-app-ajout-en-vol",
            "/api/auth/refresh",
            "/api/sky/friends",
        );
        // Le point d'arrêt ne connaît pas `/api/sky/friends` : sa seconde
        // tentative rend 404, que `ajouter_ami` traduit en `CodeIntrouvable`.
        // L'issue rendue importe peu — ce qui est visé, c'est ce qu'écrit
        // l'après-commande.
        let noyau_du_fil = Arc::clone(&c.noyau);
        let fil = std::thread::spawn(move || noyau_du_fil.ajouter_ami("SKY-ABCD-EFGH"));

        c.serveur.attendre_la_requete();
        c.noyau.deconnexion().unwrap();
        assert!(
            c.noyau.coffre().jetons().unwrap().is_none(),
            "la déconnexion a bien vidé le coffre AVANT que le renouvellement ne réponde"
        );
        c.serveur.liberer_la_reponse();
        let _ = fil.join().expect("le fil d'ajout a paniqué");

        // LE FAIT DANGEREUX D'ABORD.
        assert!(
            c.noyau.coffre().jetons().unwrap().is_none(),
            "les jetons frais rangés par `renouveler` pendant la commande sont oubliés"
        );
        assert_eq!(
            c.noyau.instantane().connexion,
            Connexion::Deconnecte,
            "l'utilisateur s'est déconnecté : l'après-commande ne doit pas réafficher le \
             compte quitté"
        );
        assert!(c.noyau.instantane().code.is_none(), "aucune donnée du compte quitté");

        // CONTRÔLE POSITIF : la trace prouve que la reprise sur 401 a bien été
        // jouée — donc que des jetons frais ont bien été rangés dans
        // l'intervalle — et qu'aucune synchronisation n'a suivi.
        assert_eq!(
            c.serveur.chemins(),
            vec!["/api/sky/friends", "/api/auth/refresh", "/api/sky/friends"],
            "le 401, le renouvellement et la seconde tentative ont eu lieu, et aucune \
             synchronisation d'après-commande n'a suivi"
        );

        // SECOND CONTRÔLE POSITIF, sur un noyau intact : sans déconnexion, la
        // commande rafraîchit bien l'affichage tout de suite. Sans lui, les
        // assertions ci-dessus passeraient aussi si `apres_commande` ne
        // synchronisait JAMAIS.
        let d = contexte("sky-test-app-ajout-temoin", true);
        assert!(d.noyau.instantane().code.is_none(), "rien n'a encore été synchronisé");
        let _ = d.noyau.ajouter_ami("SKY-ABCD-EFGH");
        assert_eq!(
            d.noyau.instantane().code.as_deref(),
            Some("FAUX2345"),
            "une commande d'annuaire rafraîchit bien l'affichage tout de suite"
        );
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
