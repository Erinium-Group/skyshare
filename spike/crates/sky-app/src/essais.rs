//! Outillage des tests du cœur — jamais compilé hors tests.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sky_compte::{Coffre, Config, ErreurCompte, Jetons};
use sky_encode::Codec;
use sky_partage::{Arret, ErreurPartage, Evenement, Fin};

use crate::coquille::Coquille;
use crate::faux_serveur::FauxServeur;
use crate::noyau::{Branchements, Noyau};
use crate::partage::Partageur;
use crate::reveil::Horloge;
use crate::vue::{EcranVue, Instantane};

pub(crate) struct CoquilleEspion {
    pub etats: Mutex<Vec<Instantane>>,
    /// Les valeurs reçues par `demarrage_automatique`, dans l'ordre : le seul
    /// moyen de prouver que la case passe bien par la coquille — c'est elle
    /// qui touche le registre de Windows, jamais le noyau.
    pub demarrages: Mutex<Vec<bool>>,
    /// Les valeurs reçues par `icone_partage`, dans l'ordre : l'icône près de
    /// l'horloge n'existe pas dans un test sans fenêtre, seul son pilotage se
    /// prouve.
    pub icones: Mutex<Vec<bool>>,
    /// Ce que la coquille répond à `ecrans()`. Le test le change pour mettre en
    /// scène un écran branché ou débranché, sans aucun matériel.
    pub ecrans: Mutex<Vec<EcranVue>>,
}

/// TROIS écrans par défaut, et non zéro (ronde de correction 2) : `partager`
/// valide désormais le rang contre la liste relevée, donc un espion vide
/// refuserait tous les partages et les tests de la tâche 11 changeraient de
/// sujet sans le dire. Les tests qui mettent en scène un branchement posent
/// leur propre liste.
impl Default for CoquilleEspion {
    fn default() -> CoquilleEspion {
        CoquilleEspion {
            etats: Mutex::new(Vec::new()),
            demarrages: Mutex::new(Vec::new()),
            icones: Mutex::new(Vec::new()),
            ecrans: Mutex::new(
                (0..3)
                    .map(|index| EcranVue {
                        index,
                        nom: format!("Écran de test {}", index + 1),
                        principal: index == 0,
                    })
                    .collect(),
            ),
        }
    }
}

impl Coquille for Arc<CoquilleEspion> {
    fn publier_etat(&self, instantane: &Instantane) {
        self.etats.lock().unwrap().push(instantane.clone());
    }

    fn demarrage_automatique(&self, actif: bool) -> Result<(), String> {
        self.demarrages.lock().unwrap().push(actif);
        Ok(())
    }

    fn icone_partage(&self, actif: bool) {
        self.icones.lock().unwrap().push(actif);
    }

    fn ecrans(&self) -> Vec<EcranVue> {
        self.ecrans.lock().unwrap().clone()
    }
}

/// La fenêtre qu'annonce le factice — DISTINCTE de `FENETRE_HOTE` EXPRÈS.
///
/// `Noyau::partager` pose déjà `FENETRE_HOTE` au clic, avant même de lancer le
/// fil. Un factice qui annoncerait la même valeur rendrait indistinguables
/// « l'événement est arrivé jusqu'à l'instantané » et « la réservation l'avait
/// déjà écrit » : mesuré, un test écrit ainsi restait VERT alors que
/// `appliquer` ne traitait plus l'événement du tout.
pub(crate) const FENETRE_FACTICE: Duration = Duration::from_secs(90);

/// Un partage qui annonce sa disponibilité puis attend l'arrêt, sans réseau
/// ni carte graphique. Aucune capture d'écran, aucune session NVENC.
pub(crate) struct PartageurFactice;

impl Partageur for PartageurFactice {
    fn heberger(
        &self,
        _noyau: &Noyau,
        _codec: Codec,
        _ecran: usize,
        arret: &Arret,
        evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage> {
        evenements(Evenement::Pret);
        evenements(Evenement::Disponible { fenetre: FENETRE_FACTICE });
        while !arret.est_demande() {
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(Fin::Arrete)
    }

    fn regarder(
        &self,
        _noyau: &Noyau,
        _ami: i64,
        _lancement: Instant,
        arret: &Arret,
        _evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage> {
        while !arret.est_demande() {
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(Fin::Arrete)
    }
}

/// Horloge que le test avance à la main : aucune durée réelle n'est attendue,
/// et le plancher entre deux synchronisations devient observable.
pub(crate) struct HorlogeDeTest {
    instant: Mutex<Instant>,
}

impl HorlogeDeTest {
    pub fn nouvelle() -> Arc<HorlogeDeTest> {
        Arc::new(HorlogeDeTest { instant: Mutex::new(Instant::now()) })
    }

    pub fn avancer(&self, duree: Duration) {
        let mut instant = self.instant.lock().unwrap();
        *instant += duree;
    }
}

impl Horloge for Arc<HorlogeDeTest> {
    fn maintenant(&self) -> Instant {
        *self.instant.lock().unwrap()
    }
}

pub(crate) struct Contexte {
    pub serveur: FauxServeur,
    pub noyau: Arc<Noyau>,
    pub coquille: Arc<CoquilleEspion>,
    pub horloge: Arc<HorlogeDeTest>,
    /// Nombre d'appels au connecteur (qui, en production, ouvre le navigateur).
    pub connexions: Arc<Mutex<u32>>,
}

/// Un noyau branché sur un serveur double. `connecte` : des jetons valides
/// sont déjà dans le coffre. La connexion simulée range un jeton reconnu par
/// le double, sans navigateur.
pub(crate) fn contexte(prefixe: &str, connecte: bool) -> Contexte {
    let serveur = FauxServeur::demarrer();
    let jeton = serveur.jeton_de_test();
    let coffre = Coffre::pour_test(prefixe);
    if connecte {
        coffre
            .ranger_jetons(&Jetons {
                session: jeton.clone(),
                renouvellement: "peu-importe".into(),
            })
            .unwrap();
    }
    let coquille = Arc::new(CoquilleEspion::default());
    let horloge = HorlogeDeTest::nouvelle();
    let connexions = Arc::new(Mutex::new(0u32));
    let compteur = Arc::clone(&connexions);
    let noyau = Arc::new(Noyau::nouveau(
        Config::vers(&serveur.url()),
        coffre,
        Branchements {
            coquille: Box::new(Arc::clone(&coquille)),
            connecter: Box::new(move |_config, coffre| {
                *compteur.lock().unwrap() += 1;
                let jetons =
                    Jetons { session: jeton.clone(), renouvellement: "peu-importe".into() };
                coffre.ranger_jetons(&jetons)?;
                Ok(jetons)
            }),
            nom_machine: Some("MACHINE-DE-TEST".into()),
            horloge: Box::new(Arc::clone(&horloge)),
            partageur: Box::new(PartageurFactice),
        },
    ));
    Contexte { serveur, noyau, coquille, horloge, connexions }
}

// --- Serveur qui retient sa réponse -----------------------------------

/// Corps d'une réponse `GET /api/sky/sync` complète et valide — la forme du
/// double, réduite à ce que `sky_compte::synchroniser` lit.
///
/// Elle porte UNE enveloppe (ronde de correction 2, Mineur) : le serveur
/// l'efface en la livrant, donc une réponse rendue à un appelant d'une session
/// périmée la perdrait pour son vrai destinataire. C'est cette enveloppe-là que
/// le test cherche à ne pas voir ressortir.
const SYNC_COMPLET: &str = concat!(
    r#"{"version":7,"code":"FAUX2345","amis":[],"demandes":[],"listes":[],"appareils":[],"#,
    r#""enveloppes":[{"id":"1","expediteur_device_id":1,"destinataire_device_id":2,"#,
    r#""charge":"QUJDRA=="}]}"#
);

/// `GET /api/auth/me` — `sky_compte::moi` ne lit que ces deux champs.
const MOI: &str = r#"{"id":1,"discordName":"BOB"}"#;

/// `POST /api/auth/refresh` — `sky_compte::renouveler` ne lit que `acces`, et
/// RANGE le résultat dans le coffre (`session.rs:142`). C'est ce rangement-là
/// que le test du constat 2 cherche à voir nettoyé.
const RENOUVELLEMENT: &str = r#"{"acces":"jeton-renouvele"}"#;

/// Serveur HTTP minimal qui **retient la première réponse** à un chemin choisi,
/// jusqu'à ce que le test la libère. C'est ce qui permet de placer une
/// déconnexion PENDANT une requête précise, sans dépendre d'aucune durée réelle
/// — les deux signaux sont des canaux, pas des attentes.
///
/// CE N'EST PAS UN SECOND DOUBLE (le premier, `FauxServeur`, est dérivé du code
/// du site et reste la référence de forme) : c'est un **point d'arrêt**. Il ne
/// vérifie aucun jeton, ne valide aucune entrée, et rend des réponses fixes
/// réduites à ce que `sky-compte` lit. La fidélité au contrat est le travail du
/// double ; celui-ci n'apporte que la pause, que `FauxServeur` ne sait pas
/// faire — il répond d'un trait, sur son propre fil.
///
/// RONDE DE CORRECTION 3 : le chemin retenu est désormais un paramètre. Les
/// trois contrôles de génération de `connexion` s'exercent chacun dans une
/// fenêtre différente (`/api/auth/me` pour le 2, `/api/sky/devices` pour le 3),
/// et sans cela aucun test ne les distinguait les uns des autres.
///
/// Seule la PREMIÈRE requête au chemin retenu est retenue : le contrôle positif
/// d'un test rejoue ensuite le même chemin sans blocage.
pub(crate) struct ServeurBloquant {
    url: String,
    port: u16,
    recue: Receiver<()>,
    liberer: SyncSender<()>,
    appareils_crees: Arc<AtomicUsize>,
    chemins: Arc<Mutex<Vec<String>>>,
    arret: Arc<AtomicBool>,
    fil: Option<JoinHandle<()>>,
}

impl ServeurBloquant {
    pub fn demarrer_en_retenant(chemin_retenu: &str) -> ServeurBloquant {
        ServeurBloquant::demarrer(chemin_retenu, None)
    }

    /// `chemin_refuse` : la PREMIÈRE requête à ce chemin rend 401. C'est le seul
    /// moyen de déclencher la reprise d'`avec_jeton_valide`
    /// (`sky-compte/src/session.rs`) — renouvellement, puis seconde tentative —
    /// et donc de faire ranger des jetons FRAIS dans le coffre pendant qu'un
    /// appel authentifié est en vol (ronde de correction 4, constat 2).
    ///
    /// TÂCHE 8 : le chemin refusé est devenu un paramètre. Le constat 2 le
    /// posait sur `/api/sky/sync` (une synchronisation en vol) ; la sortie
    /// gardée de `demarrer` se prend, elle, dans la fenêtre de `moi()`, donc sur
    /// `/api/auth/me`. Sans ce paramètre, aucun test ne pouvait faire ranger des
    /// jetons frais pendant un DÉMARRAGE.
    pub fn demarrer(chemin_retenu: &str, chemin_refuse: Option<&str>) -> ServeurBloquant {
        let ecoute = TcpListener::bind("127.0.0.1:0").expect("socket local");
        let port = ecoute.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}");
        let (dire_recue, recue) = sync_channel(1);
        let (liberer, attendre_liberation) = sync_channel(1);
        let appareils_crees = Arc::new(AtomicUsize::new(0));
        let chemins = Arc::new(Mutex::new(Vec::new()));
        let arret = Arc::new(AtomicBool::new(false));

        let chemin_retenu = chemin_retenu.to_string();
        let chemin_refuse = chemin_refuse.map(str::to_string);
        let compteur = Arc::clone(&appareils_crees);
        let journal = Arc::clone(&chemins);
        let arret_du_fil = Arc::clone(&arret);
        let fil = std::thread::spawn(move || {
            let mut deja_retenu = false;
            let mut deja_refuse = false;
            loop {
                let Ok((mut flux, _)) = ecoute.accept() else { return };
                if arret_du_fil.load(Ordering::SeqCst) {
                    return;
                }
                // Délai de lecture : une requête tronquée rend le test rouge,
                // jamais une suite qui ne finit pas.
                let _ = flux.set_read_timeout(Some(Duration::from_secs(5)));
                let Some(chemin) = lire_chemin(&mut flux) else { continue };

                journal.lock().unwrap().push(chemin.clone());

                // Statut ET phrase de raison : « HTTP/1.1 404 » suivi d'un
                // espace et de rien du tout n'est pas une ligne de statut
                // valide (RFC 9112 §4) — ronde de correction 4, constat 5.
                // Le refus vient AVANT le routage : c'est un 401 du serveur,
                // pas une variante de réponse de la route.
                let refuse_celle_ci = !deja_refuse
                    && chemin_refuse.as_deref().is_some_and(|r| chemin.starts_with(r));
                if refuse_celle_ci {
                    deja_refuse = true;
                }

                let (statut, raison, corps) = if refuse_celle_ci {
                    (401, "Unauthorized", r#"{"error":"session expirée"}"#.to_string())
                } else if chemin.starts_with("/api/sky/devices") {
                    let n = compteur.fetch_add(1, Ordering::SeqCst) + 1;
                    (201, "Created", format!("{{\"id\":{n}}}"))
                } else if chemin.starts_with("/api/auth/me") {
                    (200, "OK", MOI.to_string())
                } else if chemin.starts_with("/api/auth/refresh") {
                    (200, "OK", RENOUVELLEMENT.to_string())
                } else if chemin.starts_with("/api/sky/sync") {
                    (200, "OK", SYNC_COMPLET.to_string())
                } else {
                    (404, "Not Found", r#"{"error":"route inconnue du point d'arrêt"}"#.to_string())
                };

                if !deja_retenu && chemin.starts_with(&chemin_retenu) {
                    deja_retenu = true;
                    let _ = dire_recue.send(());
                    // Point d'arrêt : la requête est en vol tant que le test
                    // n'a pas libéré la réponse.
                    let _ = attendre_liberation.recv();
                }

                let reponse = format!(
                    "HTTP/1.1 {statut} {raison}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corps}",
                    corps.len()
                );
                let _ = flux.write_all(reponse.as_bytes());
                let _ = flux.flush();
            }
        });

        ServeurBloquant { url, port, recue, liberer, appareils_crees, chemins, arret, fil: Some(fil) }
    }

    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// Nombre d'appareils créés par `POST /api/sky/devices` — l'observable qui
    /// distingue le contrôle 2 du contrôle 3 : sans le contrôle 2, la connexion
    /// abandonnée enregistre quand même un appareil avant d'être arrêtée.
    pub fn appareils_crees(&self) -> usize {
        self.appareils_crees.load(Ordering::SeqCst)
    }

    /// Les chemins reçus, dans l'ordre. Contrôle positif du test du constat 2 :
    /// sans lui, « le coffre est vide » passerait aussi bien si le
    /// renouvellement n'avait jamais eu lieu.
    pub fn chemins(&self) -> Vec<String> {
        self.chemins.lock().unwrap().clone()
    }

    /// Rend la main quand la requête retenue est arrivée. Borné : un blocage
    /// rend un test rouge, jamais une suite qui ne finit pas.
    pub fn attendre_la_requete(&self) {
        self.recue
            .recv_timeout(Duration::from_secs(5))
            .expect("aucune requête retenue reçue par le point d'arrêt en 5 s");
    }

    pub fn liberer_la_reponse(&self) {
        let _ = self.liberer.send(());
    }
}

/// Lit une requête HTTP entière (en-têtes puis corps annoncé par
/// `Content-Length`) et rend son chemin. Le corps est lu même s'il n'est pas
/// utilisé : répondre puis fermer avant que le client ait fini d'écrire lui
/// donnerait une erreur d'écriture plutôt que la réponse.
fn lire_chemin(flux: &mut std::net::TcpStream) -> Option<String> {
    let mut brut = Vec::new();
    let mut tampon = [0u8; 1024];
    let fin_des_entetes = loop {
        match brut.windows(4).position(|f| f == b"\r\n\r\n") {
            Some(position) => break position + 4,
            None => {
                let lus = flux.read(&mut tampon).ok()?;
                if lus == 0 {
                    return None;
                }
                brut.extend_from_slice(&tampon[..lus]);
            }
        }
    };
    let entetes = String::from_utf8_lossy(&brut[..fin_des_entetes]).to_string();
    let taille_du_corps = entetes
        .lines()
        .find_map(|ligne| ligne.to_ascii_lowercase().strip_prefix("content-length:").map(str::trim).and_then(|v| v.parse::<usize>().ok()))
        .unwrap_or(0);
    while brut.len() < fin_des_entetes + taille_du_corps {
        let lus = flux.read(&mut tampon).ok()?;
        if lus == 0 {
            break;
        }
        brut.extend_from_slice(&tampon[..lus]);
    }
    entetes.lines().next()?.split_whitespace().nth(1).map(str::to_string)
}

impl Drop for ServeurBloquant {
    fn drop(&mut self) {
        // Débloque le fil s'il attend encore, puis réveille `accept` par une
        // connexion à vide, pour ne pas le laisser orphelin.
        self.arret.store(true, Ordering::SeqCst);
        let _ = self.liberer.try_send(());
        let _ = std::net::TcpStream::connect(("127.0.0.1", self.port));
        if let Some(fil) = self.fil.take() {
            let _ = fil.join();
        }
    }
}

pub(crate) struct ContexteBloquant {
    pub serveur: ServeurBloquant,
    pub noyau: Arc<Noyau>,
}

/// Un noyau connecté, branché sur `ServeurBloquant`, dont le connecteur n'est
/// pas utilisé.
pub(crate) fn contexte_bloquant(prefixe: &str, chemin_retenu: &str) -> ContexteBloquant {
    contexte_bloquant_avec(prefixe, chemin_retenu, None)
}

/// Le même, mais la première requête à `chemin_refuse` est refusée (401) : la
/// reprise d'`avec_jeton_valide` renouvelle le jeton et le RANGE dans le coffre
/// au milieu de l'appel. C'est le dispositif du constat 2 de la ronde 4
/// (`/api/sky/sync`) et, tâche 8, de la sortie gardée de `demarrer`
/// (`/api/auth/me`).
pub(crate) fn contexte_bloquant_avec_renouvellement(
    prefixe: &str,
    chemin_retenu: &str,
    chemin_refuse: &str,
) -> ContexteBloquant {
    contexte_bloquant_avec(prefixe, chemin_retenu, Some(chemin_refuse))
}

fn contexte_bloquant_avec(
    prefixe: &str,
    chemin_retenu: &str,
    chemin_refuse: Option<&str>,
) -> ContexteBloquant {
    let serveur = ServeurBloquant::demarrer(chemin_retenu, chemin_refuse);
    let coffre = Coffre::pour_test(prefixe);
    coffre
        .ranger_jetons(&Jetons {
            session: "jeton-de-test".into(),
            renouvellement: "peu-importe".into(),
        })
        .unwrap();
    let noyau = Arc::new(Noyau::nouveau(
        Config::vers(&serveur.url()),
        coffre,
        Branchements {
            coquille: Box::new(Arc::new(CoquilleEspion::default())),
            connecter: Box::new(|_config, _coffre| {
                Err(ErreurCompte::Protocole("connecteur non utilisé par ce test".into()))
            }),
            nom_machine: Some("MACHINE-DE-TEST".into()),
            horloge: Box::new(HorlogeDeTest::nouvelle()),
            partageur: Box::new(PartageurFactice),
        },
    ));
    ContexteBloquant { serveur, noyau }
}

/// Un noyau DÉCONNECTÉ dont le connecteur réussit tout de suite (jetons rangés
/// dans le coffre, comme le vrai), mais dont le SERVEUR retient une requête
/// choisie. C'est le dispositif des contrôles 2 et 3 de `connexion` : la
/// déconnexion se place pendant `moi()` ou pendant
/// `assurer_appareil`/`synchroniser`, jamais pendant `connecter`.
pub(crate) fn contexte_connexion_en_pause(prefixe: &str, chemin_retenu: &str) -> ContexteBloquant {
    let serveur = ServeurBloquant::demarrer_en_retenant(chemin_retenu);
    // Coffre vide : l'application démarre déconnectée.
    let coffre = Coffre::pour_test(prefixe);
    let noyau = Arc::new(Noyau::nouveau(
        Config::vers(&serveur.url()),
        coffre,
        Branchements {
            coquille: Box::new(Arc::new(CoquilleEspion::default())),
            connecter: Box::new(|_config, coffre| {
                let jetons = Jetons {
                    session: "jeton-de-test".into(),
                    renouvellement: "peu-importe".into(),
                };
                coffre.ranger_jetons(&jetons)?;
                Ok(jetons)
            }),
            nom_machine: Some("MACHINE-DE-TEST".into()),
            horloge: Box::new(HorlogeDeTest::nouvelle()),
            partageur: Box::new(PartageurFactice),
        },
    ));
    ContexteBloquant { serveur, noyau }
}

// --- Connecteur qui retient sa réponse ---------------------------------

/// Un noyau DÉCONNECTÉ dont le connecteur se bloque à l'appel jusqu'à ce que
/// le test le libère — l'équivalent, pour la connexion, de `ServeurBloquant`
/// (ronde de correction 2, Important).
///
/// En production, `sky_compte::connecter` ouvre le navigateur et attend au
/// plus cinq minutes ; c'est cette fenêtre que le test doit pouvoir occuper à
/// volonté, sans jamais attendre une durée réelle. Comme le vrai, ce
/// connecteur **range les jetons dans le coffre avant de rendre la main** :
/// c'est précisément ce qui rend l'annulation insuffisante si elle se contente
/// de ne rien afficher.
pub(crate) struct ContexteConnexion {
    pub serveur: FauxServeur,
    pub noyau: Arc<Noyau>,
    appele: Receiver<()>,
    liberer: SyncSender<()>,
}

impl ContexteConnexion {
    /// Rend la main quand le connecteur a été appelé (le « navigateur » est
    /// ouvert). Borné : un blocage rend un test rouge, pas une suite sans fin.
    pub fn attendre_le_connecteur(&self) {
        self.appele
            .recv_timeout(Duration::from_secs(5))
            .expect("le connecteur n'a pas été appelé en 5 s");
    }

    pub fn liberer_le_connecteur(&self) {
        let _ = self.liberer.send(());
    }
}

pub(crate) fn contexte_connexion_bloquante(prefixe: &str) -> ContexteConnexion {
    let serveur = FauxServeur::demarrer();
    // DEUX jetons distincts, tous deux reconnus par le double : sans cela, un
    // test qui vérifie « le coffre de la seconde connexion a survécu » ne
    // distinguerait pas les deux sessions.
    let jeton_du_premier = serveur.jeton_de_test_pour("premier");
    let jeton_des_suivants = serveur.jeton_de_test_pour("suivants");
    // Coffre vide : l'application démarre déconnectée.
    let coffre = Coffre::pour_test(prefixe);
    let (dire_appele, appele) = sync_channel(1);
    let (liberer, attendre) = sync_channel(1);
    // `Receiver` est `Send` mais pas `Sync` : le `Connecteur` exige les deux.
    let attendre = Mutex::new(attendre);
    // RONDE DE CORRECTION 3 : SEUL LE PREMIER APPEL BLOQUE. Avant, tous les
    // appels attendaient le même canal : un test qui enchaîne deux connexions
    // voyait la seconde attendre pour de bon le délai de 5 s, et l'ordre réel
    // n'était plus celui que le test croyait mettre en scène. Mesuré sur le
    // test A/B : la première connexion reprenait AVANT que la seconde ait rangé
    // ses jetons, et l'assertion passait pour la mauvaise raison.
    let appels = AtomicUsize::new(0);
    let noyau = Arc::new(Noyau::nouveau(
        Config::vers(&serveur.url()),
        coffre,
        Branchements {
            coquille: Box::new(Arc::new(CoquilleEspion::default())),
            connecter: Box::new(move |_config, coffre| {
                let premier = appels.fetch_add(1, Ordering::SeqCst) == 0;
                if premier {
                    let _ = dire_appele.send(());
                    let _ = attendre.lock().unwrap().recv_timeout(Duration::from_secs(5));
                }
                let session = if premier {
                    jeton_du_premier.clone()
                } else {
                    jeton_des_suivants.clone()
                };
                let jetons = Jetons { session, renouvellement: "peu-importe".into() };
                coffre.ranger_jetons(&jetons)?;
                Ok(jetons)
            }),
            nom_machine: Some("MACHINE-DE-TEST".into()),
            horloge: Box::new(HorlogeDeTest::nouvelle()),
            partageur: Box::new(PartageurFactice),
        },
    ));
    ContexteConnexion { serveur, noyau, appele, liberer }
}

/// Le message que rend le connecteur quand « l'onglet a été fermé ».
pub(crate) const MESSAGE_ONGLET_FERME: &str = "onglet de connexion fermé";

/// Deux connexions retenues INDÉPENDAMMENT : la première aboutit (jetons
/// rangés, comme le vrai `connecter`), la seconde échoue sans rien ranger —
/// l'onglet Discord fermé. Deux canaux distincts, parce que l'enchaînement du
/// constat 2 exige que la SECONDE soit déjà `EnCours` quand la PREMIÈRE aboutit
/// (ronde de correction 4, constat 1).
///
/// Aucune durée réelle n'est attendue : les deux attentes sont des
/// `recv_timeout` de 5 s, donc un blocage rend un test rouge et jamais une
/// suite sans fin.
pub(crate) struct ContexteDeuxConnexions {
    /// Tenu en vie, jamais lu : la première connexion appelle `moi()` avant de
    /// s'abandonner, et le double doit encore répondre à ce moment-là. Le
    /// relâcher ici fermerait le serveur au milieu du test. Nommé avec un
    /// souligné PLUTÔT qu'assorti d'une assertion décorative : une assertion
    /// qui ne discrimine rien n'est pas une preuve (ronde 3, Mineur 4).
    _serveur: FauxServeur,
    pub noyau: Arc<Noyau>,
    appelee: [Receiver<()>; 2],
    liberer: [SyncSender<()>; 2],
}

impl ContexteDeuxConnexions {
    /// `rang` : 0 pour la première connexion, 1 pour la seconde.
    pub fn attendre_le_connecteur(&self, rang: usize) {
        self.appelee[rang]
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|_| panic!("le connecteur n°{rang} n'a pas été appelé en 5 s"));
    }

    pub fn liberer_le_connecteur(&self, rang: usize) {
        let _ = self.liberer[rang].send(());
    }
}

pub(crate) fn contexte_deux_connexions(prefixe: &str) -> ContexteDeuxConnexions {
    let serveur = FauxServeur::demarrer();
    let jeton_du_premier = serveur.jeton_de_test_pour("premier");
    // Coffre vide : l'application démarre déconnectée.
    let coffre = Coffre::pour_test(prefixe);
    let (dire_a, appele_a) = sync_channel(1);
    let (dire_b, appele_b) = sync_channel(1);
    let (liberer_a, attendre_a) = sync_channel(1);
    let (liberer_b, attendre_b) = sync_channel(1);
    // `Receiver` est `Send` mais pas `Sync` : le `Connecteur` exige les deux.
    // DEUX verrous distincts, jamais un tableau sous un seul : la première
    // connexion attend PENDANT que la seconde s'exécute, et un verrou partagé
    // la ferait attendre le verrou au lieu de son canal — l'ordre mis en scène
    // par le test ne serait plus celui qu'il annonce.
    let attendre_a = Mutex::new(attendre_a);
    let attendre_b = Mutex::new(attendre_b);
    let appels = AtomicUsize::new(0);
    let noyau = Arc::new(Noyau::nouveau(
        Config::vers(&serveur.url()),
        coffre,
        Branchements {
            coquille: Box::new(Arc::new(CoquilleEspion::default())),
            connecter: Box::new(move |_config, coffre| {
                let rang = appels.fetch_add(1, Ordering::SeqCst);
                assert!(rang < 2, "ce contexte ne met en scène que DEUX connexions");
                if rang == 0 {
                    let _ = dire_a.send(());
                    let _ = attendre_a.lock().unwrap().recv_timeout(Duration::from_secs(5));
                    let jetons = Jetons {
                        session: jeton_du_premier.clone(),
                        renouvellement: "peu-importe".into(),
                    };
                    coffre.ranger_jetons(&jetons)?;
                    Ok(jetons)
                } else {
                    let _ = dire_b.send(());
                    let _ = attendre_b.lock().unwrap().recv_timeout(Duration::from_secs(5));
                    // Le vrai `connecter` ne range RIEN quand il échoue.
                    Err(ErreurCompte::Protocole(MESSAGE_ONGLET_FERME.to_string()))
                }
            }),
            nom_machine: Some("MACHINE-DE-TEST".into()),
            horloge: Box::new(HorlogeDeTest::nouvelle()),
            partageur: Box::new(PartageurFactice),
        },
    ));
    ContexteDeuxConnexions {
        _serveur: serveur,
        noyau,
        appelee: [appele_a, appele_b],
        liberer: [liberer_a, liberer_b],
    }
}
