//! Rattachement de l'appareil à une nouvelle session (revue finale du jalon C2, I1),
//! contre le serveur double.
//!
//! Le défaut : le site ne reconnaît « l'appareil courant » que par la session qui l'a
//! enregistré (`devices.session_id`), et chaque connexion crée une nouvelle session.
//! Sans rattachement, après un nouveau `login`, plus aucune enveloppe n'est livrée à
//! la machine — sans la moindre erreur. Un renouvellement de jeton, lui, garde la
//! session et ne rompt rien.

// `allow(dead_code)` : chaque binaire n'utilise qu'une partie du double, dont les
// propres tests ne sont inclus que dans `faux_serveur_test.rs` (revue finale, m1).
#[allow(dead_code)]
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use faux_serveur::{EnveloppeFausse, FauxServeur};
use sky_compte::{
    echanger_le_code, enregistrer_appareil, rattacher_appareil, renouveler, synchroniser, ClientHttp,
    Coffre, Config, ErreurCompte, Rattachement,
};

/// Connexion native contre le double : un code neuf (le code est à usage unique),
/// échangé, jetons rangés dans `coffre` — ce que fait `connecter` une fois la
/// redirection reçue.
fn se_connecter(s: &FauxServeur, config: &Config, coffre: &Coffre, code: &str) {
    s.etat_mut().code_natif_valide = Some((code.to_string(), "secret-de-test".to_string()));
    let jetons = echanger_le_code(config, code, "secret-de-test").expect("échange natif attendu en succès");
    coffre.ranger_jetons(&jetons).unwrap();
}

/// Machine prête avant un nouveau `login` : connectée une première fois, appareil
/// enregistré avec la clé du coffre. Rend l'identifiant de l'appareil.
fn machine_prete(s: &FauxServeur, config: &Config, coffre: &Coffre, nom: &str) -> i64 {
    se_connecter(s, config, coffre, "CODE-PREMIER-LOGIN");
    let cle = coffre.identite().unwrap().public_key();
    enregistrer_appareil(config, coffre, nom, &cle).unwrap()
}

/// Place dans la boîte du double une enveloppe pour l'appareil `destinataire` — ce
/// que ferait le dépôt d'un ami qui voit cet appareil. Le contenu importe peu ici.
fn enveloppe_pour(s: &FauxServeur, destinataire: i64) {
    let mut e = s.etat_mut();
    let id = format!("env-{}", e.enveloppes.len() + 1);
    e.enveloppes.push(EnveloppeFausse {
        id,
        expediteur_device_id: 777,
        destinataire_device_id: destinataire,
        charge: "YQ==".to_string(),
    });
}

#[test]
fn a_un_renouvellement_de_jeton_n_empeche_pas_la_reception() {
    // (a) Échoue si le jeton renouvelé ne portait plus la session de l'appareil : la
    // synchronisation qui suit le renouvellement ne livrerait rien.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = Coffre::pour_test("sky-test-rattache-a-renouvellement");
    let id = machine_prete(&s, &config, &coffre, "PC de test");

    // Le prochain appel est refusé une fois : `synchroniser` renouvelle, puis retente.
    s.etat_mut().refuser_le_premier_appel = true;
    enveloppe_pour(&s, id);
    let etat = synchroniser(&config, &coffre, None).unwrap();

    assert_eq!(s.etat_mut().appels_de_renouvellement, 1, "le renouvellement doit avoir eu lieu");
    assert_eq!(etat.enveloppes.len(), 1, "enveloppe non livrée après un renouvellement");
}

#[test]
fn b_une_nouvelle_connexion_sans_rattachement_ne_recoit_plus_rien() {
    // (b) Le défaut I1, reproduit : après une nouvelle connexion, l'appareil enregistré
    // appartient à l'ancienne session, et la synchronisation ne livre rien — sans
    // erreur. Si ce test rougit un jour, le site a changé de modèle : relire I1.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = Coffre::pour_test("sky-test-rattache-b-sans");
    let id = machine_prete(&s, &config, &coffre, "PC de test");

    se_connecter(&s, &config, &coffre, "CODE-SECOND-LOGIN");
    enveloppe_pour(&s, id);
    let etat = synchroniser(&config, &coffre, None).unwrap();

    assert!(etat.enveloppes.is_empty(), "sans rattachement, rien ne doit être livré");
    assert_eq!(s.etat_mut().enveloppes.len(), 1, "l'enveloppe attend toujours, jamais livrée");
}

#[test]
fn c_une_nouvelle_connexion_avec_rattachement_recoit_de_nouveau() {
    // (c) Échoue si le rattachement ne révoquait pas l'ancien appareil (deux appareils
    // actifs : les amis scelleraient encore pour l'ancien), ne rangeait pas le nouvel
    // identifiant, changeait de clé, ou ne liait pas l'appareil à la nouvelle session.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = Coffre::pour_test("sky-test-rattache-c-avec");
    let ancien = machine_prete(&s, &config, &coffre, "PC de test");
    let cle = coffre.identite().unwrap().public_key();

    se_connecter(&s, &config, &coffre, "CODE-SECOND-LOGIN");
    let issue = rattacher_appareil(&config, &coffre).unwrap();

    let Rattachement::Rattache { ancien: ancien_rendu, nouveau, nom } = issue else {
        panic!("attendu Rattache, obtenu {issue:?}");
    };
    assert_eq!(ancien_rendu, ancien);
    assert_ne!(nouveau, ancien);
    assert_eq!(nom, "PC de test", "le nom de l'appareil est conservé");
    assert_eq!(coffre.identifiant_appareil().unwrap(), Some(nouveau), "le nouvel identifiant doit être rangé");
    assert_eq!(
        s.etat_mut().cles_des_appareils.get(&nouveau),
        Some(&STANDARD.encode(cle)),
        "réenregistré avec la MÊME clé publique"
    );

    let etat = synchroniser(&config, &coffre, None).unwrap();
    let actifs: Vec<i64> = etat.appareils.iter().filter(|a| a.revoked_at.is_none()).map(|a| a.id).collect();
    assert_eq!(actifs, vec![nouveau], "l'ancien appareil doit être révoqué");

    enveloppe_pour(&s, nouveau);
    let etat = synchroniser(&config, &coffre, None).unwrap();
    assert_eq!(etat.enveloppes.len(), 1, "après rattachement, l'enveloppe doit être livrée");
}

#[test]
fn d_un_reenregistrement_qui_echoue_laisse_l_ancien_identifiant_range() {
    // (d) Échoue si l'identifiant rangé était remplacé (ou effacé) avant que le
    // réenregistrement ait réussi : la machine perdrait la trace de son appareil.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = Coffre::pour_test("sky-test-rattache-d-echec");
    let ancien = machine_prete(&s, &config, &coffre, "PC de test");

    se_connecter(&s, &config, &coffre, "CODE-SECOND-LOGIN");
    s.etat_mut().refuser_enregistrement_appareil = true;
    let issue = rattacher_appareil(&config, &coffre);

    assert!(issue.is_err(), "un réenregistrement refusé est une erreur, obtenu {issue:?}");
    assert_eq!(coffre.identifiant_appareil().unwrap(), Some(ancien));
}

#[test]
fn e_la_revocation_refuse_ensuite_les_jetons_de_l_ancienne_session() {
    // (e) Échoue si révoquer l'appareil ne révoquait pas la session qu'il portait :
    // les jetons de l'ancienne session resteraient utilisables.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = Coffre::pour_test("sky-test-rattache-e-revocation");
    machine_prete(&s, &config, &coffre, "PC de test");
    let anciens_jetons = coffre.jetons().unwrap().unwrap();

    se_connecter(&s, &config, &coffre, "CODE-SECOND-LOGIN");
    rattacher_appareil(&config, &coffre).unwrap();

    let appel: Result<serde_json::Value, ErreurCompte> =
        ClientHttp::new(&config).get_json("/api/sky/sync", Some(&anciens_jetons.session));
    assert!(matches!(appel, Err(ErreurCompte::Refuse)), "jeton d'accès de l'ancienne session : {appel:?}");

    let coffre_ancien = Coffre::pour_test("sky-test-rattache-e-ancienne-session");
    coffre_ancien.ranger_jetons(&anciens_jetons).unwrap();
    let renouvellement = renouveler(&config, &coffre_ancien);
    assert!(
        matches!(renouvellement, Err(ErreurCompte::Refuse)),
        "renouvellement de l'ancienne session : {renouvellement:?}"
    );

    // Contre-preuve : la session courante, elle, passe toujours.
    synchroniser(&config, &coffre, None).unwrap();
}

#[test]
fn f_un_appareil_inconnu_du_compte_est_reenregistre_sous_un_nom_par_defaut() {
    // Identifiant laissé sur la machine par un autre compte : la révocation rend 404
    // (accepté), et l'appareil est réenregistré quand même — sans nom connu.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = Coffre::pour_test("sky-test-rattache-f-inconnu");
    se_connecter(&s, &config, &coffre, "CODE-F");
    coffre.ranger_identifiant_appareil(4242).unwrap();

    let issue = rattacher_appareil(&config, &coffre).unwrap();

    let Rattachement::ReenregistreSousNomParDefaut { ancien, nouveau, nom } = issue else {
        panic!("attendu ReenregistreSousNomParDefaut, obtenu {issue:?}");
    };
    assert_eq!(ancien, 4242);
    assert!(!nom.is_empty());
    assert_eq!(coffre.identifiant_appareil().unwrap(), Some(nouveau));
}

#[test]
fn g_sans_appareil_enregistre_rien_n_est_fait() {
    // Échoue si le rattachement créait un appareil alors qu'aucun n'était enregistré
    // sur cette machine : `device register` reste le seul à en créer un premier.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = Coffre::pour_test("sky-test-rattache-g-sans-appareil");
    se_connecter(&s, &config, &coffre, "CODE-G");

    assert_eq!(rattacher_appareil(&config, &coffre).unwrap(), Rattachement::AucunAppareil);
    assert_eq!(s.etat_mut().prochain_id_appareil, 0);
}
