//! Type d'erreur unique du crate `sky-compte`.

use std::fmt;

/// Erreur renvoyée par les opérations de `sky-compte`.
///
/// Aucune variante ne doit jamais porter un jeton, même partiellement : un
/// jeton dans un message d'erreur finit dans un rapport de bug, un log, ou
/// une trace. Les appelants qui construisent ces variantes doivent s'en
/// tenir à des informations qui ne dépendent pas du secret transporté par
/// la requête (chemin, code d'état, corps de la réponse serveur).
#[derive(Debug)]
pub enum ErreurCompte {
    /// Échec de transport : la requête n'a pas atteint le serveur, ou la
    /// réponse n'a pas pu être reçue (DNS, connexion, délai dépassé).
    Reseau(String),
    /// Le serveur a répondu 401. Il répond « Code refusé » pour quatre
    /// causes distinctes et volontairement indiscernables : code inconnu,
    /// code expiré, code déjà consommé, ou mauvais secret. Distinguer ces
    /// cas côté client apprendrait à un attaquant qu'il détient un code
    /// valide et qu'il ne lui manque que le secret — cette variante ne
    /// porte donc aucun détail, à dessein.
    Refuse,
    /// La réponse ne correspond pas au contrat attendu : JSON illisible,
    /// champ manquant, ou code d'état ni 2xx ni 401.
    Protocole(String),
    /// Échec du coffre local (stockage du jeton via `keyring`, etc.) —
    /// destiné aux tâches suivantes de ce jalon.
    Coffre(String),
}

impl ErreurCompte {
    /// Construit une erreur à partir du code d'état HTTP et du corps de la
    /// réponse.
    ///
    /// Un statut 401 devient systématiquement `Refuse`, sans inspecter
    /// `corps` : le serveur a délibérément rendu les quatre causes de refus
    /// indiscernables, le client ne doit pas les redistinguer à partir du
    /// message renvoyé.
    pub fn depuis_statut(statut: u16, corps: &str) -> ErreurCompte {
        if statut == 401 {
            return ErreurCompte::Refuse;
        }
        ErreurCompte::Protocole(format!("statut {statut} inattendu : {}", corps_sans_en_tete(corps)))
    }
}

/// Retire toute valeur qui suit `Bearer` dans `corps` avant de l'inclure dans un
/// message d'erreur.
///
/// Rien ne prouve aujourd'hui qu'un en-tête `Authorization` puisse se retrouver
/// échoté dans un corps de réponse — mais si un intermédiaire (proxy, page
/// d'erreur générique, etc.) le faisait un jour, ce filtre l'empêche d'atterrir
/// tel quel dans nos messages d'erreur et donc dans un rapport de bug.
fn corps_sans_en_tete(corps: &str) -> std::borrow::Cow<'_, str> {
    match corps.find("Bearer ") {
        None => std::borrow::Cow::Borrowed(corps),
        Some(debut) => {
            let apres_prefixe = debut + "Bearer ".len();
            let fin_valeur = corps[apres_prefixe..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .map(|i| apres_prefixe + i)
                .unwrap_or(corps.len());
            std::borrow::Cow::Owned(format!(
                "{}[reduit]{}",
                &corps[..apres_prefixe],
                &corps[fin_valeur..]
            ))
        }
    }
}

impl fmt::Display for ErreurCompte {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErreurCompte::Reseau(detail) => write!(f, "erreur réseau : {detail}"),
            ErreurCompte::Refuse => write!(f, "identifiants refusés"),
            ErreurCompte::Protocole(detail) => write!(f, "réponse inattendue du serveur : {detail}"),
            ErreurCompte::Coffre(detail) => write!(f, "échec du coffre local : {detail}"),
        }
    }
}

impl std::error::Error for ErreurCompte {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_401_devient_refuse_sans_detail() {
        // Le serveur répond « Code refusé » pour quatre causes différentes,
        // volontairement indistinctes. Le client ne doit pas réintroduire la
        // distinction que le serveur a refusé de faire.
        let erreur = ErreurCompte::depuis_statut(401, "{\"error\":\"Code refusé\"}");
        assert!(matches!(erreur, ErreurCompte::Refuse));
        assert_eq!(erreur.to_string(), "identifiants refusés");
    }

    #[test]
    fn aucun_jeton_dans_le_message_d_erreur() {
        // Un jeton dans un message d'erreur finit dans un rapport de bug.
        let erreur = ErreurCompte::Reseau("échec vers /api/sky/sync".into());
        assert!(!erreur.to_string().contains("Bearer"));
    }

    #[test]
    fn un_en_tete_autorisation_echote_dans_le_corps_est_reduit() {
        // Si un intermediaire echote un jour l'en-tete Authorization dans un
        // corps de reponse d'erreur, corps_sans_en_tete doit en retirer la
        // valeur avant qu'elle n'atteigne ErreurCompte::Protocole.
        let erreur = ErreurCompte::depuis_statut(500, "trace amont: Authorization: Bearer SECRET-XYZ, requete rejetee");
        assert!(!erreur.to_string().contains("SECRET-XYZ"));
        assert!(erreur.to_string().contains("[reduit]"));
    }
}
