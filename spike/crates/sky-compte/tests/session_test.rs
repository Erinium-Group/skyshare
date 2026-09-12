//! Tests d'intégration de la tâche 6 (connexion native) qui pilotent le serveur
//! double — `echanger_le_code` uniquement : `connecter` ouvre un vrai navigateur et
//! attend une vraie requête entrante, ce n'est pas testable automatiquement.
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

use faux_serveur::FauxServeur;
use sky_compte::session::{echanger_le_code, jeton_valide};
use sky_compte::{Coffre, Config, ErreurCompte, Jetons};

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

// --- Tâche 7 : renouvellement de jeton --------------------------------

#[test]
fn un_401_declenche_un_renouvellement_puis_une_seule_reprise() {
    // Par le VRAI chemin : le jeton présenté vient du coffre, pas d'une charge
    // fabriquée à la main. C'est précisément la charge fabriquée à la main qui était
    // correcte au jalon C1, et celle du vrai chemin qui ne l'était pas.
    //
    // Écart volontaire avec le brief : "perime" et "bon" sont enregistrés auprès du
    // double AVANT l'appel. Sans "perime" dans `jetons_acceptes`, le premier appel
    // échouerait pour la cause 1 du double ("Non authentifie", jeton inconnu) et non
    // pour la cause 2 (`refuser_le_premier_appel`) — ce drapeau ne serait alors
    // jamais consommé, et la deuxième sonde (après renouvellement, avec un jeton
    // "jeton-neuf" nouvellement reconnu) tomberait dessus à son tour, cassant la
    // propriété "une seule reprise suffit". Enregistrer "perime" reproduit fidèlement
    // ce que `refuser_le_premier_appel` modélise : un jeton reconnu mais périmé une
    // fois — pas un jeton jamais délivré. Sans "bon" dans
    // `jetons_de_renouvellement_valides`, `POST /api/auth/refresh` échouerait, lui,
    // pour une tout autre raison (jeton de renouvellement inconnu).
    let s = FauxServeur::demarrer();
    s.etat_mut().jetons_acceptes.insert("perime".to_string());
    s.etat_mut().jetons_de_renouvellement_valides.insert("bon".to_string());
    s.etat_mut().refuser_le_premier_appel = true;
    let coffre = Coffre::pour_test("sky-test-renouv");
    coffre.ranger_jetons(&Jetons { session: "perime".into(), renouvellement: "bon".into() }).unwrap();

    let jeton = jeton_valide(&Config::vers(&s.url()), &coffre).unwrap();

    assert_eq!(jeton, "jeton-neuf");
    assert_eq!(s.etat_mut().appels_de_renouvellement, 1, "un seul renouvellement");
    assert_eq!(coffre.jetons().unwrap().unwrap().session, "jeton-neuf");
}

#[test]
fn un_renouvellement_refuse_ne_boucle_pas() {
    // Sans cette garantie, un jeton de renouvellement révoqué produit une boucle
    // infinie d'appels au serveur. `refuser_tout` refuse ici À LA FOIS l'appel
    // authentifié initial ET le renouvellement — peu importe que "x"/"y" soient
    // eux-mêmes reconnus par le double ou non, `refuser_tout` court-circuite les
    // deux avant même que cette reconnaissance ne soit consultée (voir
    // `autoriser_appel` et `gerer_refresh` dans le double).
    let s = FauxServeur::demarrer();
    s.etat_mut().refuser_tout = true;
    let coffre = Coffre::pour_test("sky-test-boucle");
    coffre.ranger_jetons(&Jetons { session: "x".into(), renouvellement: "y".into() }).unwrap();

    let r = jeton_valide(&Config::vers(&s.url()), &coffre);

    assert!(matches!(r, Err(ErreurCompte::Refuse)));
    assert!(s.etat_mut().appels_de_renouvellement <= 1);
}

#[test]
fn jeton_valide_sans_coffre_rempli_est_refuse() {
    // Qu'est-ce qui ferait échouer ce test précisément ? Que `jeton_valide` panique,
    // ou rende une autre variante que `Refuse`, quand le coffre ne contient encore
    // aucun jeton (application jamais connectée) — aucun appel réseau n'a de raison
    // d'être tenté dans ce cas.
    let s = FauxServeur::demarrer();
    let coffre = Coffre::pour_test("sky-test-jamais-connecte");

    let r = jeton_valide(&Config::vers(&s.url()), &coffre);

    assert!(matches!(r, Err(ErreurCompte::Refuse)));
}

#[test]
fn un_jeton_deja_accepte_ne_declenche_aucun_renouvellement() {
    // Contre-preuve du premier test : si `jeton_valide` renouvelait
    // systématiquement, ce test rougirait sur le compteur alors même que le jeton
    // initial était parfaitement valable. Prouve que le renouvellement est
    // conditionnel à un refus réel, pas inconditionnel.
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    let coffre = Coffre::pour_test("sky-test-deja-valide");
    coffre
        .ranger_jetons(&Jetons { session: jeton.clone(), renouvellement: "inutilise".into() })
        .unwrap();

    let rendu = jeton_valide(&Config::vers(&s.url()), &coffre).unwrap();

    assert_eq!(rendu, jeton);
    assert_eq!(s.etat_mut().appels_de_renouvellement, 0);
}
