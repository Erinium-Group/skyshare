//! Tests d'intégration de la tâche 6 (connexion native) qui pilotent le serveur
//! double — `echanger_le_code` uniquement : `connecter` ouvre un vrai navigateur et
//! attend une vraie requête entrante, ce n'est pas testable automatiquement.
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

use faux_serveur::FauxServeur;
use sky_compte::session::echanger_le_code;
use sky_compte::{Coffre, Config, ErreurCompte};

// La propriété « rien n'est rangé dans le coffre en cas de refus » n'est PAS testée
// ici : `echanger_le_code` ne reçoit jamais de `Coffre` (seule `connecter` en reçoit
// un), donc un test qui créerait un coffre à côté de cet appel ne pourrait jamais le
// voir écrit ni rougir si un bug l'y faisait écrire. Cette propriété vit dans
// `connecter`, qui reste une limite connue et non couverte : elle ouvre un vrai
// navigateur et attend une vraie requête entrante, ce qui la rend non testable
// automatiquement. Ici, seul ce que `echanger_le_code` peut réellement faire échouer
// est vérifié.
#[test]
fn un_code_refuse_rend_une_erreur_refuse() {
    let s = FauxServeur::demarrer();
    s.etat_mut().refuser_echange = true;

    let r = echanger_le_code(&Config::vers(&s.url()), "code-bidon", "secret-bidon");

    assert!(matches!(r, Err(ErreurCompte::Refuse)));
}

#[test]
fn echanger_le_code_reussit_et_les_jetons_sont_utilisables_ensuite() {
    // Échoue si les champs `acces`/`refresh` du double n'étaient pas mappés vers
    // `session`/`renouvellement`, ou si le jeton reçu ne pouvait pas ensuite être
    // rangé puis relu depuis le coffre — la moitié qui prouve que ce chemin de
    // succès sert réellement à quelque chose.
    let s = FauxServeur::demarrer();
    s.etat_mut().code_natif_valide = Some(("CODEVALIDE".to_string(), "secret-correct".to_string()));
    let coffre = Coffre::pour_test("sky-test-echange-reussi");

    let jetons = echanger_le_code(&Config::vers(&s.url()), "CODEVALIDE", "secret-correct").unwrap();
    assert_eq!(jetons.session, "jeton-acces-natif");
    assert_eq!(jetons.renouvellement, "jeton-refresh-natif");

    coffre.ranger_jetons(&jetons).unwrap();
    assert_eq!(coffre.jetons().unwrap(), Some(jetons));
}

#[test]
fn echanger_le_code_avec_un_secret_incorrect_est_refuse() {
    // Distinct du test de refus explicite (`refuser_echange`) : ici le code EST
    // enregistré comme valide, mais le secret présenté ne correspond pas — la
    // propriété centrale du flux (un code seul ne suffit jamais). Même remarque que
    // pour `un_code_refuse_rend_une_erreur_refuse` : pas de `Coffre` ici, cette
    // fonction n'en reçoit jamais.
    let s = FauxServeur::demarrer();
    s.etat_mut().code_natif_valide = Some(("CODEVALIDE".to_string(), "secret-correct".to_string()));

    let r = echanger_le_code(&Config::vers(&s.url()), "CODEVALIDE", "secret-incorrect");

    assert!(matches!(r, Err(ErreurCompte::Refuse)));
}
