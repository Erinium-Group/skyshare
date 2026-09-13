//! Tests d'intégration de la tâche 9 (la boîte aux lettres) qui pilotent le
//! serveur double : `deposer`, et l'extension de `http.rs` pour le 204 sans
//! corps. `relever` est une fonction PURE (aucun appel réseau) : ses tests
//! vivent en unité dans `boite.rs`, sauf le test bout en bout ci-dessous qui
//! passe par un vrai dépôt pour prouver que `deposer` et `relever`
//! s'accordent sur le même format d'enveloppe.
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use faux_serveur::FauxServeur;
use sky_compte::{
    deposer, enregistrer_appareil, relever, synchroniser, AppareilDAmi, Coffre, Config, ErreurCompte, Jetons,
};
use sky_crypto::Identity;

/// Prépare un coffre authentifié auprès du double ET porteur d'un
/// identifiant d'appareil local — les deux préalables qu'exige `deposer`
/// avant tout appel réseau.
fn coffre_pret(prefixe: &str, s: &FauxServeur, id_local: i64) -> Coffre {
    let coffre = Coffre::pour_test(prefixe);
    let jeton = s.jeton_de_test();
    coffre
        .ranger_jetons(&Jetons { session: jeton, renouvellement: "peu-importe".to_string() })
        .unwrap();
    coffre.ranger_identifiant_appareil(id_local).unwrap();
    coffre
}

fn appareil_de(id: i64, identite: &Identity) -> AppareilDAmi {
    AppareilDAmi { id, public_key: STANDARD.encode(identite.public_key()) }
}

/// Prépare un coffre authentifié auprès du double, porteur d'un appareil
/// RÉELLEMENT ENREGISTRÉ côté serveur (`POST /api/sky/devices`, donc
/// associé à `jeton` dans `EtatFaux::jetons_appareil`) — AJOUT DE LA RONDE
/// DE CORRECTION 1 : `coffre_pret` (au-dessus) range l'identifiant
/// UNIQUEMENT en local, sans jamais l'enregistrer côté serveur, donc ne
/// suffit plus pour un test qui doit RECEVOIR une enveloppe (le double ne
/// livre plus qu'à l'appareil associé, voir la revue de la tâche 9,
/// IMPORTANT 2). `jeton` doit être distinct par appareil — voir
/// `FauxServeur::jeton_de_test_pour`. Rend le coffre ET l'identifiant que
/// le serveur a réellement attribué.
fn coffre_avec_appareil_reel(prefixe: &str, s: &FauxServeur, jeton: String, identite: &Identity) -> (Coffre, i64) {
    let coffre = Coffre::pour_test(prefixe);
    coffre
        .ranger_jetons(&Jetons { session: jeton, renouvellement: "peu-importe".to_string() })
        .unwrap();
    let id = enregistrer_appareil(&Config::vers(&s.url()), &coffre, "Appareil de test", &identite.public_key())
        .unwrap();
    (coffre, id)
}

#[test]
fn le_204_du_double_est_traite_comme_un_succes_sans_decodage() {
    // NEUTRALISATION QUI COMPTE LE PLUS DE CETTE TÂCHE : si
    // `post_json_reponse_vide_avec_refus` décodait le corps d'un 2xx comme
    // le fait `post_json_avec_refus` (`rep.into_json::<()>()`), ce test
    // rougirait — un 204 n'a aucun corps à décoder, et un dépôt RÉUSSI
    // deviendrait `Err(ErreurCompte::Protocole(_))` au lieu d'un succès.
    let s = FauxServeur::demarrer();
    let jeton = s.jeton_de_test();
    let client = sky_compte::ClientHttp::new(&Config::vers(&s.url()));
    let corps = serde_json::json!({
        "expediteur_device_id": 1,
        "destinataire_device_id": 2,
        "charge": "YQ==",
    });

    let r: Result<sky_compte::ReponseHttp<()>, ErreurCompte> =
        client.post_json_reponse_vide_avec_refus("/api/sky/envelopes", &corps, Some(&jeton));

    assert!(matches!(r, Ok(sky_compte::ReponseHttp::Succes(()))), "attendu un succès");
}

#[test]
fn deposer_reussit_et_compte_le_depot() {
    let s = FauxServeur::demarrer();
    let coffre = coffre_pret("sky-test-boite-int-depose", &s, 1);
    let bob = Identity::generate();

    let n = deposer(&Config::vers(&s.url()), &coffre, &[appareil_de(9, &bob)], b"charge de test").unwrap();

    assert_eq!(n, 1);
    assert_eq!(s.etat_mut().depots_recus, 1);
}

#[test]
fn deposer_scelle_pour_la_bonne_cle_le_destinataire_seul_peut_ouvrir() {
    // NEUTRALISATION : si `deposer` scellait avec sa PROPRE clé (ou une clé
    // fixe) plutôt que celle du destinataire, `bob.open(...)` échouerait
    // ici alors que le dépôt lui-même aurait réussi (`n == 1`) — les deux
    // assertions discriminent des chemins de code différents.
    //
    // RONDE DE CORRECTION 1 (IMPORTANT 2, task-9-review.md) : A (le
    // déposant) et B (bob) sont désormais deux APPAREILS RÉELS, chacun son
    // jeton — B synchronise avec LE SIEN, pas avec celui du déposant. Avant
    // cette ronde, ce test synchronisait avec le MÊME coffre que celui qui
    // avait déposé : ça marchait uniquement parce que le double ne
    // filtrait encore rien par appareil, pas parce que « B a reçu son
    // enveloppe » était réellement prouvé.
    let s = FauxServeur::demarrer();
    let coffre_a = coffre_pret("sky-test-boite-int-bonne-cle-a", &s, 1);
    let bob = Identity::generate();
    let (coffre_b, id_b) =
        coffre_avec_appareil_reel("sky-test-boite-int-bonne-cle-b", &s, s.jeton_de_test_pour("bonne-cle-b"), &bob);

    let n = deposer(&Config::vers(&s.url()), &coffre_a, &[appareil_de(id_b, &bob)], b"pour bob seul").unwrap();
    assert_eq!(n, 1);

    let etat_b = synchroniser(&Config::vers(&s.url()), &coffre_b, None).unwrap();
    let messages = relever(&etat_b, &bob);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].clair, b"pour bob seul");
}

#[test]
fn deux_appareils_reels_seul_le_destinataire_recoit_et_un_jeton_sans_appareil_ne_recoit_rien() {
    // RONDE DE CORRECTION 1 (IMPORTANT 2, task-9-review.md) — remplace
    // `seul_le_destinataire_scelle_pour_peut_ouvrir_un_tiers_ne_peut_pas`,
    // qui synchronisait avec le MÊME coffre que celui qui avait déposé : le
    // double ne modélisait alors qu'un seul compte, jamais deux appareils
    // physiques séparés (réserve explicite du rapport de la tâche 9).
    //
    // Trois jetons distincts (deux appareils réels, un jeton sans appareil) :
    // A dépose pour B ; B
    // synchronise avec LE SIEN et ouvre l'enveloppe ; A, qui synchronise
    // avec le SIEN (un appareil réel, mais pas le destinataire), ne la voit
    // PAS ; un jeton SANS appareil associé ne reçoit jamais rien non plus.
    //
    // ORDRE IMPORTANT : A synchronise AVANT B. La livraison est
    // destructive — si B synchronisait en premier, l'enveloppe
    // disparaîtrait de `EtatFaux::enveloppes` AVANT que A ne synchronise,
    // et l'absence côté A prouverait seulement « déjà consommée », jamais
    // « filtrée par appareil ».
    let s = FauxServeur::demarrer();

    let alice = Identity::generate();
    let (coffre_a, _id_a) =
        coffre_avec_appareil_reel("sky-test-boite-int-deux-appareils-a", &s, s.jeton_de_test_pour("deux-appareils-a"), &alice);

    let bob = Identity::generate();
    let (coffre_b, id_b) =
        coffre_avec_appareil_reel("sky-test-boite-int-deux-appareils-b", &s, s.jeton_de_test_pour("deux-appareils-b"), &bob);

    let n = deposer(&Config::vers(&s.url()), &coffre_a, &[appareil_de(id_b, &bob)], b"offre secrete").unwrap();
    assert_eq!(n, 1);

    // A synchronise EN PREMIER (voir ORDRE IMPORTANT ci-dessus) : A est un
    // appareil réel et authentifié, mais pas le destinataire — l'enveloppe
    // ne doit PAS lui apparaître.
    let etat_a = synchroniser(&Config::vers(&s.url()), &coffre_a, None).unwrap();
    assert!(etat_a.enveloppes.is_empty(), "A ne doit jamais voir l'enveloppe destinee a B");

    // B synchronise ensuite avec SON jeton et ouvre l'enveloppe.
    let etat_b = synchroniser(&Config::vers(&s.url()), &coffre_b, None).unwrap();
    let messages_bob = relever(&etat_b, &bob);
    assert_eq!(messages_bob.len(), 1);
    assert_eq!(messages_bob[0].clair, b"offre secrete");

    // Un troisième jeton, SANS AUCUN appareil associé (aucun
    // `enregistrer_appareil` réussi avec lui), ne reçoit jamais rien —
    // même règle que le site pour une session sans application installée.
    let coffre_sans_appareil = Coffre::pour_test("sky-test-boite-int-deux-appareils-sans-appareil");
    coffre_sans_appareil
        .ranger_jetons(&Jetons {
            session: s.jeton_de_test_pour("deux-appareils-sans-appareil"),
            renouvellement: "peu-importe".to_string(),
        })
        .unwrap();
    let etat_sans_appareil = synchroniser(&Config::vers(&s.url()), &coffre_sans_appareil, None).unwrap();
    assert!(
        etat_sans_appareil.enveloppes.is_empty(),
        "un jeton sans appareil associe ne doit jamais recevoir d'enveloppe"
    );
}

#[test]
fn frontiere_de_taille_4048_accepte_4049_refuse_avant_le_reseau() {
    let s = FauxServeur::demarrer();
    let coffre = coffre_pret("sky-test-boite-int-frontiere", &s, 1);
    let bob = Identity::generate();

    let ok = deposer(&Config::vers(&s.url()), &coffre, &[appareil_de(9, &bob)], &vec![0u8; 4048]);
    assert!(ok.is_ok(), "4048 octets clairs (4096 une fois scellés) doivent passer : {ok:?}");
    assert_eq!(ok.unwrap(), 1);
    assert_eq!(s.etat_mut().depots_recus, 1);

    let refuse = deposer(&Config::vers(&s.url()), &coffre, &[appareil_de(9, &bob)], &vec![0u8; 4049]);
    assert!(matches!(refuse, Err(ErreurCompte::Protocole(_))));
    assert_eq!(s.etat_mut().depots_recus, 1, "aucun appel reseau pour 4049 octets clairs");
}

#[test]
fn un_404_pour_un_destinataire_compte_comme_non_livre_pas_une_erreur() {
    let s = FauxServeur::demarrer();
    let coffre = coffre_pret("sky-test-boite-int-404", &s, 1);
    s.etat_mut().refuser_depot = true;
    let bob = Identity::generate();

    let n = deposer(&Config::vers(&s.url()), &coffre, &[appareil_de(9, &bob)], b"charge").unwrap();

    assert_eq!(n, 0, "un 404 metier ne doit pas etre une erreur");
}

#[test]
fn deposer_ignore_une_cle_publique_illisible_sans_appel_reseau_pour_celle_la() {
    // Un appelant peut construire `AppareilDAmi` à la main (pas seulement
    // via `synchroniser`, qui valide déjà) : une clé tronquée doit être
    // écartée sans faire échouer tout le dépôt.
    let s = FauxServeur::demarrer();
    let coffre = coffre_pret("sky-test-boite-int-cle-illisible", &s, 1);
    let bob = Identity::generate();
    let cle_tronquee = STANDARD.encode([1u8; 16]); // 16 octets, pas 32.

    let n = deposer(
        &Config::vers(&s.url()),
        &coffre,
        &[AppareilDAmi { id: 9, public_key: cle_tronquee }, appareil_de(10, &bob)],
        b"charge",
    )
    .unwrap();

    assert_eq!(n, 1, "seul le destinataire a la cle valide doit compter");
    assert_eq!(s.etat_mut().depots_recus, 1, "un seul appel reseau, pour le destinataire valide");
}
