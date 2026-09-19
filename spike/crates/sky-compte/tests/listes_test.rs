//! Listes de diffusion (jalon 1, tâche 2) contre le serveur double. Chaque
//! test nomme la ligne du site qui lit le paramètre qu'il vérifie (leçon de
//! l'essai réel du 19/09/2026 : dériver le test du serveur, pas de la
//! mémoire de ce qu'on croit devoir envoyer).
// `allow(dead_code)` : chaque binaire n'utilise qu'une partie du double.
#[allow(dead_code)]
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

use faux_serveur::{AmiFaux, FauxServeur};
use sky_compte::{
    creer_liste, definir_membres, modifier_liste, supprimer_liste, synchroniser, ChampsListe, Coffre,
    Config, CreationListe, DefinitionMembres, ErreurCompte, Jetons, Liste, ModificationListe,
    SuppressionListe,
};

fn coffre_connecte(s: &FauxServeur, prefixe: &str) -> Coffre {
    let coffre = Coffre::pour_test(prefixe);
    coffre
        .ranger_jetons(&Jetons { session: s.jeton_de_test(), renouvellement: "peu-importe".to_string() })
        .unwrap();
    coffre
}

fn creee(r: Result<CreationListe, ErreurCompte>) -> Liste {
    match r {
        Ok(CreationListe::Creee(l)) => l,
        Ok(CreationListe::NomDejaPris) => panic!("nom déjà pris inattendu"),
        Err(e) => panic!("création refusée : {e}"),
    }
}

#[test]
fn creer_liste_envoie_nom_couleur_et_emoji_sous_les_cles_que_le_site_lit() {
    // Site : `const { nom, couleur, emoji } = corps` (lists/route.ts:80).
    // Neutralisation : renommer `couleur` en `color` dans `CorpsCreation` —
    // le double lit une couleur absente, la rend nulle : l'assertion rougit.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-creation");

    let liste = creee(creer_liste(&config, &coffre, "Copains", Some("#a1b2c3"), Some("🎮")));
    assert!(liste.membres.is_empty(), "la création ne rend jamais de membres");

    let etat = synchroniser(&config, &coffre, None).unwrap();
    assert_eq!(etat.listes.len(), 1);
    assert_eq!(etat.listes[0].id, liste.id);
    assert_eq!(etat.listes[0].nom, "Copains");
    assert_eq!(etat.listes[0].couleur.as_deref(), Some("#a1b2c3"));
    assert_eq!(etat.listes[0].emoji.as_deref(), Some("🎮"));
}

#[test]
fn un_nom_de_41_unites_utf16_est_refuse_avant_le_reseau_et_40_passe() {
    // Site : `valeur.length <= NOM_MAX` (40), en unités UTF-16
    // (lists/route.ts:21-22). 20 émojis = 40 unités mais 20 `char` ; un « a »
    // de plus = 41 unités, 21 `char`. Neutralisation : mesurer en
    // `chars().count()` — le client laisse passer, le double répond 400,
    // `appels_listes` vaut 1 au lieu de 0.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-nom-41");
    let nom_40 = "😀".repeat(20);
    let nom_41 = format!("{nom_40}a");

    assert!(matches!(creer_liste(&config, &coffre, &nom_41, None, None), Err(ErreurCompte::Protocole(_))));
    assert_eq!(s.etat_mut().appels_listes, 0, "refusé avant tout appel au site");
    creee(creer_liste(&config, &coffre, &nom_40, None, None));
}

#[test]
fn un_emoji_de_9_octets_est_refuse_avant_le_reseau_et_8_passe() {
    // Site : `Buffer.byteLength(valeur, "utf8") <= EMOJI_OCTETS_MAX` (8),
    // lists/route.ts:56. « 🇫🇷 » : deux indicateurs régionaux de 4 octets.
    // Neutralisation : compter en `chars()` — « 🇫🇷a » (3 `char`) passe le
    // client, le double répond 400, `appels_listes` vaut 1.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-emoji");
    let drapeau = "🇫🇷";
    assert_eq!(drapeau.len(), 8);
    let neuf_octets = format!("{drapeau}a");

    assert!(matches!(
        creer_liste(&config, &coffre, "Drapeaux", None, Some(&neuf_octets)),
        Err(ErreurCompte::Protocole(_))
    ));
    assert_eq!(s.etat_mut().appels_listes, 0);
    creee(creer_liste(&config, &coffre, "Drapeaux", None, Some(drapeau)));
}

#[test]
fn une_couleur_hors_rrggbb_est_refusee_avant_le_reseau() {
    // Site : `/^#[0-9A-Fa-f]{6}$/` (lists/route.ts:37). Neutralisation :
    // n'exiger que le `#` initial — « #12345G » part, `appels_listes` > 0.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-couleur");
    for mauvaise in ["#12345G", "123456", "#1234567", "#12345"] {
        assert!(
            matches!(creer_liste(&config, &coffre, "C", Some(mauvaise), None), Err(ErreurCompte::Protocole(_))),
            "couleur {mauvaise}"
        );
    }
    assert_eq!(s.etat_mut().appels_listes, 0);
}

#[test]
fn un_nom_deja_pris_rend_nom_deja_pris() {
    // Site : 409 « Nom de liste deja utilise » (lists/route.ts:112-114).
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-409");
    creee(creer_liste(&config, &coffre, "Jeu", None, None));
    assert!(matches!(creer_liste(&config, &coffre, "Jeu", None, None), Ok(CreationListe::NomDejaPris)));
}

#[test]
fn modifier_n_envoie_que_les_champs_fournis_et_null_remet_a_rien() {
    // Site : `"couleur" in donnees` (lists/[id]/route.ts:94) — clé absente =
    // inchangée, `null` = remise à rien. Neutralisation : sérialiser un champ
    // absent en `null` (`"nom": null`) — le double refuse le nom nul (400),
    // l'appel rend une erreur de protocole au lieu de `Modifiee`.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-modifier");
    let liste = creee(creer_liste(&config, &coffre, "Soirée", Some("#FF00AA"), Some("🎮")));

    let champs = ChampsListe { couleur: Some(None), ..ChampsListe::default() };
    assert_eq!(modifier_liste(&config, &coffre, liste.id, &champs).unwrap(), ModificationListe::Modifiee);

    let etat = synchroniser(&config, &coffre, None).unwrap();
    assert_eq!(etat.listes[0].nom, "Soirée");
    assert_eq!(etat.listes[0].couleur, None);
    assert_eq!(etat.listes[0].emoji.as_deref(), Some("🎮"));
}

#[test]
fn modifier_ou_supprimer_une_liste_inconnue_rend_introuvable() {
    // Site : 404 « Liste introuvable » (lists/[id]/route.ts:124, 158).
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-inconnue");
    let champs = ChampsListe { nom: Some("x"), ..ChampsListe::default() };
    assert_eq!(modifier_liste(&config, &coffre, 999, &champs).unwrap(), ModificationListe::Introuvable);
    assert_eq!(supprimer_liste(&config, &coffre, 999).unwrap(), SuppressionListe::Introuvable);
}

#[test]
fn supprimer_retire_la_liste_d_une_synchronisation_complete() {
    // La suppression ne fait pas forcément progresser la version (etat.ts) :
    // seule une synchronisation SANS précédent la voit à coup sûr.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-supprimer");
    let a = creee(creer_liste(&config, &coffre, "A", None, None));
    let b = creee(creer_liste(&config, &coffre, "B", None, None));
    assert_eq!(supprimer_liste(&config, &coffre, a.id).unwrap(), SuppressionListe::Supprimee);

    let etat = synchroniser(&config, &coffre, None).unwrap();
    assert_eq!(etat.listes.iter().map(|l| l.id).collect::<Vec<_>>(), vec![b.id]);
}

#[test]
fn definir_membres_envoie_membre_ids_et_la_synchronisation_les_relit() {
    // Site : `const { membreIds } = corps` (members/route.ts:54).
    // Neutralisation : renommer la clé en `membres` — le double répond 400,
    // l'appel rend `Refuses`.
    let s = FauxServeur::demarrer();
    s.etat_mut().amis.push(AmiFaux::sans_appareil("bob"));
    s.etat_mut().amis.push(AmiFaux::sans_appareil("carole"));
    let (bob, carole) = { let e = s.etat_mut(); (e.amis[0].id, e.amis[1].id) };
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-membres");
    let liste = creee(creer_liste(&config, &coffre, "Équipe", None, None));

    assert_eq!(definir_membres(&config, &coffre, liste.id, &[carole, bob, bob]).unwrap(), DefinitionMembres::Definis);

    let etat = synchroniser(&config, &coffre, None).unwrap();
    let mut attendus = vec![bob, carole];
    attendus.sort_unstable();
    assert_eq!(etat.listes[0].membres, attendus);
}

#[test]
fn un_membre_qui_n_est_pas_ami_fait_refuser_sans_rien_ecrire() {
    // Site : 400 uniforme (members/route.ts:83). Rien n'est écrit.
    let s = FauxServeur::demarrer();
    s.etat_mut().amis.push(AmiFaux::sans_appareil("bob"));
    let bob = s.etat_mut().amis[0].id;
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-non-ami");
    let liste = creee(creer_liste(&config, &coffre, "L", None, None));

    assert_eq!(definir_membres(&config, &coffre, liste.id, &[bob, 424242]).unwrap(), DefinitionMembres::Refuses);
    let etat = synchroniser(&config, &coffre, None).unwrap();
    assert!(etat.listes[0].membres.is_empty());
}

#[test]
fn plus_de_200_membres_sont_refuses_avant_le_reseau() {
    // Site : `valeur.length <= MEMBRES_MAX` (members/route.ts:10).
    // Neutralisation : retirer le contrôle de longueur de `membres_valides` —
    // la requête part (`appels_listes` passe à 2) et le double répond 400.
    let s = FauxServeur::demarrer();
    let config = Config::vers(&s.url());
    let coffre = coffre_connecte(&s, "sky-test-listes-201");
    let liste = creee(creer_liste(&config, &coffre, "L", None, None));
    let trop: Vec<i64> = (1..=201).collect();

    assert!(matches!(definir_membres(&config, &coffre, liste.id, &trop), Err(ErreurCompte::Protocole(_))));
    assert_eq!(s.etat_mut().appels_listes, 1, "seule la création a atteint le site");
}
