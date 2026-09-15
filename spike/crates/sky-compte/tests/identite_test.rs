//! Tests d'intégration de la tâche 10 (`GET /api/auth/me`) qui pilotent le
//! serveur double : `moi`.
// `allow(dead_code)` : chaque binaire n'utilise qu'une partie du double, et ses
// propres tests — qui en exerçaient tout — ne sont plus inclus qu'une fois, dans
// `faux_serveur_test.rs` (revue finale, m1).
#[allow(dead_code)]
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

use faux_serveur::{FauxServeur, MoiFaux};
use sky_compte::{Coffre, Config, ErreurCompte, Jetons, Moi};

fn coffre_authentifie(prefixe: &str, s: &FauxServeur) -> Coffre {
    let coffre = Coffre::pour_test(prefixe);
    let jeton = s.jeton_de_test();
    coffre
        .ranger_jetons(&Jetons { session: jeton, renouvellement: "peu-importe".to_string() })
        .unwrap();
    coffre
}

#[test]
fn moi_lit_lidentifiant_et_le_nom_discord_en_camel_case() {
    // Échoue si `moi` lisait `discord_name` (snake_case) au lieu de
    // `discordName` (camelCase, forme exacte de la réponse du site) : le
    // champ manquerait à la désérialisation et l'appel rendrait une
    // erreur de protocole plutôt que la valeur attendue.
    let s = FauxServeur::demarrer();
    s.etat_mut().moi = Some(MoiFaux::nouveau(42, "Killian"));
    let coffre = coffre_authentifie("sky-test-identite-succes", &s);

    let moi = sky_compte::moi(&Config::vers(&s.url()), &coffre).unwrap();

    assert_eq!(moi, Moi { id: 42, discord_name: "Killian".to_string() });
}

#[test]
fn moi_sans_utilisateur_connu_est_une_erreur_de_protocole() {
    // Le double ne rend 404 « Utilisateur introuvable » que si aucun `moi`
    // n'a été enregistré — même forme que `findUserById` rendant `null`
    // côté site.
    let s = FauxServeur::demarrer();
    let coffre = coffre_authentifie("sky-test-identite-404", &s);

    let r = sky_compte::moi(&Config::vers(&s.url()), &coffre);

    assert!(matches!(r, Err(ErreurCompte::Protocole(_))));
}

#[test]
fn moi_avec_session_partielle_est_une_erreur_de_protocole() {
    // Le 403 de session partielle (TOTP non vérifié) n'a pas de traitement
    // distinct côté client : une erreur de protocole ordinaire, comme tout
    // statut que ce client ne distingue pas explicitement.
    let s = FauxServeur::demarrer();
    s.etat_mut().moi = Some(MoiFaux::nouveau(1, "peu importe"));
    s.etat_mut().session_partielle_totp = true;
    let coffre = coffre_authentifie("sky-test-identite-403", &s);

    let r = sky_compte::moi(&Config::vers(&s.url()), &coffre);

    assert!(matches!(r, Err(ErreurCompte::Protocole(_))));
}

#[test]
fn moi_sans_jeton_reconnu_est_refuse() {
    // Même mécanisme d'authentification que les autres routes protégées
    // (`autoriser_appel`) : un jeton absent du coffre échoue AVANT même
    // d'atteindre le double.
    let s = FauxServeur::demarrer();
    let coffre = Coffre::pour_test("sky-test-identite-sans-jeton");

    let r = sky_compte::moi(&Config::vers(&s.url()), &coffre);

    assert!(matches!(r, Err(ErreurCompte::Refuse)));
}
