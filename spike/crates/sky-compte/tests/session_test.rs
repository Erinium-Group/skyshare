//! Tests d'intégration de la tâche 6 (connexion native) qui pilotent le serveur
//! double — `echanger_le_code` uniquement : `connecter` ouvre un vrai navigateur et
//! attend une vraie requête entrante, ce n'est pas testable automatiquement.
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

use faux_serveur::FauxServeur;
use sky_compte::session::echanger_le_code;
use sky_compte::{Coffre, Config, ErreurCompte};

#[test]
fn un_code_refuse_ne_range_rien_dans_le_coffre() {
    let s = FauxServeur::demarrer();
    s.etat_mut().refuser_echange = true;
    let coffre = Coffre::pour_test("sky-test-refus");

    let r = echanger_le_code(&Config::vers(&s.url()), "code-bidon", "secret-bidon");

    assert!(matches!(r, Err(ErreurCompte::Refuse)));
    assert!(coffre.jetons().unwrap().is_none());
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
    // propriété centrale du flux (un code seul ne suffit jamais).
    let s = FauxServeur::demarrer();
    s.etat_mut().code_natif_valide = Some(("CODEVALIDE".to_string(), "secret-correct".to_string()));
    let coffre = Coffre::pour_test("sky-test-mauvais-secret");

    let r = echanger_le_code(&Config::vers(&s.url()), "CODEVALIDE", "secret-incorrect");

    assert!(matches!(r, Err(ErreurCompte::Refuse)));
    assert!(coffre.jetons().unwrap().is_none());
}
