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

use std::time::Duration;

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
pub fn url_de_depart(port: u16, empreinte: &str) -> String {
    let base = Config::depuis_env().base_url;
    format!("{base}/api/auth/discord?port={port}&empreinte={empreinte}")
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
/// jalon C1, faute d'appelant — voir le brief de cette tâche. `jeton_valide`,
/// plus bas, en est le premier appelant réel.
///
/// Si `coffre` ne contient aucun jeton (jamais connecté), rend `ErreurCompte::Refuse` :
/// du point de vue de l'appelant, l'absence de session à renouveler appelle la même
/// réaction qu'un renouvellement refusé par le serveur — se reconnecter via `connecter`.
pub fn renouveler(config: &Config, coffre: &Coffre) -> Result<Jetons, ErreurCompte> {
    let actuels = coffre.jetons()?.ok_or(ErreurCompte::Refuse)?;
    let client = ClientHttp::new(config);
    let corps = CorpsRefresh { refresh: &actuels.renouvellement };
    let reponse: ReponseRefresh = client.post_json("/api/auth/refresh", &corps, None)?;
    let jetons = Jetons { session: reponse.acces, renouvellement: actuels.renouvellement };
    coffre.ranger_jetons(&jetons)?;
    Ok(jetons)
}

/// Sonde `GET /api/sky/sync` avec `jeton` — seul le statut de la réponse compte, son
/// corps est ignoré : ce n'est pas une vraie synchronisation (tâche 8), seulement le
/// moyen de savoir si le serveur reconnaît encore ce jeton. `/api/sky/sync` est la
/// seule route authentifiée du double qui ne modifie rien côté serveur, donc la
/// sonder ne coûte aucun effet de bord.
fn jeton_est_accepte(config: &Config, jeton: &str) -> Result<(), ErreurCompte> {
    let client = ClientHttp::new(config);
    client.get_json::<serde_json::Value>("/api/sky/sync", Some(jeton))?;
    Ok(())
}

/// Rend un jeton de session dont le serveur vient tout juste de confirmer qu'il le
/// reconnaît, en renouvelant d'abord si nécessaire.
///
/// UNE SEULE reprise, jamais deux : si le jeton en coffre est refusé, `renouveler`
/// est appelé une fois, et le nouveau jeton est sondé une seule fois de plus. Un
/// second refus (jeton de renouvellement lui-même révoqué, ou nouveau jeton de
/// session encore refusé) remonte tel quel, sans nouvelle tentative — sans cette
/// borne, un jeton de renouvellement révoqué déclencherait une boucle infinie
/// d'appels au serveur.
pub fn jeton_valide(config: &Config, coffre: &Coffre) -> Result<String, ErreurCompte> {
    let jetons = coffre.jetons()?.ok_or(ErreurCompte::Refuse)?;

    match jeton_est_accepte(config, &jetons.session) {
        Ok(()) => Ok(jetons.session),
        Err(ErreurCompte::Refuse) => {
            let renouveles = renouveler(config, coffre)?;
            jeton_est_accepte(config, &renouveles.session)?;
            Ok(renouveles.session)
        }
        Err(autre) => Err(autre),
    }
}

/// Sépare `code` et `error` de la requête de redirection reçue par le serveur local
/// (`/?code=...` ou `/?error=...`).
fn extraire_code_ou_erreur(chemin: &str) -> (Option<String>, Option<String>) {
    let Some((_, requete)) = chemin.split_once('?') else {
        return (None, None);
    };
    let mut code = None;
    let mut erreur = None;
    for paire in requete.split('&') {
        if let Some(valeur) = paire.strip_prefix("code=") {
            code = Some(valeur.to_string());
        } else if let Some(valeur) = paire.strip_prefix("error=") {
            erreur = Some(valeur.to_string());
        }
    }
    (code, erreur)
}

/// Ouvre le navigateur par défaut sur `url` — le projet est Windows uniquement, `cmd
/// /C start` suffit sans dépendance supplémentaire.
fn ouvrir_navigateur(url: &str) -> Result<(), ErreurCompte> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
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
    // ferme l'onglet ou renonce laisserait ce thread bloqué pour toujours dans un
    // `recv()` sans délai — sans message, sans moyen d'en sortir. `recv_timeout` rend
    // `Ok(None)` à l'expiration plutôt que de bloquer indéfiniment ; une seule requête
    // traitée sinon, qu'elle porte un code ou une erreur — le serveur ferme aussitôt
    // après, dans les deux cas.
    let requete = serveur
        .recv_timeout(DELAI_ATTENTE_REDIRECTION)
        .map_err(|e| ErreurCompte::Reseau(e.to_string()))?;
    let Some(requete) = requete else {
        // Sortie propre : ferme le serveur local plutôt que de le laisser écouter
        // derrière. Le message ne nomme ni le code ni le secret — seulement qu'il
        // faut relancer.
        drop(serveur);
        return Err(ErreurCompte::Reseau(
            "aucune réponse reçue du site dans le délai imparti — rien n'est arrivé, relancez la connexion"
                .to_string(),
        ));
    };
    let (code, erreur) = extraire_code_ou_erreur(requete.url());
    let reponse = tiny_http::Response::from_string(PAGE_DE_RETOUR).with_header(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
            .expect("en-tete statique toujours valide"),
    );
    let _ = requete.respond(reponse);
    drop(serveur);

    if let Some(raison) = erreur {
        return Err(ErreurCompte::Protocole(format!("authentification refusee par le site : {raison}")));
    }
    let code = code.ok_or_else(|| {
        ErreurCompte::Protocole("redirection locale sans code ni erreur".to_string())
    })?;

    let jetons = echanger_le_code(config, &code, &secret)?;
    coffre.ranger_jetons(&jetons)?;
    Ok(jetons)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_url_de_depart_porte_le_port_et_l_empreinte() {
        // Échoue si le port ou l'empreinte n'apparaissent pas tels quels dans l'URL —
        // c'est ce que le site signe ensuite dans le `state`.
        let u = url_de_depart(47821, &"a".repeat(64));
        assert!(u.contains("/api/auth/discord?"));
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
