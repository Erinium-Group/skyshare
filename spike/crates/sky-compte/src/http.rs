//! Configuration et client HTTP synchrone vers l'API du site.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::erreur::ErreurCompte;

/// Serveur de production, utilisé quand `SKY_API_URL` est absente.
const BASE_URL_PAR_DEFAUT: &str = "https://eriniumgroup.vercel.app";

/// Neon se suspend après 5 minutes d'inactivité. Le réveil a été mesuré à
/// 748,8 ms contre ~35 ms à chaud. Un délai de 500 ms transformerait un
/// réveil NORMAL en panne. Ne pas « optimiser » cette valeur.
const DELAI: Duration = Duration::from_secs(5);

/// Configuration du client : seulement l'URL de base de l'API.
#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
}

impl Config {
    /// Lit `SKY_API_URL`, ou retombe sur le serveur de production si la
    /// variable est absente ou invalide.
    pub fn depuis_env() -> Config {
        let base_url = std::env::var("SKY_API_URL").unwrap_or_else(|_| BASE_URL_PAR_DEFAUT.to_string());
        Config { base_url }
    }

    /// Construit une configuration visant `url` explicitement — c'est ce
    /// que les tests des tâches suivantes utilisent pour viser le serveur
    /// double.
    pub fn vers(url: &str) -> Config {
        Config { base_url: url.to_string() }
    }
}

/// Client HTTP synchrone vers l'API du site — pas de runtime asynchrone,
/// `ureq` bloque le thread appelant le temps de la requête.
pub struct ClientHttp {
    agent: ureq::Agent,
    base_url: String,
}

impl ClientHttp {
    /// Construit un client dont le délai d'attente est fixé une fois pour
    /// toutes à `DELAI` — voir sa documentation pour la raison de cette
    /// valeur.
    pub fn new(config: &Config) -> ClientHttp {
        let agent = ureq::AgentBuilder::new().timeout(DELAI).build();
        ClientHttp { agent, base_url: config.base_url.clone() }
    }

    fn url(&self, chemin: &str) -> String {
        format!("{}{}", self.base_url, chemin)
    }

    fn avec_jeton(requete: ureq::Request, jeton: Option<&str>) -> ureq::Request {
        match jeton {
            Some(j) => requete.set("Authorization", &format!("Bearer {j}")),
            None => requete,
        }
    }

    /// Requête `GET` désérialisée en `T`. `jeton`, s'il est fourni, est posé
    /// dans l'en-tête `Authorization` — jamais dans le chemin ni dans un
    /// message d'erreur.
    pub fn get_json<T: DeserializeOwned>(&self, chemin: &str, jeton: Option<&str>) -> Result<T, ErreurCompte> {
        let requete = Self::avec_jeton(self.agent.get(&self.url(chemin)), jeton);
        Self::traiter_reponse(requete.call())
    }

    /// Requête `POST` avec un corps JSON sérialisé depuis `corps`, réponse
    /// désérialisée en `T`. Même règle que `get_json` pour `jeton`.
    pub fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        chemin: &str,
        corps: &B,
        jeton: Option<&str>,
    ) -> Result<T, ErreurCompte> {
        let requete = Self::avec_jeton(self.agent.post(&self.url(chemin)), jeton);
        Self::traiter_reponse(requete.send_json(corps))
    }

    /// Traduit une réponse `ureq` en `Result<T, ErreurCompte>`. Le corps
    /// d'une réponse en erreur peut contenir un message serveur (jamais un
    /// jeton — le jeton ne voyage que dans l'en-tête `Authorization`, que
    /// `ureq` ne renvoie pas dans l'erreur), donc `depuis_statut` peut le
    /// lire sans risque.
    fn traiter_reponse<T: DeserializeOwned>(reponse: Result<ureq::Response, ureq::Error>) -> Result<T, ErreurCompte> {
        match reponse {
            Ok(rep) => rep.into_json::<T>().map_err(|e| ErreurCompte::Protocole(e.to_string())),
            Err(ureq::Error::Status(statut, rep)) => {
                let corps = rep.into_string().unwrap_or_default();
                Err(ErreurCompte::depuis_statut(statut, &corps))
            }
            Err(ureq::Error::Transport(transport)) => Err(ErreurCompte::Reseau(transport.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn aucun_jeton_dans_lechec_reseau_reel() {
        // Passe par le vrai chemin ClientHttp -> traiter_reponse, contrairement au
        // test manuel d'erreur.rs. Le port 1 en local refuse la connexion tout de
        // suite (échec de transport réel, pas de délai de 5 s à attendre) : ce test
        // rougirait si get_json se mettait un jour à glisser `jeton` dans le message
        // d'erreur — voir ronde de correction 1.
        let client = ClientHttp::new(&Config::vers("http://127.0.0.1:1"));
        let r: Result<Value, ErreurCompte> = client.get_json("/x", Some("SENTINEL-JETON"));
        assert!(!r.unwrap_err().to_string().contains("SENTINEL-JETON"));
    }
}
