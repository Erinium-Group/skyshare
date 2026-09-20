//! Outillage des tests du cœur — jamais compilé hors tests.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sky_compte::{Coffre, Config, ErreurCompte, Jetons};

use crate::coquille::Coquille;
use crate::faux_serveur::FauxServeur;
use crate::noyau::{Branchements, Noyau};
use crate::reveil::Horloge;
use crate::vue::Instantane;

#[derive(Default)]
pub(crate) struct CoquilleEspion {
    pub etats: Mutex<Vec<Instantane>>,
}

impl Coquille for Arc<CoquilleEspion> {
    fn publier_etat(&self, instantane: &Instantane) {
        self.etats.lock().unwrap().push(instantane.clone());
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
    arret: Arc<AtomicBool>,
    fil: Option<JoinHandle<()>>,
}

impl ServeurBloquant {
    pub fn demarrer_en_retenant(chemin_retenu: &str) -> ServeurBloquant {
        let ecoute = TcpListener::bind("127.0.0.1:0").expect("socket local");
        let port = ecoute.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}");
        let (dire_recue, recue) = sync_channel(1);
        let (liberer, attendre_liberation) = sync_channel(1);
        let appareils_crees = Arc::new(AtomicUsize::new(0));
        let arret = Arc::new(AtomicBool::new(false));

        let chemin_retenu = chemin_retenu.to_string();
        let compteur = Arc::clone(&appareils_crees);
        let arret_du_fil = Arc::clone(&arret);
        let fil = std::thread::spawn(move || {
            let mut deja_retenu = false;
            loop {
                let Ok((mut flux, _)) = ecoute.accept() else { return };
                if arret_du_fil.load(Ordering::SeqCst) {
                    return;
                }
                // Délai de lecture : une requête tronquée rend le test rouge,
                // jamais une suite qui ne finit pas.
                let _ = flux.set_read_timeout(Some(Duration::from_secs(5)));
                let Some(chemin) = lire_chemin(&mut flux) else { continue };

                let (statut, corps) = if chemin.starts_with("/api/sky/devices") {
                    let n = compteur.fetch_add(1, Ordering::SeqCst) + 1;
                    (201, format!("{{\"id\":{n}}}"))
                } else if chemin.starts_with("/api/auth/me") {
                    (200, MOI.to_string())
                } else if chemin.starts_with("/api/sky/sync") {
                    (200, SYNC_COMPLET.to_string())
                } else {
                    (404, r#"{"error":"route inconnue du point d'arrêt"}"#.to_string())
                };

                if !deja_retenu && chemin.starts_with(&chemin_retenu) {
                    deja_retenu = true;
                    let _ = dire_recue.send(());
                    // Point d'arrêt : la requête est en vol tant que le test
                    // n'a pas libéré la réponse.
                    let _ = attendre_liberation.recv();
                }

                let reponse = format!(
                    "HTTP/1.1 {statut} \r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corps}",
                    corps.len()
                );
                let _ = flux.write_all(reponse.as_bytes());
                let _ = flux.flush();
            }
        });

        ServeurBloquant { url, port, recue, liberer, appareils_crees, arret, fil: Some(fil) }
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
    let serveur = ServeurBloquant::demarrer_en_retenant(chemin_retenu);
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
        },
    ));
    ContexteConnexion { serveur, noyau, appele, liberer }
}
