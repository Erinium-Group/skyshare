//! Tests d'intégration de la tâche 8 (l'annuaire) qui pilotent le serveur
//! double : `synchroniser`, `enregistrer_appareil`, `ajouter_ami`,
//! `accepter_ami`. `resoudre_ami` et la validation de clé publique sont
//! testées en unité dans `annuaire.rs` (pas besoin du double, aucune E/S).
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

use faux_serveur::{AmiFaux, AppareilDAmiFaux, FauxServeur};
use sky_compte::{
    accepter_ami, ajouter_ami, enregistrer_appareil, synchroniser, Acceptation, AjoutAmi, Coffre,
    Config, Etat,
};

fn etat_vide() -> Etat {
    Etat { version: 0, code: String::new(), amis: Vec::new(), demandes: Vec::new(), appareils: Vec::new(), enveloppes: Vec::new() }
}

// --- synchroniser -------------------------------------------------------

#[test]
fn premiere_synchronisation_sans_precedent_rend_letat_complet() {
    // Échoue si `synchroniser` n'envoyait pas le jeton, ne construisait pas
    // le chemin sans `?version=` en l'absence de précédent, ou ne mappait
    // pas correctement `version`/`code`/`amis`.
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    s.etat_mut().version = 7;
    s.etat_mut().amis.push(AmiFaux::sans_appareil("bob"));

    let coffre = Coffre::pour_test("sky-test-annuaire-sync-premiere");
    coffre.ranger_jetons(&sky_compte::Jetons { session: jeton, renouvellement: "peu-importe".to_string() }).unwrap();

    let etat = synchroniser(&Config::vers(&s.url()), &coffre, None).unwrap();
    assert_eq!(etat.version, 7);
    assert_eq!(etat.amis.len(), 1);
    assert_eq!(etat.amis[0].discord_name, "bob");
}

#[test]
fn synchroniser_ne_jette_jamais_les_enveloppes_dune_reponse_complete() {
    // NEUTRALISATION CIBLÉE PAR LE CAHIER DES CHARGES : si `convertir_etat`
    // oubliait de reporter `enveloppes`, ce test rougirait alors que
    // `amis` resterait correct — la preuve que c'est bien CE champ qui est
    // vérifié, pas un effet de bord d'un autre.
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    s.etat_mut().version = 1;

    let coffre = Coffre::pour_test("sky-test-annuaire-sync-enveloppes");
    coffre.ranger_jetons(&sky_compte::Jetons { session: jeton, renouvellement: "peu-importe".to_string() }).unwrap();

    // Dépose une enveloppe directement dans l'état piloté plutôt que par
    // `POST /api/sky/envelopes` : ce test vise `synchroniser`, pas le dépôt.
    s.etat_mut().enveloppes.push(faux_serveur::EnveloppeFausse {
        id: "1".to_string(),
        expediteur_device_id: 7,
        destinataire_device_id: 9,
        charge: "YWJj".to_string(),
    });

    let etat = synchroniser(&Config::vers(&s.url()), &coffre, None).unwrap();
    assert_eq!(etat.enveloppes.len(), 1);
    assert_eq!(etat.enveloppes[0].id, "1");
    assert_eq!(etat.enveloppes[0].expediteur_device_id, 7);
    assert_eq!(etat.enveloppes[0].destinataire_device_id, 9);
    assert_eq!(etat.enveloppes[0].charge, "YWJj");
}

#[test]
fn inchange_conserve_lami_precedent_mais_vide_les_enveloppes() {
    // NEUTRALISATION CIBLÉE : si le cas `inchange` de `synchroniser`
    // recopiait `precedent.enveloppes` tel quel plutôt que de le vider,
    // ce test rougirait précisément sur `enveloppes`, jamais sur `amis`
    // (qui doit, lui, bien être conservé).
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    s.etat_mut().version = 3;
    s.etat_mut().amis.push(AmiFaux::sans_appareil("alice"));

    let coffre = Coffre::pour_test("sky-test-annuaire-inchange");
    coffre.ranger_jetons(&sky_compte::Jetons { session: jeton, renouvellement: "peu-importe".to_string() }).unwrap();

    let precedent = Etat {
        version: 3,
        code: "ANCIEN01".to_string(),
        amis: vec![sky_compte::Ami { id: 1, discord_name: "alice".to_string(), appareils: Vec::new() }],
        demandes: Vec::new(),
        appareils: Vec::new(),
        enveloppes: vec![sky_compte::EnveloppeRecue {
            id: "99".to_string(),
            expediteur_device_id: 1,
            destinataire_device_id: 2,
            charge: "YQ==".to_string(),
        }],
    };

    let etat = synchroniser(&Config::vers(&s.url()), &coffre, Some(&precedent)).unwrap();
    assert_eq!(etat.version, 3);
    assert_eq!(etat.amis.len(), 1, "les amis du précédent doivent être conservés sur `inchange`");
    assert!(etat.enveloppes.is_empty(), "les enveloppes déjà livrées ne doivent pas être retraitées");
}

#[test]
fn une_enveloppe_en_attente_court_circuite_inchange_meme_a_version_egale() {
    // Preuve, côté client, de la neutralisation exigée par le cahier des
    // charges sur le double : une enveloppe déposée entre deux appels à
    // version inchangée doit continuer à voyager, pas se perdre derrière
    // un `{ inchange: true }` prématuré.
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    s.etat_mut().version = 5;

    let coffre = Coffre::pour_test("sky-test-annuaire-envelope-court-circuite");
    coffre.ranger_jetons(&sky_compte::Jetons { session: jeton, renouvellement: "peu-importe".to_string() }).unwrap();

    let precedent = Etat { version: 5, ..etat_vide() };

    s.etat_mut().enveloppes.push(faux_serveur::EnveloppeFausse {
        id: "42".to_string(),
        expediteur_device_id: 1,
        destinataire_device_id: 2,
        charge: "eA==".to_string(),
    });

    let etat = synchroniser(&Config::vers(&s.url()), &coffre, Some(&precedent)).unwrap();
    assert_eq!(etat.enveloppes.len(), 1);
    assert_eq!(etat.enveloppes[0].id, "42");
}

// Le cas « `{ inchange: true }` reçu sans précédent » n'est PAS testé ici :
// le protocole réel ne peut pas le produire à travers ce double (sans
// précédent, `synchroniser` n'envoie jamais `?version=`, donc
// `version_connue` vaut toujours `None` côté double, qui ne peut alors
// jamais correspondre à `Some(e.version)`) — un test qui tenterait de le
// provoquer via `FauxServeur` ne ferait que prouver que le double se
// comporte normalement, pas que la garde du client fonctionne. Cette garde
// est testée directement en unité dans `annuaire.rs`
// (`inchange_sans_precedent_est_une_erreur_de_protocole`), où la valeur
// `ReponseSync::Inchange` peut être construite sans passer par une requête
// HTTP.

// --- enregistrer_appareil ------------------------------------------------

#[test]
fn enregistrer_appareil_reussit_et_rend_lidentifiant() {
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    let coffre = Coffre::pour_test("sky-test-annuaire-enregistrer-appareil");
    coffre.ranger_jetons(&sky_compte::Jetons { session: jeton, renouvellement: "peu-importe".to_string() }).unwrap();

    let cle = [3u8; 32];
    let id = enregistrer_appareil(&Config::vers(&s.url()), &coffre, "Mon PC", &cle).unwrap();
    assert_eq!(id, 1);

    let cle2 = [4u8; 32];
    let id2 = enregistrer_appareil(&Config::vers(&s.url()), &coffre, "Mon autre PC", &cle2).unwrap();
    assert_eq!(id2, 2, "un second enregistrement doit recevoir un identifiant distinct");
}

// --- ajouter_ami ----------------------------------------------------------

#[test]
fn ajouter_ami_reussit_avec_le_code_enregistre() {
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    s.etat_mut().code_ami_valide = Some(("AMIVALID".to_string(), 42));

    let coffre = Coffre::pour_test("sky-test-annuaire-ajouter-ami-succes");
    coffre.ranger_jetons(&sky_compte::Jetons { session: jeton, renouvellement: "peu-importe".to_string() }).unwrap();

    let r = ajouter_ami(&Config::vers(&s.url()), &coffre, "AMIVALID").unwrap();
    assert_eq!(r, AjoutAmi::Envoyee { friendship_id: 42 });
}

#[test]
fn ajouter_ami_avec_un_code_inconnu_rend_code_introuvable_pas_une_erreur() {
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();

    let coffre = Coffre::pour_test("sky-test-annuaire-ajouter-ami-inconnu");
    coffre.ranger_jetons(&sky_compte::Jetons { session: jeton, renouvellement: "peu-importe".to_string() }).unwrap();

    let r = ajouter_ami(&Config::vers(&s.url()), &coffre, "INCONNU1").unwrap();
    assert_eq!(r, AjoutAmi::CodeIntrouvable);
}

#[test]
fn ajouter_ami_deja_demande_rend_deja_demandee_pas_une_erreur() {
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    s.etat_mut().code_ami_valide = Some(("AMIVALID".to_string(), 42));
    s.etat_mut().ami_deja_demande = true;

    let coffre = Coffre::pour_test("sky-test-annuaire-ajouter-ami-deja");
    coffre.ranger_jetons(&sky_compte::Jetons { session: jeton, renouvellement: "peu-importe".to_string() }).unwrap();

    let r = ajouter_ami(&Config::vers(&s.url()), &coffre, "AMIVALID").unwrap();
    assert_eq!(r, AjoutAmi::DejaDemandee);
}

// --- accepter_ami ----------------------------------------------------------

#[test]
fn accepter_ami_reussit_pour_lamitie_acceptable() {
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    s.etat_mut().amitie_acceptable = Some(7);

    let coffre = Coffre::pour_test("sky-test-annuaire-accepter-succes");
    coffre.ranger_jetons(&sky_compte::Jetons { session: jeton, renouvellement: "peu-importe".to_string() }).unwrap();

    let r = accepter_ami(&Config::vers(&s.url()), &coffre, 7).unwrap();
    assert_eq!(r, Acceptation::Acceptee);
}

#[test]
fn accepter_ami_pour_une_amitie_differente_rend_introuvable_pas_une_erreur() {
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    s.etat_mut().amitie_acceptable = Some(7);

    let coffre = Coffre::pour_test("sky-test-annuaire-accepter-introuvable");
    coffre.ranger_jetons(&sky_compte::Jetons { session: jeton, renouvellement: "peu-importe".to_string() }).unwrap();

    let r = accepter_ami(&Config::vers(&s.url()), &coffre, 8).unwrap();
    assert_eq!(r, Acceptation::Introuvable);
}

// --- Bout en bout : appareil d'ami exposé par sync, filtré si corrompu ----

#[test]
fn synchroniser_expose_lappareil_dun_ami_avec_sa_cle_publique() {
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    s.etat_mut().amis.push(AmiFaux::avec_appareils(
        "bob",
        vec![AppareilDAmiFaux { id: 5, public_key: base64_de_test() }],
    ));

    let coffre = Coffre::pour_test("sky-test-annuaire-sync-appareil-ami");
    coffre.ranger_jetons(&sky_compte::Jetons { session: jeton, renouvellement: "peu-importe".to_string() }).unwrap();

    let etat = synchroniser(&Config::vers(&s.url()), &coffre, None).unwrap();
    assert_eq!(etat.amis[0].appareils.len(), 1);
    assert_eq!(etat.amis[0].appareils[0].id, 5);
    assert_eq!(etat.amis[0].appareils[0].public_key, base64_de_test());
}

#[test]
fn un_401_sur_sync_declenche_un_renouvellement_transparent() {
    // Preuve que `synchroniser` passe bien par `avec_jeton_valide` :
    // un refus ponctuel du jeton courant doit être absorbé par un
    // renouvellement automatique, invisible pour l'appelant.
    let s = FauxServeur::demarrer();
    s.etat_mut().code_natif_valide = Some(("CODEVALIDE".to_string(), "secret-correct".to_string()));

    let coffre = Coffre::pour_test("sky-test-annuaire-sync-renouvelle");
    let jetons = sky_compte::echanger_le_code(&Config::vers(&s.url()), "CODEVALIDE", "secret-correct").unwrap();
    coffre.ranger_jetons(&jetons).unwrap();

    s.etat_mut().refuser_le_premier_appel = true;

    let etat = synchroniser(&Config::vers(&s.url()), &coffre, None).unwrap();
    assert_eq!(etat.version, 0);
}

fn base64_de_test() -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD.encode([5u8; 32])
}
