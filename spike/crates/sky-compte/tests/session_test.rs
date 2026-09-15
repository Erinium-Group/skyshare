//! Tests d'intégration de la tâche 6 (connexion native) qui pilotent le serveur
//! double — `echanger_le_code` uniquement : `connecter` ouvre un vrai navigateur et
//! attend une vraie requête entrante, ce n'est pas testable automatiquement.
// `allow(dead_code)` : chaque binaire n'utilise qu'une partie du double, et ses
// propres tests — qui en exerçaient tout — ne sont plus inclus qu'une fois, dans
// `faux_serveur_test.rs` (revue finale, m1).
#[allow(dead_code)]
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

use faux_serveur::FauxServeur;
use sky_compte::session::{avec_jeton_valide, echanger_le_code, jeton_courant, renouveler};
use sky_compte::{ClientHttp, Coffre, Config, ErreurCompte, Jetons};

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
    assert_eq!(jetons.session, "jeton-acces-natif-1");
    assert_eq!(jetons.renouvellement, "jeton-refresh-natif-1");

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
//
// Le sens a été inversé après relecture du coordinateur : `jeton_courant` ne fait
// plus AUCUN appel réseau (voir sa doc dans session.rs) ; c'est `avec_jeton_valide`
// qui exécute l'appel réel de l'appelant et ne renouvelle qu'en réaction à un 401
// effectivement reçu sur CET appel — jamais par sondage préalable. Les tests
// ci-dessous exercent `avec_jeton_valide` avec une fermeture qui fait un VRAI appel
// HTTP à `/api/sky/sync` (via `ClientHttp`, la même route que `synchroniser`, la
// tâche 8, utilisera réellement) : par le vrai chemin, pas une charge fabriquée à
// la main. C'est précisément la charge fabriquée à la main qui était correcte au
// jalon C1, et celle du vrai chemin qui ne l'était pas.

/// Fermeture de test : un appel authentifié réel à la seule route sans effet de
/// bord du double, qui sert de représentant pour "l'appel de la tâche 8/9".
fn sonder_sync(config: &Config, jeton: &str) -> Result<serde_json::Value, ErreurCompte> {
    ClientHttp::new(config).get_json("/api/sky/sync", Some(jeton))
}

#[test]
fn un_401_declenche_un_renouvellement_puis_une_seule_reprise() {
    // Écart volontaire avec le brief : "perime" et "bon" sont enregistrés auprès du
    // double AVANT l'appel. Sans "perime" dans `jetons_acceptes`, le premier appel
    // échouerait pour la cause 1 du double ("Non authentifie", jeton inconnu) et non
    // pour la cause 2 (`refuser_le_premier_appel`) — ce drapeau ne serait alors
    // jamais consommé, et la deuxième tentative (après renouvellement, avec un jeton
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
    let config = Config::vers(&s.url());

    let resultat = avec_jeton_valide(&config, &coffre, |jeton| sonder_sync(&config, jeton));

    assert!(resultat.is_ok(), "attendu un succès après la reprise, obtenu {resultat:?}");
    assert_eq!(s.etat_mut().appels_de_renouvellement, 1, "un seul renouvellement");
    assert_eq!(coffre.jetons().unwrap().unwrap().session, "jeton-neuf-1");
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
    let config = Config::vers(&s.url());

    let r = avec_jeton_valide(&config, &coffre, |jeton| sonder_sync(&config, jeton));

    assert!(matches!(r, Err(ErreurCompte::Refuse)), "attendu Refuse, obtenu {r:?}");
    assert!(
        s.etat_mut().appels_de_renouvellement <= 1,
        "un seul renouvellement tenté, obtenu {}",
        s.etat_mut().appels_de_renouvellement
    );
}

#[test]
fn un_appel_qui_reussit_du_premier_coup_ne_declenche_aucun_renouvellement() {
    // Contre-preuve du premier test : si `avec_jeton_valide` renouvelait
    // systématiquement (au lieu de réagir à un 401 réel), ce test rougirait sur le
    // compteur alors même que le jeton initial était parfaitement valable. Prouve
    // que le renouvellement est conditionnel à un refus réel, pas inconditionnel.
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    let coffre = Coffre::pour_test("sky-test-deja-valide");
    coffre
        .ranger_jetons(&Jetons { session: jeton.clone(), renouvellement: "inutilise".into() })
        .unwrap();
    let config = Config::vers(&s.url());

    let rendu = avec_jeton_valide(&config, &coffre, |j| sonder_sync(&config, j)).unwrap();

    assert_eq!(rendu["version"], 0);
    assert_eq!(s.etat_mut().appels_de_renouvellement, 0);
}

#[test]
fn avec_jeton_valide_rend_bien_la_valeur_de_lappel_reussi() {
    // Qu'est-ce qui ferait échouer ce test précisément ? Que `avec_jeton_valide`
    // n'oublie ou ne remplace la valeur T produite par l'appel de l'appelant — la
    // seule raison d'être de son type générique. Distinct du test précédent : celui
    // d'au-dessus vérifie l'ABSENCE de renouvellement, celui-ci vérifie que la
    // valeur traverse intacte même en présence d'un renouvellement (le double range
    // des enveloppes dans son état, relues par `/api/sky/sync` : un contenu non
    // trivial, pas une coïncidence de type).
    let s = FauxServeur::demarrer();
    s.etat_mut().jetons_acceptes.insert("perime".to_string());
    s.etat_mut().jetons_de_renouvellement_valides.insert("bon".to_string());
    s.etat_mut().refuser_le_premier_appel = true;
    s.etat_mut().version = 77;
    let coffre = Coffre::pour_test("sky-test-valeur-traverse");
    coffre.ranger_jetons(&Jetons { session: "perime".into(), renouvellement: "bon".into() }).unwrap();
    let config = Config::vers(&s.url());

    let recu = avec_jeton_valide(&config, &coffre, |j| sonder_sync(&config, j)).unwrap();

    assert_eq!(recu["version"], 77);
}

// --- jeton_courant : plus aucun appel réseau ---------------------------

#[test]
fn jeton_courant_sans_coffre_rempli_est_refuse() {
    // Test DIRECT, sans double ni réseau : si la garde de `jeton_courant` était
    // retirée (p. ex. remplacée par un jeton vide via `unwrap_or_default`), CE test
    // rougirait précisément ici, sans dépendre d'un filet de sécurité ailleurs —
    // contrairement à l'ancienne version de ce test (voir task-7-report.md,
    // neutralisation n°4) qui passait quand même via la garde, elle aussi présente,
    // de `renouveler` : ce test-ci isole la garde qu'il prétend vérifier.
    let coffre = Coffre::pour_test("sky-test-jamais-connecte");

    let r = jeton_courant(&coffre);

    assert!(matches!(r, Err(ErreurCompte::Refuse)), "attendu Refuse, obtenu {r:?}");
}

#[test]
fn jeton_courant_rend_le_jeton_range_sans_le_verifier() {
    // Échoue si `jeton_courant` transformait, tronquait, ou refusait à tort un
    // jeton pourtant présent en coffre — même un jeton que le double n'a jamais vu
    // (aucun `FauxServeur` ici : la fonction ne doit RIEN interroger).
    let coffre = Coffre::pour_test("sky-test-jeton-present");
    coffre
        .ranger_jetons(&Jetons { session: "jamais-vu-du-serveur".into(), renouvellement: "r".into() })
        .unwrap();

    assert_eq!(jeton_courant(&coffre).unwrap(), "jamais-vu-du-serveur");
}

// --- renouveler : garde propre, indépendante d'avec_jeton_valide -------

#[test]
fn renouveler_sans_coffre_rempli_est_refuse_sans_appel_reseau() {
    // `renouveler` est `pub` et appelable directement, sans passer par
    // `avec_jeton_valide` — sa propre garde contre un coffre vide n'est donc pas
    // redondante avec celle de `jeton_courant` : cette dernière n'est simplement
    // jamais atteinte sur CE chemin d'appel. Le port 1 en local refuse la connexion
    // immédiatement (voir `http.rs::aucun_jeton_dans_lechec_reseau_reel`) : si la
    // garde de `renouveler` disparaissait, ce test rougirait quand même, mais avec
    // `ErreurCompte::Reseau`, PAS `Refuse` — la distinction precise que ce test
    // vérifie, pas seulement "une erreur quelconque".
    let coffre = Coffre::pour_test("sky-test-renouveler-vide");

    let r = renouveler(&Config::vers("http://127.0.0.1:1"), &coffre);

    assert!(matches!(r, Err(ErreurCompte::Refuse)), "attendu Refuse (pas Reseau), obtenu {r:?}");
}
