//! Connexion native par boucle locale (RFC 8252).
//!
//! L'application tire un secret, ouvre un serveur HTTP sur `127.0.0.1` (port choisi
//! par le système), ouvre le navigateur vers le site — qui fait l'authentification
//! Discord puis redirige vers ce serveur avec un code à usage unique — puis échange ce
//! code contre des jetons via `POST /api/auth/native`.
//!
//! Trois propriétés à ne pas défaire :
//! - le port voyage dans la signature que le site calcule (`port`, `empreinte`), donc
//!   il n'est pas falsifiable ;
//! - l'URL de redirection ne porte jamais de jeton, seulement un code ;
//! - l'échange exige le secret, que seule cette application détient — un code
//!   intercepté seul ne sert donc à rien. C'est pourquoi le secret ne part jamais dans
//!   `url_de_depart` : seule son empreinte SHA-256 y figure.

use std::time::{Duration, Instant};

use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::coffre::{Coffre, Jetons};
use crate::erreur::ErreurCompte;
use crate::http::{ClientHttp, Config};

/// Délai maximal d'attente de la redirection sur le serveur de boucle locale.
///
/// Point de départ ARGUMENTÉ, pas une mesure — contrairement au délai de `ClientHttp`
/// (5 s, celui-là mesuré sur le réveil de Neon). Une authentification Discord réelle
/// suppose : basculer vers le navigateur, retrouver ou taper un mot de passe (gestionnaire
/// de mots de passe compris), puis éventuellement un second facteur — application
/// d'authentification à rouvrir, ou SMS à attendre. Quelques dizaines de secondes pour
/// un utilisateur entraîné qui ne se trompe pas ; largement plus pour un premier essai,
/// une faute de frappe à corriger, ou un second facteur qui traîne. 5 minutes laisse
/// cette marge sans faire attendre indéfiniment quelqu'un qui a fermé l'onglet ou
/// renoncé.
const DELAI_ATTENTE_REDIRECTION: Duration = Duration::from_secs(5 * 60);

/// Page rendue au navigateur une fois la redirection reçue — le résultat réel
/// (succès ou échec) est rapporté dans le terminal par l'appelant, pas ici : cette
/// page est affichée avant même que l'échange du code n'ait eu lieu.
const PAGE_DE_RETOUR: &str =
    "<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body>\
     <p>Vous pouvez retourner dans le terminal SkyShare.</p></body></html>";

/// Encode `octets` en hexadécimal minuscule.
fn vers_hex(octets: &[u8]) -> String {
    octets.iter().map(|o| format!("{o:02x}")).collect()
}

/// Tire 32 octets aléatoires (générateur du système) et les rend en hexadécimal.
/// Ce secret ne doit jamais quitter la machine avant l'échange final — voir le
/// commentaire de module.
pub fn secret_aleatoire() -> String {
    let mut octets = [0u8; 32];
    rand::rng().fill_bytes(&mut octets);
    vers_hex(&octets)
}

/// Empreinte SHA-256 de `secret`, en hexadécimal minuscule — c'est elle, jamais le
/// secret, qui part vers le site au départ du flux.
pub fn empreinte_du_secret(secret: &str) -> String {
    let condense = Sha256::digest(secret.as_bytes());
    vers_hex(&condense)
}

/// URL à ouvrir dans le navigateur pour démarrer l'authentification Discord.
///
/// Ne prend pas de `Config` : ce flux ne peut de toute façon jamais viser le serveur
/// double des tests (qui n'implémente pas l'authentification Discord réelle), donc il
/// résout sa cible de la même façon que le reste du produit — `SKY_API_URL` si posée,
/// sinon la production. `echanger_le_code`, lui, reçoit le `Config` de l'appelant :
/// c'est par là que les tests redirigent l'échange du code vers le double.
///
/// `natif=1` est ce qui fait signer au site un state NATIF portant le port et
/// l'empreinte (`api/auth/discord/route.ts`, `parametres.get("natif") === "1"`).
/// Sans lui, le site ignore `port` et `empreinte`, signe un state web ordinaire, et le
/// retour de Discord ne redirige jamais vers la boucle locale : `login` attend
/// indéfiniment. Trouvé au premier essai réel, 19/09/2026.
pub fn url_de_depart(port: u16, empreinte: &str) -> String {
    let base = Config::depuis_env().base_url;
    format!("{base}/api/auth/discord?natif=1&port={port}&empreinte={empreinte}")
}

#[derive(Serialize)]
struct CorpsEchange<'a> {
    code: &'a str,
    secret: &'a str,
}

#[derive(Deserialize)]
struct ReponseEchange {
    acces: String,
    refresh: String,
}

/// Le `POST /api/auth/native` seul, isolé du navigateur pour être testable : envoie
/// `{ code, secret }`, rend les jetons de session en cas de succès.
pub fn echanger_le_code(config: &Config, code: &str, secret: &str) -> Result<Jetons, ErreurCompte> {
    let client = ClientHttp::new(config);
    let corps = CorpsEchange { code, secret };
    let reponse: ReponseEchange = client.post_json("/api/auth/native", &corps, None)?;
    Ok(Jetons { session: reponse.acces, renouvellement: reponse.refresh })
}

#[derive(Serialize)]
struct CorpsRefresh<'a> {
    refresh: &'a str,
}

/// `POST /api/auth/refresh` rend un seul champ — jamais de nouveau jeton de
/// renouvellement (voir `gerer_refresh` côté double, et
/// `api/auth/refresh/route.ts` côté site) : un renouvellement ne fait que
/// remplacer le jeton de session, le jeton de renouvellement présenté reste
/// valable pour la prochaine fois.
#[derive(Deserialize)]
struct ReponseRefresh {
    acces: String,
}

/// Renouvelle le jeton de session à partir du jeton de renouvellement rangé dans
/// `coffre`, range le résultat dans `coffre`, le rend.
///
/// Ce point d'entrée a échoué pour 100 % des tentatives réelles pendant tout le
/// jalon C1, faute d'appelant — voir le brief de cette tâche. `avec_jeton_valide`,
/// plus bas, en est le premier appelant réel.
///
/// Si `coffre` ne contient aucun jeton (jamais connecté), rend `ErreurCompte::Refuse`
/// AVANT tout appel réseau : du point de vue de l'appelant, l'absence de session à
/// renouveler appelle la même réaction qu'un renouvellement refusé par le serveur —
/// se reconnecter via `connecter`. Cette garde reste nécessaire même si
/// `avec_jeton_valide` ne l'atteint jamais avec un coffre vide (voir son propre
/// garde-fou juste avant) : `renouveler` est elle-même `pub`, appelable directement
/// par un futur appelant qui n'en connaît pas l'usage interne — sans cette garde,
/// `.renouvellement` paniquerait sur `None`.
pub fn renouveler(config: &Config, coffre: &Coffre) -> Result<Jetons, ErreurCompte> {
    let actuels = coffre.jetons()?.ok_or(ErreurCompte::Refuse)?;
    let client = ClientHttp::new(config);
    let corps = CorpsRefresh { refresh: &actuels.renouvellement };
    let reponse: ReponseRefresh = client.post_json("/api/auth/refresh", &corps, None)?;
    let jetons = Jetons { session: reponse.acces, renouvellement: actuels.renouvellement };
    coffre.ranger_jetons(&jetons)?;
    Ok(jetons)
}

/// Rend le jeton de session actuellement rangé dans `coffre` — AUCUN appel réseau,
/// ne dit rien de sa validité auprès du serveur, seulement de sa présence locale.
/// `Refuse` si le coffre ne contient encore aucun jeton (jamais connecté).
///
/// Volontairement dépourvue de toute notion de validité : un appel réseau de
/// vérification préalable (une version antérieure de cette fonction sondait
/// `GET /api/sky/sync` avant même l'appel réel) doublerait le nombre de requêtes
/// sur la route la plus fréquente de l'application, dont le budget est publié et
/// mesuré (`docs/mesures-jalon-c1.md`, côté site). `avec_jeton_valide`, juste après,
/// compose ce jeton avec un vrai appel et réagit au 401 s'il survient — un sondage
/// dirait « le jeton était bon il y a un instant » ; une reprise sur 401 dit « CET
/// appel a été refusé », ce qui ne peut jamais se désynchroniser de la réalité.
pub fn jeton_courant(coffre: &Coffre) -> Result<String, ErreurCompte> {
    Ok(coffre.jetons()?.ok_or(ErreurCompte::Refuse)?.session)
}

/// Exécute `appel` avec le jeton de session courant ; si le serveur le refuse (401,
/// donc `ErreurCompte::Refuse`), renouvelle une fois via `renouveler` puis retente
/// `appel` une seule fois de plus avec le nouveau jeton.
///
/// UNE SEULE reprise, jamais deux : tout échec au-delà de cette unique reprise —
/// renouvellement lui-même refusé, ou nouveau jeton de session encore refusé —
/// remonte tel quel, sans nouvelle tentative. Sans cette borne, un jeton de
/// renouvellement révoqué déclencherait une boucle infinie d'appels au serveur.
///
/// C'est ici, et seulement ici, qu'un appel réseau réel est tenté avant d'avoir la
/// certitude que le jeton est bon — contrairement à `jeton_courant`, qui ne
/// suppose rien. Les tâches 8 et 9 appellent `avec_jeton_valide` pour la totalité
/// de leurs appels authentifiés (`GET /api/sky/sync`, `POST /api/sky/envelopes`,
/// etc.), jamais `jeton_courant` seule pour un appel réel : `jeton_courant` ne rend
/// qu'un jeton PRÉSENT, pas un jeton VALIDE.
pub fn avec_jeton_valide<T>(
    config: &Config,
    coffre: &Coffre,
    appel: impl Fn(&str) -> Result<T, ErreurCompte>,
) -> Result<T, ErreurCompte> {
    let jeton = jeton_courant(coffre)?;
    match appel(&jeton) {
        Err(ErreurCompte::Refuse) => {
            let renouveles = renouveler(config, coffre)?;
            appel(&renouveles.session)
        }
        resultat => resultat,
    }
}

/// Sépare `code` et `error` de la requête de redirection reçue par le serveur local
/// (`/?code=...` ou `/?error=...`). Une valeur vide compte comme absente.
fn extraire_code_ou_erreur(chemin: &str) -> (Option<String>, Option<String>) {
    let Some((_, requete)) = chemin.split_once('?') else {
        return (None, None);
    };
    let mut code = None;
    let mut erreur = None;
    for paire in requete.split('&') {
        if let Some(valeur) = paire.strip_prefix("code=").filter(|v| !v.is_empty()) {
            code = Some(valeur.to_string());
        } else if let Some(valeur) = paire.strip_prefix("error=").filter(|v| !v.is_empty()) {
            erreur = Some(valeur.to_string());
        }
    }
    (code, erreur)
}

/// Longueur maximale de la raison `error=` recopiée dans le terminal.
const LONGUEUR_MAX_RAISON: usize = 64;

/// Le vrai rappel du site, une fois reconnu parmi les requêtes reçues en boucle locale.
#[derive(Debug, PartialEq, Eq)]
enum Rappel {
    Code(String),
    /// Raison DÉJÀ assainie (`assainir_raison`) : jamais la valeur brute.
    Erreur(String),
}

/// Réduit la valeur `error=` à ce qu'on peut afficher sans risque : lettres et
/// chiffres ASCII, `_`, `-`, `.`, au plus `LONGUEUR_MAX_RAISON` caractères.
///
/// Cette valeur n'est pas forcément écrite par le site : n'importe quel processus
/// local qui atteint le port peut la choisir (revue finale, m4). Recopiée brute, elle
/// porterait caractères de contrôle et séquences d'échappement jusqu'au terminal
/// (effacer l'écran, changer le titre, maquiller un message). Une liste blanche
/// plutôt qu'une liste noire : aucune séquence inventée demain ne la franchit.
fn assainir_raison(brute: &str) -> String {
    let propre: String = brute
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .take(LONGUEUR_MAX_RAISON)
        .collect();
    if propre.is_empty() {
        "(raison illisible)".to_string()
    } else {
        propre
    }
}

/// Reconnaît le rappel du site parmi les requêtes reçues : `GET` sur le chemin `/`
/// exactement (celui de `callback/route.ts` et `totp/verify/route.ts` côté site :
/// `http://127.0.0.1:<port>/?code=...`), portant `code` ou `error`. Toute autre
/// requête — `/favicon.ico` demandé par le navigateur, sonde locale, préchargement —
/// n'est PAS le rappel : `None`, et l'attente continue.
fn classer_requete_locale(est_get: bool, url: &str) -> Option<Rappel> {
    if !est_get {
        return None;
    }
    let chemin = url.split_once('?').map_or(url, |(chemin, _)| chemin);
    if chemin != "/" {
        return None;
    }
    match extraire_code_ou_erreur(url) {
        (_, Some(raison)) => Some(Rappel::Erreur(assainir_raison(&raison))),
        (Some(code), None) => Some(Rappel::Code(code)),
        (None, None) => None,
    }
}

/// Attend le rappel du site sur `serveur` jusqu'à `echeance`. Chaque requête qui
/// n'est pas le rappel reçoit un 404 et l'attente reprend, avec le temps qui reste —
/// jamais un délai remis à zéro. `Ok(None)` à l'échéance.
///
/// Avant cette correction (revue finale, m4), la PREMIÈRE requête reçue était prise
/// pour le rappel : un `/favicon.ico` ou une sonde locale arrivé avant la
/// redirection faisait échouer la connexion (« redirection locale sans code ni
/// erreur »), et le serveur fermait avant que le vrai rappel n'arrive.
fn attendre_rappel(serveur: &tiny_http::Server, echeance: Instant) -> Result<Option<Rappel>, ErreurCompte> {
    loop {
        let reste = echeance.saturating_duration_since(Instant::now());
        if reste.is_zero() {
            return Ok(None);
        }
        let Some(requete) = serveur.recv_timeout(reste).map_err(|e| ErreurCompte::Reseau(e.to_string()))? else {
            return Ok(None);
        };
        let est_get = *requete.method() == tiny_http::Method::Get;
        match classer_requete_locale(est_get, requete.url()) {
            None => {
                let _ = requete.respond(tiny_http::Response::from_string("").with_status_code(404));
            }
            Some(rappel) => {
                let reponse = tiny_http::Response::from_string(PAGE_DE_RETOUR).with_header(
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                        .expect("en-tete statique toujours valide"),
                );
                let _ = requete.respond(reponse);
                return Ok(Some(rappel));
            }
        }
    }
}

/// Programme et arguments qui ouvrent `url` dans le navigateur par défaut.
///
/// JAMAIS `cmd /C start` : `cmd` réinterprète sa ligne de commande, et le `&` qui
/// sépare les paramètres de l'URL (`?port=…&empreinte=…`) y devient un séparateur de
/// commandes — le navigateur recevait une URL tronquée au premier `&`, et `cmd`
/// tentait d'exécuter `empreinte=…`. Trouvé au premier essai réel, 19/09/2026 :
/// aucun test n'ouvrait de navigateur. `rundll32 url.dll,FileProtocolHandler` reçoit
/// l'URL comme UN argument, sans interpréteur de commandes entre les deux.
fn commande_du_navigateur(url: &str) -> (&'static str, [&str; 2]) {
    ("rundll32", ["url.dll,FileProtocolHandler", url])
}

/// Ouvre le navigateur par défaut sur `url` (le projet est Windows uniquement), et
/// affiche l'adresse en secours si le navigateur ne s'ouvre pas.
///
/// Afficher cette adresse est sans risque : elle porte le port local et l'EMPREINTE
/// du secret, jamais le secret lui-même, sans lequel le code rendu par le site ne
/// s'échange contre rien (`POST /api/auth/native`).
fn ouvrir_navigateur(url: &str) -> Result<(), ErreurCompte> {
    eprintln!("Si le navigateur ne s'ouvre pas, colle cette adresse dans ton navigateur :\n{url}\n");
    let (programme, arguments) = commande_du_navigateur(url);
    std::process::Command::new(programme)
        .args(arguments)
        .spawn()
        .map(|_| ())
        .map_err(|e| ErreurCompte::Reseau(format!("impossible d'ouvrir le navigateur : {e}")))
}

/// Connexion native complète : tire un secret, ouvre un serveur de boucle locale sur
/// un port libre, ouvre le navigateur, attend l'unique redirection, échange le code
/// contre des jetons, les range dans `coffre`.
///
/// Non testable automatiquement : ouvre un vrai navigateur et attend une vraie
/// requête entrante. `url_de_depart` et `echanger_le_code`, elles, sont testées
/// séparément.
pub fn connecter(config: &Config, coffre: &Coffre) -> Result<Jetons, ErreurCompte> {
    let secret = secret_aleatoire();
    let empreinte = empreinte_du_secret(&secret);

    let serveur = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| ErreurCompte::Reseau(format!("ouverture du serveur local impossible : {e}")))?;
    // `ListenAddr::IP` est aujourd'hui la seule variante tant que la fonctionnalité
    // socket Unix de `tiny_http` n'est pas activée (elle ne l'est pas ici) — même
    // remarque que `faux_serveur::adresse_port`.
    let tiny_http::ListenAddr::IP(adresse) = serveur.server_addr();
    let port = adresse.port();

    ouvrir_navigateur(&url_de_depart(port, &empreinte))?;

    // Attente BORNÉE (voir DELAI_ATTENTE_REDIRECTION) : sans délai, un utilisateur qui
    // ferme l'onglet ou renonce laisserait ce thread bloqué pour toujours — sans
    // message, sans moyen d'en sortir. L'échéance est fixée UNE fois : les requêtes
    // étrangères au rappel, qui reçoivent un 404 (voir `attendre_rappel`), ne la
    // repoussent pas.
    let rappel = attendre_rappel(&serveur, Instant::now() + DELAI_ATTENTE_REDIRECTION);
    // Sortie propre dans tous les cas (rappel, erreur, délai) : le serveur local ne
    // reste pas à écouter derrière.
    drop(serveur);

    let code = match rappel? {
        None => {
            // Le message ne nomme ni le code ni le secret — seulement qu'il faut relancer.
            return Err(ErreurCompte::Reseau(
                "aucun rappel du site reçu dans le délai imparti — relancez la connexion".to_string(),
            ));
        }
        Some(Rappel::Erreur(raison)) => {
            return Err(ErreurCompte::Protocole(format!(
                "authentification refusee par le site : {raison}"
            )));
        }
        Some(Rappel::Code(code)) => code,
    };

    let jetons = echanger_le_code(config, &code, &secret)?;
    coffre.ranger_jetons(&jetons)?;
    Ok(jetons)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Ouverture du navigateur (essai réel, 19/09/2026) ---------------

    #[test]
    fn l_url_de_depart_passe_au_navigateur_en_un_seul_argument_intact() {
        // Échoue si l'ouverture repassait par `cmd` (qui coupe au premier `&`
        // et exécute la suite), ou si l'URL était découpée en plusieurs
        // arguments : le site recevrait un `state` sans son empreinte.
        let url = url_de_depart(51234, "empreinte-de-test");
        assert!(url.contains('&'), "le test n'a de sens que si l'URL porte un `&`");
        let (programme, arguments) = commande_du_navigateur(&url);
        assert!(
            !programme.eq_ignore_ascii_case("cmd") && !programme.eq_ignore_ascii_case("cmd.exe"),
            "`cmd` réinterprète `&` comme séparateur de commandes"
        );
        assert_eq!(
            arguments.iter().filter(|a| **a == url).count(),
            1,
            "l'URL doit être transmise en un seul argument, intacte"
        );
    }

    // --- Serveur de boucle locale (revue finale, m4) ------------------

    #[test]
    fn le_rappel_du_site_est_reconnu_code_ou_erreur() {
        assert_eq!(classer_requete_locale(true, "/?code=ABC123"), Some(Rappel::Code("ABC123".to_string())));
        assert_eq!(
            classer_requete_locale(true, "/?error=access_denied"),
            Some(Rappel::Erreur("access_denied".to_string()))
        );
    }

    #[test]
    fn une_requete_hors_du_chemin_de_rappel_n_est_pas_le_rappel() {
        // Échoue si le contrôle du chemin disparaissait : `/autre?code=...` porte bien
        // un code, mais pas sur le chemin où le site redirige (`/`). `/favicon.ico`,
        // `/` nu et un code vide ne portent aucun rappel exploitable.
        assert_eq!(classer_requete_locale(true, "/autre?code=ABC123"), None);
        assert_eq!(classer_requete_locale(true, "/favicon.ico"), None);
        assert_eq!(classer_requete_locale(true, "/"), None);
        assert_eq!(classer_requete_locale(true, "/?code="), None);
    }

    #[test]
    fn une_requete_autre_que_get_n_est_pas_le_rappel() {
        // Échoue si la méthode n'était pas vérifiée : le navigateur suit la
        // redirection du site par un GET, rien d'autre.
        assert_eq!(classer_requete_locale(false, "/?code=ABC123"), None);
    }

    #[test]
    fn la_raison_d_erreur_ne_porte_ni_controle_ni_sequence_d_echappement() {
        // Échoue si la valeur de `error=` était recopiée brute : ESC, BEL, CR/LF et les
        // crochets d'une séquence CSI ou OSC (changer le titre, effacer l'écran)
        // atteindraient le terminal.
        let url = "/?error=\u{1b}]0;titre\u{7}\u{1b}[2J\r\nacces_refuse";
        let Some(Rappel::Erreur(raison)) = classer_requete_locale(true, url) else {
            panic!("attendu une erreur reconnue");
        };
        assert!(
            raison.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')),
            "raison non assainie : {raison:?}"
        );
        assert!(raison.ends_with("acces_refuse"), "raison : {raison:?}");
    }

    #[test]
    fn la_raison_d_erreur_est_bornee_en_longueur() {
        // Échoue si la borne disparaissait : 500 caractères valides seraient recopiés.
        let url = format!("/?error={}", "a".repeat(500));
        assert_eq!(
            classer_requete_locale(true, &url),
            Some(Rappel::Erreur("a".repeat(LONGUEUR_MAX_RAISON)))
        );
    }

    #[test]
    fn l_attente_repond_404_aux_requetes_etrangeres_et_continue_jusqu_au_rappel() {
        // Vrai serveur `tiny_http` en boucle locale, vraies requêtes HTTP : ce test
        // prouve que `attendre_rappel` BRANCHE le classement, pas seulement qu'il est
        // juste. Échoue si la première requête reçue (`/favicon.ico`) était prise pour
        // le rappel, si elle ne recevait pas 404, ou si l'attente s'arrêtait là.
        let serveur = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let tiny_http::ListenAddr::IP(adresse) = serveur.server_addr();
        let base = format!("http://{adresse}");
        let navigateur = std::thread::spawn(move || {
            // Délai côté client : sans lui, une attente qui ne reprendrait PAS après le
            // 404 — le défaut même que ce test vise — laisserait la seconde requête sans
            // réponse, et ce test se figerait au lieu de rougir. Vérifié : avec la
            // reprise neutralisée, il rougit en 5 s.
            let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(5)).build();
            let statut_favicon = match agent.get(&format!("{base}/favicon.ico")).call() {
                Ok(r) => r.status(),
                Err(ureq::Error::Status(statut, _)) => statut,
                Err(e) => panic!("transport inattendu : {e}"),
            };
            let statut_rappel = match agent.get(&format!("{base}/?code=CODE-REEL")).call() {
                Ok(r) => r.status(),
                Err(e) => panic!("le rappel est resté sans réponse : {e}"),
            };
            (statut_favicon, statut_rappel)
        });

        let rappel = attendre_rappel(&serveur, Instant::now() + Duration::from_secs(10)).unwrap();
        let (statut_favicon, statut_rappel) = navigateur.join().unwrap();

        assert_eq!(statut_favicon, 404);
        assert_eq!(statut_rappel, 200);
        assert_eq!(rappel, Some(Rappel::Code("CODE-REEL".to_string())));
    }

    #[test]
    fn l_attente_rend_none_a_l_echeance() {
        let serveur = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let echeance = Instant::now() + Duration::from_millis(50);
        assert_eq!(attendre_rappel(&serveur, echeance).unwrap(), None);
    }

    #[test]
    fn l_url_de_depart_porte_le_port_et_l_empreinte() {
        // Échoue si le port ou l'empreinte n'apparaissent pas tels quels dans l'URL —
        // c'est ce que le site signe ensuite dans le `state`.
        // Échoue aussi si `natif=1` manque : le site signerait alors un state web
        // et ne renverrait jamais vers la boucle locale (essai réel, 19/09/2026).
        let u = url_de_depart(47821, &"a".repeat(64));
        assert!(u.contains("/api/auth/discord?"));
        let parametres = u.split_once('?').map_or("", |(_, p)| p);
        assert!(
            parametres.split('&').any(|p| p == "natif=1"),
            "sans `natif=1`, le site ignore le port et l'empreinte"
        );
        assert!(u.contains("port=47821"));
        assert!(u.contains(&format!("empreinte={}", "a".repeat(64))));
    }

    #[test]
    fn le_secret_ne_quitte_jamais_la_machine_avant_l_echange() {
        // Le site ne recoit que l'EMPREINTE au depart. Le secret lui-meme ne part
        // qu'au POST final. C'est ce qui rend un code intercepte inutilisable.
        let secret = secret_aleatoire();
        let u = url_de_depart(1234, &empreinte_du_secret(&secret));
        assert!(!u.contains(&secret));
    }

    #[test]
    fn secret_aleatoire_est_du_hex_de_32_octets_et_varie() {
        // Échoue si secret_aleatoire ne tirait pas 32 octets (longueur hex fausse),
        // ne les encodait pas en hexadécimal, ou rendait toujours la même valeur.
        let a = secret_aleatoire();
        let b = secret_aleatoire();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn l_empreinte_est_deterministe_et_ne_revele_pas_le_secret() {
        // Échoue si deux appels sur le même secret divergeaient (non déterministe),
        // si deux secrets distincts produisaient la même empreinte, ou si l'empreinte
        // reprenait le secret en clair.
        let secret = "secret-de-test";
        let empreinte = empreinte_du_secret(secret);
        assert_eq!(empreinte.len(), 64);
        assert!(empreinte.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(empreinte, empreinte_du_secret(secret));
        assert_ne!(empreinte, empreinte_du_secret("secret-different"));
        assert!(!empreinte.contains(secret));
    }
}
