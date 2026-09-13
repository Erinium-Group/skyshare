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
use sky_compte::{deposer, relever, synchroniser, AppareilDAmi, Coffre, Config, ErreurCompte, Jetons};
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
    let s = FauxServeur::demarrer();
    let coffre = coffre_pret("sky-test-boite-int-bonne-cle", &s, 1);
    let bob = Identity::generate();

    let n = deposer(&Config::vers(&s.url()), &coffre, &[appareil_de(9, &bob)], b"pour bob seul").unwrap();
    assert_eq!(n, 1);

    let etat = synchroniser(&Config::vers(&s.url()), &coffre, None).unwrap();
    let messages = relever(&etat, &bob);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].clair, b"pour bob seul");
}

#[test]
fn seul_le_destinataire_scelle_pour_peut_ouvrir_un_tiers_ne_peut_pas() {
    // Remplace le test du brief (`un_tiers_ne_peut_pas_ouvrir_ce_qui_ne_lui_est_pas_destine`)
    // qui recopiait mot pour mot un test de `sky-crypto` sans jamais passer
    // par `deposer`/`relever` — voir l'arbitrage de la tâche 9. Celui-ci
    // passe par les deux : dépôt réel via le double, relève réelle depuis
    // l'état renvoyé.
    let s = FauxServeur::demarrer();
    let coffre = coffre_pret("sky-test-boite-int-tiers", &s, 1);
    let bob = Identity::generate();
    let mallory = Identity::generate();

    let n = deposer(&Config::vers(&s.url()), &coffre, &[appareil_de(9, &bob)], b"offre secrete").unwrap();
    assert_eq!(n, 1);

    let etat = synchroniser(&Config::vers(&s.url()), &coffre, None).unwrap();

    let messages_bob = relever(&etat, &bob);
    assert_eq!(messages_bob.len(), 1);
    assert_eq!(messages_bob[0].clair, b"offre secrete");

    let messages_mallory = relever(&etat, &mallory);
    assert!(messages_mallory.is_empty(), "un tiers ne doit rien pouvoir ouvrir");
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
