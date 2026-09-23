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

/// LE CONTRAT DE FORME AVEC `app/src/types.ts`, comparé champ par champ.
///
/// EXIGENCE DU CONTRÔLEUR (tâche 10). L'essai réel du jalon n'a pas eu lieu : le
/// chemin qui relie ce module à l'interface n'a JAMAIS été exécuté. Les deux
/// tests ci-dessus figent une poignée de noms choisis à la main — ils n'auraient
/// pas vu un champ ajouté d'un seul côté, ni un `friendshipId` devenu
/// `friendship_id` dans une structure qu'ils ne citent pas.
///
/// Celui-ci compare des ENSEMBLES : tous les noms de champs émis par serde,
/// contre tous ceux déclarés dans `types.ts` ; toutes les étiquettes de variante
/// émises, contre toutes celles déclarées. Il rougit si l'un des deux côtés
/// change sans l'autre, dans les DEUX sens.
///
/// CE QU'IL NE COUVRE PAS, et qu'il ne remplace pas :
/// - les TYPES des champs (`number` contre `string`) : seuls les NOMS sont
///   comparés ; `serde_json` ne distingue pas non plus `i64` de `usize`.
/// - le fait qu'un champ soit optionnel en TypeScript (`?`) : le `?` est
///   retiré avant comparaison. La NULLABILITÉ, elle, est vérifiée à part
///   ci-dessous — serde émet `null`, jamais une clé absente.
/// - le rattachement d'un champ à SA structure : `nom` existe dans cinq
///   structures, et l'ensemble ne le dit qu'une fois. Un champ déplacé d'une
///   structure à une autre passerait.
/// - tout le reste du chemin : que Tauri émette bien l'événement, que
///   l'interface le reçoive, que l'écran s'affiche. SEUL un essai réel le dira.
#[cfg(test)]
mod contrat_typescript {
    use super::*;
    use std::collections::BTreeSet;

    /// `include_str!` et non une lecture de fichier : la dépendance devient une
    /// dépendance de COMPILATION. Modifier `types.ts` recompile ce test.
    const TYPES_TS: &str = include_str!("../../../../app/src/types.ts");

    /// Tout ce que l'interface peut recevoir, en un seul jeu : un instantané
    /// complet, plus chaque variante de `PartageVue` et de `FinVue`, dont une
    /// seule apparaît à la fois dans un instantané.
    fn tout_ce_qui_est_emis() -> Vec<serde_json::Value> {
        let instantane = Instantane {
            connexion: Connexion::Connecte,
            nom: Some("BOB".to_string()),
            code: Some("ABCD2345".to_string()),
            amis: vec![AmiVue {
                id: 1,
                friendship_id: 11,
                nom: "Alice".to_string(),
                appareils: 1,
            }],
            demandes: vec![DemandeVue { friendship_id: 12, nom: "Bob".to_string() }],
            listes: vec![ListeVue {
                id: 7,
                nom: "Jeu".to_string(),
                couleur: Some("#C4664A".to_string()),
                emoji: Some("🎮".to_string()),
                membres: vec![1],
            }],
            appareils: vec![AppareilVue {
                id: 4,
                nom: "BUREAU".to_string(),
                courant: true,
                revoque: false,
            }],
            partage: PartageVue::Inactif,
            nvenc: true,
            ecrans: vec![EcranVue { index: 0, nom: "Écran 1".to_string(), principal: true }],
            demarrage_automatique: true,
        };
        let partages = [
            PartageVue::Inactif,
            PartageVue::Disponible { debut_ms: 1, fenetre_s: 1800, ecran: 0 },
            PartageVue::Diffuse {
                spectateur: Some("Bob".to_string()),
                depuis_ms: 1,
                debit_mbps: 12.0,
                rtt_ms: 15.0,
                ecran: 0,
            },
            PartageVue::Demande { ami: "Bob".to_string(), debut_ms: 1 },
            PartageVue::Regarde {
                ami: "Bob".to_string(),
                connecte_en_s: 0.5,
                debit_mbps: 12.0,
                images_par_s: 60,
                gigue_ms: 5.0,
                depuis_ms: 1,
            },
            PartageVue::Termine { fin: FinVue::Arrete },
        ];
        let fins = [
            FinVue::Arrete,
            FinVue::PasEnPartage { ami: "Bob".to_string() },
            FinVue::ReseauBloque,
            FinVue::TropLente,
            FinVue::SessionExpiree,
            FinVue::AucuneDemande,
            FinVue::Autre { message: "quelque chose".to_string() },
        ];
        let connexions = [
            Connexion::Deconnecte,
            Connexion::EnCours,
            Connexion::Connecte,
            Connexion::SessionExpiree,
        ];
        let mut valeurs = vec![serde_json::to_value(&instantane).unwrap()];
        valeurs.extend(partages.iter().map(|p| serde_json::to_value(p).unwrap()));
        valeurs.extend(fins.iter().map(|f| serde_json::to_value(f).unwrap()));
        // Une `Connexion` seule se sérialise en chaîne : elle est portée par un
        // instantané, pour que la clé `connexion` et sa valeur soient vues.
        valeurs.extend(
            connexions.iter().map(|c| serde_json::to_value(Instantane::vide(*c)).unwrap()),
        );
        valeurs
    }

    fn recolter(valeur: &serde_json::Value, cles: &mut BTreeSet<String>, tags: &mut BTreeSet<String>) {
        match valeur {
            serde_json::Value::Object(objet) => {
                for (cle, sous) in objet {
                    cles.insert(cle.clone());
                    // Les trois clés qui portent une étiquette de variante :
                    // `serde(tag = ...)` pour `partage` et `fin`, et l'énumération
                    // simple `Connexion`.
                    if matches!(cle.as_str(), "etat" | "cause" | "connexion") {
                        if let serde_json::Value::String(tag) = sous {
                            tags.insert(tag.clone());
                        }
                    }
                    recolter(sous, cles, tags);
                }
            }
            serde_json::Value::Array(elements) => {
                for element in elements {
                    recolter(element, cles, tags);
                }
            }
            _ => {}
        }
    }

    /// Retire `//…` et `/*…*/` : les commentaires de `types.ts` contiennent des
    /// deux-points et des identifiants qui passeraient pour des champs.
    fn sans_commentaires(source: &str) -> String {
        let mut net = String::with_capacity(source.len());
        let mut reste = source;
        loop {
            let ligne = reste.find("//");
            let bloc = reste.find("/*");
            // Le commentaire qui commence le PLUS TÔT gagne : un `//` dans un
            // bloc `/* */` n'en ouvre pas un second, et réciproquement.
            enum Ouverture {
                Ligne(usize),
                Bloc(usize),
            }
            let ouverture = match (ligne, bloc) {
                (None, None) => {
                    net.push_str(reste);
                    return net;
                }
                (Some(l), None) => Ouverture::Ligne(l),
                (None, Some(b)) => Ouverture::Bloc(b),
                (Some(l), Some(b)) if l < b => Ouverture::Ligne(l),
                (Some(_), Some(b)) => Ouverture::Bloc(b),
            };
            match ouverture {
                Ouverture::Ligne(l) => {
                    net.push_str(&reste[..l]);
                    reste = match reste[l..].find('\n') {
                        Some(fin) => &reste[l + fin..],
                        None => return net,
                    };
                }
                Ouverture::Bloc(b) => {
                    net.push_str(&reste[..b]);
                    reste = match reste[b + 2..].find("*/") {
                        Some(fin) => &reste[b + 2 + fin + 2..],
                        None => return net,
                    };
                }
            }
        }
    }

    /// Les noms de propriétés déclarés dans `types.ts` : tout ce qui précède un
    /// `:` dans un morceau délimité par `{ } ; , |` ou une fin de ligne. Le `?`
    /// d'un champ optionnel est retiré.
    fn champs_declares(source: &str) -> BTreeSet<String> {
        let net = sans_commentaires(source);
        let mut champs = BTreeSet::new();
        for morceau in net.split(['{', '}', ';', ',', '|', '\n']) {
            let Some((avant, _)) = morceau.split_once(':') else { continue };
            let nom = avant.trim().trim_end_matches('?');
            let identifiant = !nom.is_empty()
                && nom.starts_with(|c: char| c.is_ascii_alphabetic())
                && nom.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if identifiant {
                champs.insert(nom.to_string());
            }
        }
        champs
    }

    /// Les littéraux de chaîne de `types.ts` : dans ce fichier, ce sont
    /// exactement les étiquettes de variante.
    fn etiquettes_declarees(source: &str) -> BTreeSet<String> {
        let net = sans_commentaires(source);
        net.split('"').skip(1).step_by(2).map(str::to_string).collect()
    }

    #[test]
    fn chaque_champ_emis_est_declare_dans_types_ts_et_reciproquement() {
        // Neutralisation : renommer un champ d'`Instantane` (par exemple
        // `nvenc` en `nvenc2`) sans toucher `types.ts` — ce test rougit en
        // nommant le champ absent de chaque côté. Et dans l'autre sens :
        // ajouter une propriété à une interface de `types.ts` sans l'ajouter
        // ici — il rougit aussi.
        let mut emis = BTreeSet::new();
        let mut etiquettes_emises = BTreeSet::new();
        for valeur in tout_ce_qui_est_emis() {
            recolter(&valeur, &mut emis, &mut etiquettes_emises);
        }
        let declares = champs_declares(TYPES_TS);

        // CONTRÔLE POSITIF : sans lui, deux ensembles VIDES seraient égaux, et
        // un `types.ts` illisible par cette extraction rendrait le test muet.
        assert!(
            declares.len() >= 30,
            "l'extraction de types.ts n'a trouvé que {} champs : elle ne lit plus le fichier",
            declares.len()
        );

        assert_eq!(
            emis.difference(&declares).collect::<Vec<_>>(),
            Vec::<&String>::new(),
            "des champs sont émis par le cœur sans être déclarés dans app/src/types.ts"
        );
        assert_eq!(
            declares.difference(&emis).collect::<Vec<_>>(),
            Vec::<&String>::new(),
            "des champs sont déclarés dans app/src/types.ts sans être émis par le cœur"
        );

        let etiquettes = etiquettes_declarees(TYPES_TS);
        assert_eq!(
            etiquettes_emises, etiquettes,
            "les variantes de Connexion, PartageVue et FinVue ne correspondent plus"
        );
    }

    #[test]
    fn un_champ_absent_est_emis_a_null_jamais_omis() {
        // `nom: string | null` et non `nom?: string` : l'interface lit
        // `instantane.nom ?? "…"`, ce qui marche dans les deux cas — mais
        // `couleur` et `emoji` d'une liste sont RENVOYÉS tels quels au site à
        // l'enregistrement, et une clé absente y deviendrait « champ inchangé »
        // au lieu de « remise à rien ». Neutralisation : poser
        // `#[serde(skip_serializing_if = "Option::is_none")]` sur ces champs —
        // ce test rougit.
        // `valeur["cle"]` NE CONVIENT PAS ICI : indexer un objet JSON par une
        // clé ABSENTE rend `Null`, exactement comme une clé présente à `null`.
        // Mesuré : avec cette écriture-là, poser
        // `skip_serializing_if = "Option::is_none"` sur `couleur` laissait le
        // test VERT. C'est la classe de défaut que `CLAUDE.md` disqualifie — un
        // test qui passerait aussi bien dans le cas négatif. La présence de la
        // clé se demande à `get`.
        fn present_et_nul(valeur: &serde_json::Value, cle: &str) {
            let objet = valeur.as_object().expect("un objet JSON");
            assert_eq!(
                objet.get(cle),
                Some(&serde_json::Value::Null),
                "la clé « {cle} » doit être PRÉSENTE et valoir null, pas absente"
            );
        }

        let vide = serde_json::to_value(Instantane::vide(Connexion::Deconnecte)).unwrap();
        present_et_nul(&vide, "nom");
        present_et_nul(&vide, "code");
        let liste = serde_json::to_value(ListeVue {
            id: 1,
            nom: "Jeu".to_string(),
            couleur: None,
            emoji: None,
            membres: Vec::new(),
        })
        .unwrap();
        present_et_nul(&liste, "couleur");
        present_et_nul(&liste, "emoji");
        let diffuse = serde_json::to_value(PartageVue::Diffuse {
            spectateur: None,
            depuis_ms: 1,
            debit_mbps: 1.0,
            rtt_ms: 1.0,
            ecran: 0,
        })
        .unwrap();
        present_et_nul(&diffuse, "spectateur");
    }
}
