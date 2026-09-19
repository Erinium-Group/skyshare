//! La boîte aux lettres : dépôt d'une charge scellée pour les appareils d'un
//! ami, et relève des enveloppes reçues.
//!
//! `deposer` est le SEUL point d'entrée authentifié de ce module — il passe
//! par `avec_jeton_valide` (`session.rs`), comme tout appel authentifié de
//! ce crate. `relever`, elle, n'appelle jamais le réseau : voir sa
//! documentation pour la raison.

use std::fmt;

use serde::Serialize;
use sky_crypto::Identity;

use crate::annuaire::{AppareilDAmi, Etat};
use crate::coffre::Coffre;
use crate::erreur::ErreurCompte;
use crate::http::{ClientHttp, Config, ReponseHttp};
use crate::session::avec_jeton_valide;

/// Surcoût du scellage d'une charge par `Identity::seal` : une clé éphémère
/// de 32 octets et un tag d'authentification de 16 octets — voir
/// `sky_crypto::tests::surcout_borne`, et son miroir plus bas
/// (`le_surcout_de_scellement_correspond_a_la_constante`), qui doit rougir
/// si ce nombre change un jour côté `sky-crypto`.
const SURCOUT_SCELLEMENT: usize = 48;

/// Taille maximale d'une charge SCELLÉE que le serveur accepte
/// (`envelopes_taille` en base, `TAILLE_CHARGE_MAX` côté site,
/// `src/lib/sky/enveloppes.ts`).
const TAILLE_MAX_CHARGE_SCELLEE: usize = 4096;

/// Taille maximale d'une charge CLAIRE que `deposer` accepte.
///
/// Le serveur refuse au-delà de `TAILLE_MAX_CHARGE_SCELLEE` octets DÉCODÉS ;
/// le scellage en ajoute `SURCOUT_SCELLEMENT`. Un contrôle `len() > 4096`
/// sur le CLAIR laisserait donc passer 4050 octets, qui en font 4098 une
/// fois scellés — et le serveur les refuserait par son 404 uniforme, sans
/// jamais dire pourquoi. Ce client est le SEUL à pouvoir donner un message
/// clair : la borne doit donc porter sur le clair, déjà réduite du surcoût.
pub const TAILLE_MAX_CLAIR: usize = TAILLE_MAX_CHARGE_SCELLEE - SURCOUT_SCELLEMENT;

/// Un message déchiffré, tel que rendu par `relever`.
///
/// `clair` porte l'offre ou la réponse SDP d'une connexion — adresses IP
/// comprises. Ne dérive **pas** `Debug` : même discipline que `Jetons`
/// (`coffre.rs`), pour la même raison — un `panic!`, un `assert_eq!` échoué
/// ou un `{:?}` de débogage sur cette valeur ne doit jamais écrire une
/// charge en clair, ce que le projet promet de ne jamais journaliser.
/// L'implémentation manuelle ci-dessous montre `id` et
/// `expediteur_device_id` (sans risque, ce sont des identifiants opaques)
/// et la LONGUEUR de `clair`, jamais son contenu.
#[derive(Clone, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    pub expediteur_device_id: i64,
    pub clair: Vec<u8>,
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Message")
            .field("id", &self.id)
            .field("expediteur_device_id", &self.expediteur_device_id)
            .field("clair_longueur_octets", &self.clair.len())
            .finish()
    }
}

#[derive(Serialize)]
struct CorpsDepot<'a> {
    expediteur_device_id: i64,
    destinataire_device_id: i64,
    charge: &'a str,
}

/// Décode `valeur` en base64 standard et exige exactement 32 octets en
/// forme canonique (aller-retour identique) — même contrôle que
/// `annuaire::cle_publique_valide`, dupliqué ici parce que celle-ci ne rend
/// qu'un booléen et que `deposer` a besoin des OCTETS décodés. Défensif, pas
/// redondant : un appelant peut construire un `AppareilDAmi` à la main,
/// sans passer par `synchroniser` (qui valide déjà cette forme).
fn decoder_cle_publique(valeur: &str) -> Option<[u8; 32]> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let octets = STANDARD.decode(valeur).ok()?;
    if octets.len() != 32 || STANDARD.encode(&octets) != valeur {
        return None;
    }
    octets.try_into().ok()
}

/// Dépose `charge_claire` pour chaque appareil de `destinataires`, scellée
/// UNE FOIS PAR DESTINATAIRE avec sa clé publique propre — `Identity::seal`
/// tire une paire éphémère à chaque appel, donc deux scellages du même
/// message diffèrent toujours, même vers le même destinataire.
///
/// Lit l'identifiant d'appareil local dans `coffre` (rangé par
/// `enregistrer_appareil`) : absent, l'appel échoue AVANT tout accès réseau
/// — sans lui, `expediteur_device_id` n'a aucune valeur à envoyer.
///
/// Rend le NOMBRE de dépôts ACCEPTÉS, pas le nombre de destinataires visés :
/// un 404 pour un destinataire donné (appareil révoqué entre-temps, amitié
/// retirée, clé publique illisible construite à la main) COMPTE comme non
/// livré, PAS comme une erreur — les autres destinataires sont toujours
/// tentés. Une erreur de transport ou un refus d'authentification (401,
/// même après la reprise unique d'`avec_jeton_valide`), eux, restent des
/// erreurs et interrompent la boucle : ni l'un ni l'autre ne dit rien du
/// destinataire visé, contrairement à un 404 métier.
pub fn deposer(
    config: &Config,
    coffre: &Coffre,
    destinataires: &[AppareilDAmi],
    charge_claire: &[u8],
) -> Result<usize, ErreurCompte> {
    if charge_claire.len() > TAILLE_MAX_CLAIR {
        return Err(ErreurCompte::Protocole(format!(
            "charge trop grande : {} octets clairs, maximum {} (le scellage ajoute {} octets à la \
             limite serveur de {} octets)",
            charge_claire.len(),
            TAILLE_MAX_CLAIR,
            SURCOUT_SCELLEMENT,
            TAILLE_MAX_CHARGE_SCELLEE,
        )));
    }

    let mon_id = coffre.identifiant_appareil()?.ok_or_else(|| {
        ErreurCompte::Protocole(
            "aucun identifiant d'appareil local — appelez enregistrer_appareil avant de deposer"
                .to_string(),
        )
    })?;

    let identite = coffre.identite()?;
    let client = ClientHttp::new(config);
    let mut acceptes = 0usize;

    for destinataire in destinataires {
        let Some(cle) = decoder_cle_publique(&destinataire.public_key) else {
            // Clé illisible : ce destinataire ne peut structurellement pas
            // recevoir cette enveloppe — pas une erreur de `deposer`, un
            // destinataire non livré comme un autre.
            continue;
        };

        let scelle = identite.seal(&cle, charge_claire);
        let charge_b64 = {
            use base64::engine::general_purpose::STANDARD;
            use base64::Engine;
            STANDARD.encode(&scelle)
        };
        let corps = CorpsDepot {
            expediteur_device_id: mon_id,
            destinataire_device_id: destinataire.id,
            charge: &charge_b64,
        };

        let issue: ReponseHttp<()> = avec_jeton_valide(config, coffre, |jeton| {
            client.post_json_reponse_vide_avec_refus("/api/sky/envelopes", &corps, Some(jeton))
        })?;

        match issue {
            ReponseHttp::Succes(()) => acceptes += 1,
            ReponseHttp::Refus { statut: 404, .. } => {
                // Refus sémantique UNIFORME du serveur (pas_ami,
                // appareil_inconnu, pas_mon_appareil, ou trop_gros côté
                // base) : non livré, pas une erreur — voir la doc de
                // `deposer`.
            }
            ReponseHttp::Refus { statut, corps } => {
                return Err(ErreurCompte::Protocole(format!("statut {statut} inattendu : {corps}")));
            }
        }
    }

    Ok(acceptes)
}

/// Ouvre les enveloppes de `etat` avec `identite` — fonction PURE, AUCUN
/// appel réseau.
///
/// `synchroniser` (`annuaire.rs`) est le SEUL canal par lequel une enveloppe
/// arrive : le serveur l'efface en la livrant. Un `relever` qui ferait son
/// propre appel réseau volerait les enveloppes du prochain `synchroniser`
/// (et inversement, un `synchroniser` qui suivrait volerait celles du
/// prochain `relever`) — les deux doivent donc composer sur le MÊME `Etat`,
/// jamais interroger le serveur chacun de son côté.
///
/// Une enveloppe qui ne s'ouvre pas est IGNORÉE EN SILENCE. Ce n'est JAMAIS
/// parce qu'elle serait destinée à un autre appareil : le serveur ne livre
/// QUE les enveloppes du device_id courant (`deviceIdCourant`, côté site,
/// `sync/route.ts`) — cette cause est structurellement exclue ici. Les
/// vraies raisons : corruption en transit, altération délibérée, ou une clé
/// d'appareil qui a changé depuis le dépôt (ré-enregistrement entre-temps).
pub fn relever(etat: &Etat, identite: &Identity) -> Vec<Message> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let mut messages = Vec::new();
    for enveloppe in &etat.enveloppes {
        let Ok(scelle) = STANDARD.decode(&enveloppe.charge) else {
            continue;
        };
        let Ok(clair) = identite.open(&scelle) else {
            continue;
        };
        messages.push(Message {
            id: enveloppe.id.clone(),
            expediteur_device_id: enveloppe.expediteur_device_id,
            clair,
        });
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annuaire::EnveloppeRecue;

    fn appareil_de_test(id: i64, cle: [u8; 32]) -> AppareilDAmi {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        AppareilDAmi { id, public_key: STANDARD.encode(cle) }
    }

    fn etat_avec(enveloppes: Vec<EnveloppeRecue>) -> Etat {
        Etat { version: 1, code: "X".to_string(), amis: Vec::new(), demandes: Vec::new(), listes: Vec::new(), appareils: Vec::new(), enveloppes }
    }

    #[test]
    fn le_surcout_de_scellement_correspond_a_la_constante() {
        // Si `sky-crypto` changeait un jour son surcoût (clé éphémère ou
        // tag d'authentification), CE test doit rougir avant que
        // `TAILLE_MAX_CLAIR` ne mente silencieusement sur ce que le serveur
        // accepte réellement une fois scellé.
        let alice = Identity::generate();
        let bob = Identity::generate();
        let clair = vec![0u8; 123];
        let scelle = alice.seal(&bob.public_key(), &clair);
        assert_eq!(scelle.len(), clair.len() + SURCOUT_SCELLEMENT);
    }

    #[test]
    fn une_charge_trop_grosse_est_refusee_avant_le_reseau() {
        // Port 1 en local : refuse toute connexion immédiatement (même
        // technique que `http.rs::aucun_jeton_dans_lechec_reseau_reel`). Si
        // la garde de taille manquait, l'erreur serait `Reseau`, jamais
        // `Protocole` — c'est la VARIANTE qui prouve qu'aucun appel n'a été
        // tenté, pas seulement l'échec en général.
        let coffre = Coffre::pour_test("sky-test-boite-taille-unitaire");
        coffre.ranger_identifiant_appareil(1).unwrap();

        let r = deposer(
            &Config::vers("http://127.0.0.1:1"),
            &coffre,
            &[appareil_de_test(9, [7u8; 32])],
            &vec![0u8; TAILLE_MAX_CLAIR + 1],
        );
        assert!(matches!(r, Err(ErreurCompte::Protocole(_))));
    }

    #[test]
    fn deposer_sans_identifiant_local_est_refuse_avant_le_reseau() {
        let coffre = Coffre::pour_test("sky-test-boite-sans-id-local");

        let r = deposer(
            &Config::vers("http://127.0.0.1:1"),
            &coffre,
            &[appareil_de_test(9, [7u8; 32])],
            b"charge",
        );
        assert!(matches!(r, Err(ErreurCompte::Protocole(_))));
    }

    #[test]
    fn relever_ignore_une_enveloppe_illisible_et_garde_les_autres() {
        // NEUTRALISATION CIBLÉE : si `relever` ne sautait plus les
        // enveloppes illisibles (ex. `.unwrap()` au lieu du `let...else`),
        // ce test paniquerait au lieu de rendre `messages.len() == 1` — les
        // trois causes d'échec (mauvaise clé, base64 invalide) sont
        // représentées séparément pour ne pas dépendre d'une seule.
        let bob = Identity::generate();
        let mallory = Identity::generate();

        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let pas_pour_bob = Identity::generate().seal(&mallory.public_key(), b"pas pour bob");
        let pour_bob = Identity::generate().seal(&bob.public_key(), b"pour bob");

        let etat = etat_avec(vec![
            EnveloppeRecue {
                id: "1".to_string(),
                expediteur_device_id: 1,
                destinataire_device_id: 2,
                charge: STANDARD.encode(&pas_pour_bob),
            },
            EnveloppeRecue {
                id: "2".to_string(),
                expediteur_device_id: 3,
                destinataire_device_id: 2,
                charge: STANDARD.encode(&pour_bob),
            },
            EnveloppeRecue {
                id: "3".to_string(),
                expediteur_device_id: 4,
                destinataire_device_id: 2,
                charge: "pas du base64 valide !!".to_string(),
            },
        ]);

        let messages = relever(&etat, &bob);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "2");
        assert_eq!(messages[0].expediteur_device_id, 3);
        assert_eq!(messages[0].clair, b"pour bob");
    }

    #[test]
    fn relever_rend_un_vecteur_vide_sans_enveloppe() {
        let identite = Identity::generate();
        let etat = etat_avec(Vec::new());
        assert!(relever(&etat, &identite).is_empty());
    }

    #[test]
    fn le_debug_de_message_naffiche_pas_le_clair() {
        // RONDE DE CORRECTION 1, IMPORTANT 1 : `clair` porte une offre ou
        // une réponse SDP (adresses IP comprises) — un `{:?}` ne doit
        // jamais l'écrire, même discipline que `le_debug_de_jetons_...`
        // dans `coffre.rs`.
        //
        // DEUX ASSERTIONS, POUR DEUX FUITES DIFFÉRENTES : un `clair` texte
        // fuiterait tel quel (`contains`) ; un `#[derive(Debug)]` NAÏF sur
        // `Vec<u8>` ne réimprime PAS le texte littéral — il énumère les
        // octets en décimal (`[84, 69, 77, ...]`), ce que `contains` seul
        // ne détecterait PAS. La borne de longueur ci-dessous couvre CE
        // cas : 4200 octets de clair énumérés un par un pèseraient
        // largement plus que 300 caractères, alors que le masquage voulu
        // n'affiche qu'un nombre. Neutralisation : remettre
        // `#[derive(Debug, ...)]` sur `Message` fait rougir CE test sur la
        // borne de longueur, et lui seul.
        const MOTIF: &str = "TEMOIN-SDP-EN-CLAIR-avec-IP-203.0.113.42-";
        let clair: Vec<u8> = MOTIF.repeat(100).into_bytes(); // ~4200 octets.
        let message = Message { id: "1".to_string(), expediteur_device_id: 7, clair: clair.clone() };

        let debug = format!("{message:?}");

        assert!(!debug.contains(MOTIF), "le rendu Debug a laissé fuir le clair en texte : {debug}");
        assert!(
            debug.len() < 300,
            "le rendu Debug ({} caracteres) est bien plus long que necessaire pour {} octets de \
             clair masqué — il les énumère probablement un par un : {debug}",
            debug.len(),
            clair.len(),
        );
    }
}
