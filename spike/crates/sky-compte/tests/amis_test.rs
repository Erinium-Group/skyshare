//! Amis et compte (jalon 1, tâche 3) contre le serveur double : retirer,
//! bloquer, régénérer le code, révoquer un appareil.
#[allow(dead_code)]
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

use faux_serveur::{AmiFaux, FauxServeur};
use sky_compte::{
    bloquer_ami, enregistrer_appareil, regenerer_code, retirer_ami, revoquer_appareil, synchroniser,
    Blocage, Coffre, Config, ErreurCompte, Jetons, Retrait,
};

fn coffre_connecte(s: &FauxServeur, prefixe: &str) -> Coffre {
    let coffre = Coffre::pour_test(prefixe);
    coffre
        .ranger_jetons(&Jetons { session: s.jeton_de_test(), renouvellement: "peu-importe".to_string() })
        .unwrap();
    coffre
}

/// Un ami dont l'identifiant d'AMITIÉ diffère de son identifiant
/// d'utilisateur : `AmiFaux::sans_appareil` leur donne la même valeur, ce qui
/// rendrait invisible une confusion entre les deux.
fn ami_d_amitie(nom: &str, friendship_id: i64) -> AmiFaux {
    let mut ami = AmiFaux::sans_appareil(nom);
    ami.friendship_id = friendship_id;
    ami
}

#[test]
fn la_synchronisation_lit_friendship_id_distinct_de_l_identifiant() {
    // Site : `f.id AS "friendshipId"` (amisDe, amis.ts:541). Neutralisation :
    // `#[serde(rename = "id")]` sur `friendship_id` d'`AmiBrut` — rougit.
    let s = FauxServeur::demarrer();
    s.etat_mut().amis.push(ami_d_amitie("bob", 777));
    let coffre = coffre_connecte(&s, "sky-test-amis-friendship-id");
    let etat = synchroniser(&Config::vers(&s.url()), &coffre, None).unwrap();
    assert_eq!(etat.amis[0].friendship_id, 777);
    assert_ne!(etat.amis[0].id, 777);
}

#[test]
fn retirer_ami_vise_l_identifiant_d_amitie_que_la_route_lit() {
    // Site : `const friendshipId = Number(id)` (friends/[id]/route.ts:17).
    // Neutralisation : construire le chemin avec 1 + friendship_id — le
    // double rend 404, l'appel rend `Introuvable`.
    let s = FauxServeur::demarrer();
    s.etat_mut().amis.push(ami_d_amitie("bob", 777));
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-amis-retirer");

    assert_eq!(retirer_ami(&config, &coffre, 777).unwrap(), Retrait::Retire);
    assert!(synchroniser(&config, &coffre, None).unwrap().amis.is_empty());
    assert_eq!(retirer_ami(&config, &coffre, 777).unwrap(), Retrait::Introuvable);
}

#[test]
fn un_identifiant_d_amitie_nul_ou_negatif_est_refuse_avant_le_reseau() {
    // Site : `!Number.isInteger(friendshipId) || friendshipId <= 0` → 400
    // (friends/[id]/route.ts:18). Neutralisation : retirer la garde — les
    // requêtes partent, `appels_amis` vaut 2.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-amis-identifiant");
    assert!(matches!(retirer_ami(&config, &coffre, 0), Err(ErreurCompte::Protocole(_))));
    assert!(matches!(bloquer_ami(&config, &coffre, -3), Err(ErreurCompte::Protocole(_))));
    assert_eq!(s.etat_mut().appels_amis, 0);
}

#[test]
fn bloquer_ami_poste_sur_block_et_retire_l_ami() {
    // Site : `POST /api/sky/friends/{id}/block` → 200 `{ ok: true }`
    // (block/route.ts:29). Neutralisation : poster sur `/accept` — le double
    // rend 404 (amitié non acceptable), l'appel rend `Introuvable`.
    let s = FauxServeur::demarrer();
    s.etat_mut().amis.push(ami_d_amitie("bob", 778));
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-amis-bloquer");
    assert_eq!(bloquer_ami(&config, &coffre, 778).unwrap(), Blocage::Bloque);
    assert!(synchroniser(&config, &coffre, None).unwrap().amis.is_empty());
    assert_eq!(bloquer_ami(&config, &coffre, 778).unwrap(), Blocage::Introuvable);
}

#[test]
fn regenerer_code_rend_le_code_que_la_synchronisation_complete_relit() {
    // Site : `return NextResponse.json({ code })` (friend-code/route.ts:42).
    // Neutralisation : lire un champ `nouveau_code` — erreur de protocole.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-amis-code");
    let avant = synchroniser(&config, &coffre, None).unwrap().code;
    let code = regenerer_code(&config, &coffre).unwrap();
    assert_ne!(code, avant);
    assert_eq!(synchroniser(&config, &coffre, None).unwrap().code, code);
}

#[test]
fn revoquer_appareil_revoque_un_autre_appareil_et_accepte_un_inconnu() {
    // Deux enregistrements sur la même session : le second détache le
    // premier (`enregistrerAppareil`), dont la révocation ne coupe donc pas
    // la session du test.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-amis-revoquer");
    let ancien = enregistrer_appareil(&config, &coffre, "Ancien PC", &[3u8; 32]).unwrap();
    let _courant = enregistrer_appareil(&config, &coffre, "PC", &[4u8; 32]).unwrap();

    revoquer_appareil(&config, &coffre, ancien).unwrap();
    let etat = synchroniser(&config, &coffre, None).unwrap();
    let revoque = etat.appareils.iter().find(|a| a.id == ancien).unwrap();
    assert!(revoque.revoked_at.is_some());
    revoquer_appareil(&config, &coffre, 999).unwrap();
}
