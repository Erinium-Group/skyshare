//! Sous-commandes de compte : connexion, appareils, amis, code ami.
//!
//! Chaque fonction `pub fn` de ce module correspond à une sous-commande de
//! `main.rs` et ne fait qu'afficher le résultat d'un appel à `sky-compte` —
//! aucune logique métier ici, elle vit dans la bibliothèque. Les fonctions
//! PURES (validation locale, mise en forme des messages) sont testées en
//! unité dans ce fichier ; les fonctions de `sky-compte` elles-mêmes sont
//! testées côté bibliothèque, contre le serveur double.
//!
//! LIMITE CONNUE, répétée dans l'aide de chaque sous-commande qui
//! synchronise sans avoir l'usage des enveloppes reçues (`device list`,
//! `friends list`, `friends add`, `code`) : le serveur EFFACE les
//! enveloppes qu'il livre (voir la documentation de
//! `sky_compte::synchroniser`), et toute synchronisation en livre
//! systématiquement toutes celles qui attendent. Lancer l'une de ces
//! commandes pendant une négociation en cours détruirait donc l'offre en
//! attente, pour l'appelant comme pour son correspondant.

use sky_compte::{
    accepter_ami, ajouter_ami, enregistrer_appareil, moi, normaliser_code_ami, synchroniser,
    Acceptation, Appareil, AjoutAmi, Coffre, Config, ErreurCompte, Jetons,
};

/// Construit la configuration et le coffre de production — factorisé pour
/// que chaque sous-commande n'ait qu'à l'appeler une fois, sans dupliquer
/// `Config::depuis_env()` / `Coffre::nouveau()` dans `main.rs`.
pub fn config_et_coffre() -> anyhow::Result<(Config, Coffre)> {
    Ok((Config::depuis_env(), Coffre::nouveau()?))
}

/// Traduit une erreur de `sky-compte` en message affichable.
///
/// `ErreurCompte::Refuse` reçoit un message UNIQUE, quelle que soit la
/// cause réelle du 401 : le serveur rend délibérément indiscernables quatre
/// causes de refus (voir sa documentation), et cet affichage ne doit
/// jamais réintroduire une distinction que le serveur a refusé de faire.
/// Les autres variantes délèguent à `ErreurCompte::Display`, qui rédige
/// déjà tout contenu potentiellement sensible (`corps_sans_en_tete`).
pub fn message_utilisateur(erreur: &ErreurCompte) -> String {
    match erreur {
        ErreurCompte::Refuse => "Connexion refusée. Relance `sky-probe login`.".to_string(),
        autre => autre.to_string(),
    }
}

/// Résumé affiché après une connexion réussie.
///
/// Ne reprend JAMAIS le contenu de `jetons` — un jeton affiché dans un
/// terminal (historique de commandes, capture d'écran, terminal partagé)
/// vaut une session volée. Le paramètre existe pour que la signature reste
/// celle attendue par les futurs appelants qui voudraient un jour y lire
/// autre chose que le contenu secret (p. ex. une date d'expiration) — pas
/// pour l'afficher aujourd'hui.
pub fn resume_de_connexion(_jetons: &Jetons, nom: &str) -> String {
    format!("Connecté en tant que {nom}.")
}

/// `login` — connexion native puis lecture du nom affiché.
///
/// Si `moi` échoue APRÈS une connexion réussie, la commande dit que la
/// connexion a réussi sans pouvoir afficher le nom, plutôt que de laisser
/// croire à un échec de connexion (arbitrage T10) : les jetons sont déjà
/// rangés dans le coffre à ce stade, par `connecter`.
pub fn login(config: &Config, coffre: &Coffre) -> anyhow::Result<()> {
    let jetons = match sky_compte::connecter(config, coffre) {
        Ok(j) => j,
        Err(e) => {
            println!("{}", message_utilisateur(&e));
            return Ok(());
        }
    };
    match moi(config, coffre) {
        Ok(m) => println!("{}", resume_de_connexion(&jetons, &m.discord_name)),
        Err(_) => println!("Connexion réussie, mais impossible d'afficher le nom du compte."),
    }
    Ok(())
}

/// Vérifie s'il faut refuser l'enregistrement d'un nouvel appareil.
///
/// Fonction pure : si un appareil est déjà enregistré et le drapeau `force`
/// est absent, renvoie le message de refus. Sinon renvoie `None`.
fn message_si_appareil_deja_enregistre(id_appareil: i64, force: bool) -> Option<String> {
    if !force {
        Some(
            format!(
                "Un appareil est déjà enregistré sur cette machine (identifiant {id_appareil}). \
                 Relance avec --force pour en enregistrer un nouveau."
            )
        )
    } else {
        None
    }
}

/// `device register` — refuse par défaut si un appareil est déjà enregistré
/// sur cette machine (arbitrage T10) : chaque enregistrement crée un
/// NOUVEL appareil côté serveur, et chaque appareil superflu reçoit ensuite
/// les offres que `deposer` scelle pour tous les appareils d'un ami.
/// `force` contourne ce refus.
pub fn device_register(config: &Config, coffre: &Coffre, nom: &str, force: bool) -> anyhow::Result<()> {
    if !force {
        match coffre.identifiant_appareil() {
            Ok(Some(id)) => {
                if let Some(msg) = message_si_appareil_deja_enregistre(id, force) {
                    println!("{msg}");
                }
                return Ok(());
            }
            Ok(None) => {}
            Err(e) => {
                println!("{}", message_utilisateur(&e));
                return Ok(());
            }
        }
    }

    let identite = match coffre.identite() {
        Ok(i) => i,
        Err(e) => {
            println!("{}", message_utilisateur(&e));
            return Ok(());
        }
    };
    let cle = identite.public_key();

    match enregistrer_appareil(config, coffre, nom, &cle) {
        Ok(id) => println!("Appareil « {nom} » enregistré (identifiant {id})."),
        Err(e) => println!("{}", message_utilisateur(&e)),
    }
    Ok(())
}

/// Formate la liste des appareils avec marquage de l'appareil courant.
///
/// Fonction pure : prend une liste d'appareils et l'identifiant du courant
/// (ou `None`), rend les lignes prêtes à afficher avec le marquage visuel.
fn formater_appareils_avec_marque(appareils: &[Appareil], courant: Option<i64>) -> Vec<String> {
    appareils
        .iter()
        .map(|a| {
            let marque = if Some(a.id) == courant { " (cet appareil)" } else { "" };
            format!("- [{}] {} ({}){marque}", a.id, a.nom, a.plateforme)
        })
        .collect()
}

/// `device list` — synchronise puis affiche les appareils enregistrés,
/// en signalant l'appareil courant (voir `Coffre::identifiant_appareil`).
///
/// /!\ SYNCHRONISE — voir la limite connue en tête de ce module.
pub fn device_list(config: &Config, coffre: &Coffre) -> anyhow::Result<()> {
    let etat = match synchroniser(config, coffre, None) {
        Ok(e) => e,
        Err(e) => {
            println!("{}", message_utilisateur(&e));
            return Ok(());
        }
    };

    if etat.appareils.is_empty() {
        println!("Aucun appareil enregistré.");
        return Ok(());
    }

    // Une erreur de lecture du coffre ne doit pas empêcher d'afficher la
    // liste : elle dégrade seulement en « aucun appareil signalé courant ».
    let courant = coffre.identifiant_appareil().unwrap_or(None);

    println!("Appareils enregistrés :");
    let lignes = formater_appareils_avec_marque(&etat.appareils, courant);
    for ligne in lignes {
        println!("{ligne}");
    }
    Ok(())
}

/// Vérifie qu'un identifiant d'amitié a la forme attendue par le serveur —
/// même contrôle que `POST /api/sky/friends/{id}/accept`
/// (`Number.isInteger(x) && x > 0`) — AVANT tout appel réseau.
fn identifiant_amitie_valide(id: i64) -> bool {
    id > 0
}

/// `friends accept <id>` — vérifie localement la forme de l'identifiant
/// avant tout appel réseau (arbitrage T10).
pub fn friends_accept(config: &Config, coffre: &Coffre, id: i64) -> anyhow::Result<()> {
    if !identifiant_amitie_valide(id) {
        println!("Identifiant invalide : attendu un entier strictement positif.");
        return Ok(());
    }

    match accepter_ami(config, coffre, id) {
        Ok(Acceptation::Acceptee) => println!("Demande d'ami acceptée."),
        Ok(Acceptation::Introuvable) => println!("Amitié introuvable."),
        Err(e) => println!("{}", message_utilisateur(&e)),
    }
    Ok(())
}

/// Normalise et vérifie la forme d'un code ami saisi par l'utilisateur,
/// SANS appel réseau — un code mal formé ne doit jamais atteindre le
/// serveur (arbitrage T10, reporté depuis la revue de la tâche 8).
fn code_normalise_ou_message(brut: &str) -> Result<String, String> {
    normaliser_code_ami(brut)
        .ok_or_else(|| "Code ami invalide : attendu huit caractères (ex. SKY-ABCD-EFGH).".to_string())
}

/// Message si `code` (déjà normalisé) désigne le compte de l'appelant
/// lui-même — comparaison avec `Etat.code`, JAMAIS en lisant le texte d'un
/// 400 : le serveur ne distingue pas ce cas par un statut dédié, et
/// l'analyser par chaîne est l'anti-pattern documenté dans `CLAUDE.md`
/// (« Validation des entrées »).
fn message_si_propre_code(code: &str, mon_code: &str) -> Option<String> {
    if code == mon_code {
        Some("C'est ton propre code ami : impossible de s'ajouter soi-même.".to_string())
    } else {
        None
    }
}

/// Message affiché quand AjoutAmi::CodeIntrouvable est renvoyé.
///
/// Couvre deux cas : un code inexistant, et un utilisateur qui a bloqué
/// l'appelant. Le serveur refuse délibérément de les distinguer pour
/// protéger la vie privée du bloqueur. Le message DOIT reflter cette
/// indistinction et ne JAMAIS suggérer un blocage.
fn message_code_introuvable() -> String {
    "Aucun compte ne correspond à ce code ami.".to_string()
}

/// `friends add <code>` — vérifie la forme localement, refuse son propre
/// code par comparaison à `Etat.code` (jamais par lecture d'un 400), puis
/// envoie la demande.
///
/// /!\ SYNCHRONISE (pour connaître son propre code) — voir la limite
/// connue en tête de ce module.
pub fn friends_add(config: &Config, coffre: &Coffre, code_brut: &str) -> anyhow::Result<()> {
    let code = match code_normalise_ou_message(code_brut) {
        Ok(c) => c,
        Err(message) => {
            println!("{message}");
            return Ok(());
        }
    };

    let etat = match synchroniser(config, coffre, None) {
        Ok(e) => e,
        Err(e) => {
            println!("{}", message_utilisateur(&e));
            return Ok(());
        }
    };

    if let Some(message) = message_si_propre_code(&code, &etat.code) {
        println!("{message}");
        return Ok(());
    }

    match ajouter_ami(config, coffre, &code) {
        Ok(AjoutAmi::Envoyee { friendship_id }) => {
            println!("Demande d'ami envoyée (identifiant {friendship_id}).")
        }
        // Couvre AUSSI, par conception, un utilisateur qui a bloqué
        // l'appelant (voir la documentation de `AjoutAmi::CodeIntrouvable`
        // côté bibliothèque) : ne JAMAIS suggérer un blocage ici.
        Ok(AjoutAmi::CodeIntrouvable) => println!("{}", message_code_introuvable()),
        Ok(AjoutAmi::DejaDemandee) => println!("Une demande existe déjà avec ce compte."),
        Err(e) => println!("{}", message_utilisateur(&e)),
    }
    Ok(())
}

/// `friends list` — amis acceptés et demandes reçues (avec leur
/// identifiant, celui attendu par `friends accept`).
///
/// /!\ SYNCHRONISE — voir la limite connue en tête de ce module.
pub fn friends_list(config: &Config, coffre: &Coffre) -> anyhow::Result<()> {
    let etat = match synchroniser(config, coffre, None) {
        Ok(e) => e,
        Err(e) => {
            println!("{}", message_utilisateur(&e));
            return Ok(());
        }
    };

    if etat.amis.is_empty() {
        println!("Aucun ami.");
    } else {
        println!("Amis :");
        for a in &etat.amis {
            println!("- [{}] {}", a.id, a.discord_name);
        }
    }

    if !etat.demandes.is_empty() {
        println!("Demandes reçues :");
        for d in &etat.demandes {
            println!(
                "- [{}] {} (accepter : sky-probe friends accept {})",
                d.friendship_id, d.discord_name, d.friendship_id
            );
        }
    }
    Ok(())
}

/// `code` — affiche le code ami du compte connecté.
///
/// /!\ SYNCHRONISE — voir la limite connue en tête de ce module.
pub fn code(config: &Config, coffre: &Coffre) -> anyhow::Result<()> {
    let etat = match synchroniser(config, coffre, None) {
        Ok(e) => e,
        Err(e) => {
            println!("{}", message_utilisateur(&e));
            return Ok(());
        }
    };
    println!("Code ami : {}", etat.code);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_message_d_echec_de_connexion_est_unique() {
        // Le serveur refuse sans distinguer code inconnu, expiré, déjà
        // consommé ou mauvais secret. L'affichage ne doit pas réintroduire
        // la distinction.
        assert_eq!(
            message_utilisateur(&ErreurCompte::Refuse),
            "Connexion refusée. Relance `sky-probe login`."
        );
    }

    #[test]
    fn aucun_jeton_n_est_jamais_affiche() {
        let jetons = Jetons { session: "SECRET-SESSION".into(), renouvellement: "SECRET-RENOUV".into() };
        let sortie = resume_de_connexion(&jetons, "Killian");
        assert!(!sortie.contains("SECRET"));
        assert!(sortie.contains("Killian"));
    }

    #[test]
    fn les_autres_erreurs_ne_sont_pas_aplaties_au_meme_message() {
        // Contre-preuve du test d'unicité ci-dessus : seule `Refuse` doit
        // recevoir ce traitement particulier, pas les autres variantes.
        let reseau = message_utilisateur(&ErreurCompte::Reseau("x".to_string()));
        assert_ne!(reseau, "Connexion refusée. Relance `sky-probe login`.");
    }

    #[test]
    fn un_identifiant_positif_est_valide() {
        assert!(identifiant_amitie_valide(1));
        assert!(identifiant_amitie_valide(42));
    }

    #[test]
    fn un_identifiant_nul_ou_negatif_est_invalide() {
        assert!(!identifiant_amitie_valide(0));
        assert!(!identifiant_amitie_valide(-3));
    }

    #[test]
    fn un_code_bien_forme_est_normalise() {
        assert_eq!(code_normalise_ou_message("sky-abcd-efgh").as_deref(), Ok("ABCDEFGH"));
    }

    #[test]
    fn un_code_mal_forme_rend_un_message_sans_appel_reseau() {
        // La forme du message importe peu ici (couverte par les tests de
        // `normaliser_code_ami` côté bibliothèque) — ce qui compte est
        // qu'AUCUN chemin réseau ne soit emprunté : `code_normalise_ou_message`
        // est une fonction pure, l'absence d'appel est structurelle.
        assert!(code_normalise_ou_message("INCONNU1").is_err());
    }

    #[test]
    fn son_propre_code_est_signale_sans_reseau() {
        assert!(message_si_propre_code("ABCDEFGH", "ABCDEFGH").is_some());
    }

    #[test]
    fn un_code_different_du_sien_ne_declenche_rien() {
        assert!(message_si_propre_code("ABCDEFGH", "ZZZZZZZZ").is_none());
    }

    #[test]
    fn le_message_de_code_introuvable_n_evoque_jamais_un_blocage() {
        // Le message pour AjoutAmi::CodeIntrouvable couvre deux cas distincts :
        // un code qui n'existe pas, et un utilisateur qui a bloqué l'appelant.
        // Le serveur refuse délibérément de les distinguer (voir la spec) pour
        // protéger la vie privée du bloqueur. Le message affiché ici DOIT
        // reflter cette indistinction et ne JAMAIS suggérer un blocage.
        let msg = message_code_introuvable();
        let msg_minuscule = msg.to_lowercase();
        // Tester les variations plausibles du mot "blocage"
        assert!(
            !msg_minuscule.contains("bloq"),
            "Le message évoque un blocage (variante de 'bloq'). \
             C'est une régression : le blocage doit rester indiscernable d'un code inexistant."
        );
    }

    #[test]
    fn appareil_courant_est_marque() {
        // Crée un appareil test avec id 42
        let appareils = vec![Appareil {
            id: 42,
            nom: "MacBook".to_string(),
            plateforme: "macOS".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            last_seen_at: None,
            revoked_at: None,
        }];
        let lignes = formater_appareils_avec_marque(&appareils, Some(42));
        assert_eq!(lignes.len(), 1);
        assert!(lignes[0].contains("(cet appareil)"));
    }

    #[test]
    fn appareil_non_courant_n_est_pas_marque() {
        let appareils = vec![Appareil {
            id: 42,
            nom: "MacBook".to_string(),
            plateforme: "macOS".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            last_seen_at: None,
            revoked_at: None,
        }];
        let lignes = formater_appareils_avec_marque(&appareils, Some(99));
        assert_eq!(lignes.len(), 1);
        assert!(!lignes[0].contains("(cet appareil)"));
    }

    #[test]
    fn aucun_appareil_courant_ne_marque_rien() {
        let appareils = vec![Appareil {
            id: 42,
            nom: "MacBook".to_string(),
            plateforme: "macOS".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            last_seen_at: None,
            revoked_at: None,
        }];
        let lignes = formater_appareils_avec_marque(&appareils, None);
        assert_eq!(lignes.len(), 1);
        assert!(!lignes[0].contains("(cet appareil)"));
    }

    #[test]
    fn appareil_deja_enregistre_refuse_sans_force() {
        let msg = message_si_appareil_deja_enregistre(42, false);
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("déjà enregistré"));
    }

    #[test]
    fn appareil_deja_enregistre_accepte_avec_force() {
        let msg = message_si_appareil_deja_enregistre(42, true);
        assert!(msg.is_none());
    }
}
