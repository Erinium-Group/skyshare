//! Les instantanés envoyés à l'interface : le SEUL contrat entre le cœur et
//! `app/src/types.ts`. Aucun jeton, aucune clé, aucune adresse (spec §3,
//! frontière D5) : l'interface affiche, elle ne détient rien.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Connexion {
    Deconnecte,
    EnCours,
    Connecte,
    SessionExpiree,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AmiVue {
    pub id: i64,
    pub friendship_id: i64,
    pub nom: String,
    pub appareils: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandeVue {
    pub friendship_id: i64,
    pub nom: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListeVue {
    pub id: i64,
    pub nom: String,
    pub couleur: Option<String>,
    pub emoji: Option<String>,
    pub membres: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppareilVue {
    pub id: i64,
    pub nom: String,
    /// Cet appareil-ci : jamais révocable depuis l'application.
    pub courant: bool,
    pub revoque: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EcranVue {
    /// Rang dans l'énumération de Windows — celui qu'attend `WgcCapture::new`.
    pub index: usize,
    pub nom: String,
    pub principal: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "cause", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum FinVue {
    Arrete,
    PasEnPartage { ami: String },
    ReseauBloque,
    TropLente,
    SessionExpiree,
    AucuneDemande,
    Autre { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "etat", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum PartageVue {
    Inactif,
    /// Hôte disponible : une demande sera honorée jusqu'à `debut + fenetre`.
    Disponible { debut_ms: u64, fenetre_s: u64, ecran: usize },
    /// Hôte : un ami regarde (ou se connecte).
    Diffuse { spectateur: Option<String>, depuis_ms: u64, debit_mbps: f64, rtt_ms: f64, ecran: usize },
    /// Spectateur : demande envoyée, réponse attendue.
    Demande { ami: String, debut_ms: u64 },
    /// Spectateur : connecté, flux mesuré puis jeté (spec D2).
    Regarde {
        ami: String,
        connecte_en_s: f64,
        debit_mbps: f64,
        images_par_s: u64,
        gigue_ms: f64,
        depuis_ms: u64,
    },
    Termine { fin: FinVue },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Instantane {
    pub connexion: Connexion,
    pub nom: Option<String>,
    pub code: Option<String>,
    pub amis: Vec<AmiVue>,
    pub demandes: Vec<DemandeVue>,
    pub listes: Vec<ListeVue>,
    pub appareils: Vec<AppareilVue>,
    pub partage: PartageVue,
    /// Carte NVIDIA utilisable : sans elle, « Partager » est désactivé.
    pub nvenc: bool,
    pub ecrans: Vec<EcranVue>,
    pub demarrage_automatique: bool,
}

impl Instantane {
    pub fn vide(connexion: Connexion) -> Instantane {
        Instantane {
            connexion,
            nom: None,
            code: None,
            amis: Vec::new(),
            demandes: Vec::new(),
            listes: Vec::new(),
            appareils: Vec::new(),
            partage: PartageVue::Inactif,
            nvenc: false,
            ecrans: Vec::new(),
            demarrage_automatique: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn le_partage_est_serialise_sous_les_noms_que_lit_l_interface() {
        // Contrat avec app/src/types.ts. Neutralisation : retirer
        // `rename_all_fields` — `connecteEnS` devient `connecte_en_s`.
        let v = serde_json::to_value(PartageVue::Regarde {
            ami: "bob".into(),
            connecte_en_s: 0.5,
            debit_mbps: 12.0,
            images_par_s: 60,
            gigue_ms: 5.0,
            depuis_ms: 1,
        })
        .unwrap();
        assert_eq!(
            v,
            json!({"etat": "regarde", "ami": "bob", "connecteEnS": 0.5, "debitMbps": 12.0,
                   "imagesParS": 60, "gigueMs": 5.0, "depuisMs": 1})
        );
        let fin =
            serde_json::to_value(PartageVue::Termine { fin: FinVue::PasEnPartage { ami: "bob".into() } })
                .unwrap();
        assert_eq!(fin, json!({"etat": "termine", "fin": {"cause": "pas_en_partage", "ami": "bob"}}));
    }

    #[test]
    fn l_instantane_est_en_camel_case_et_la_connexion_en_snake_case() {
        let v = serde_json::to_value(Instantane::vide(Connexion::SessionExpiree)).unwrap();
        assert_eq!(v["connexion"], "session_expiree");
        assert_eq!(v["demarrageAutomatique"], false);
        assert_eq!(v["partage"], json!({"etat": "inactif"}));
    }
}
