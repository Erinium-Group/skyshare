//! Outillage des tests du cœur — jamais compilé hors tests.

use std::io::{Read, Write};
use std::net::TcpListener;
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
const SYNC_COMPLET: &str = concat!(
    r#"{"version":7,"code":"FAUX2345","amis":[],"demandes":[],"#,
    r#""listes":[],"appareils":[],"enveloppes":[]}"#
);

/// Serveur HTTP minimal qui **retient sa réponse** jusqu'à ce que le test la
/// libère : c'est ce qui permet de placer une déconnexion PENDANT une requête
/// en vol (ronde de correction 1, Important 1) sans dépendre d'aucune durée
/// réelle — les deux signaux sont des canaux, pas des attentes.
///
/// `FauxServeur` ne peut pas jouer ce rôle : il répond d'un trait, sur son
/// propre fil, sans point d'arrêt. Ce serveur-ci ne vérifie aucun jeton : la
/// propriété éprouvée est « une réponse d'une session périmée n'écrit rien »,
/// pas l'authentification, que le double couvre déjà.
pub(crate) struct ServeurBloquant {
    url: String,
    recue: Receiver<()>,
    liberer: SyncSender<()>,
    fil: Option<JoinHandle<()>>,
}

impl ServeurBloquant {
    pub fn demarrer() -> ServeurBloquant {
        let ecoute = TcpListener::bind("127.0.0.1:0").expect("socket local");
        let url = format!("http://127.0.0.1:{}", ecoute.local_addr().unwrap().port());
        let (dire_recue, recue) = sync_channel(1);
        let (liberer, attendre_liberation) = sync_channel(1);

        let fil = std::thread::spawn(move || {
            let Ok((mut flux, _)) = ecoute.accept() else { return };
            // Lecture au plus jusqu'à la fin des en-têtes : la requête est un
            // GET, elle n'a pas de corps.
            let mut tampon = [0u8; 2048];
            let _ = flux.read(&mut tampon);
            let _ = dire_recue.send(());
            // Point d'arrêt : la requête est en vol tant que le test n'a pas
            // libéré la réponse.
            let _ = attendre_liberation.recv();
            let reponse = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                SYNC_COMPLET.len(),
                SYNC_COMPLET
            );
            let _ = flux.write_all(reponse.as_bytes());
            let _ = flux.flush();
        });

        ServeurBloquant { url, recue, liberer, fil: Some(fil) }
    }

    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// Rend la main quand la requête est arrivée et attend sa réponse. Borné :
    /// un blocage rend un test rouge, jamais une suite qui ne finit pas.
    pub fn attendre_la_requete(&self) {
        self.recue
            .recv_timeout(Duration::from_secs(5))
            .expect("aucune requête reçue par le serveur bloquant en 5 s");
    }

    pub fn liberer_la_reponse(&self) {
        let _ = self.liberer.send(());
    }
}

impl Drop for ServeurBloquant {
    fn drop(&mut self) {
        // Débloque le fil s'il attend encore, pour ne pas le laisser orphelin.
        let _ = self.liberer.try_send(());
        if let Some(fil) = self.fil.take() {
            let _ = fil.join();
        }
    }
}

pub(crate) struct ContexteBloquant {
    pub serveur: ServeurBloquant,
    pub noyau: Arc<Noyau>,
}

/// Un noyau connecté, branché sur `ServeurBloquant`.
pub(crate) fn contexte_bloquant(prefixe: &str) -> ContexteBloquant {
    let serveur = ServeurBloquant::demarrer();
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
