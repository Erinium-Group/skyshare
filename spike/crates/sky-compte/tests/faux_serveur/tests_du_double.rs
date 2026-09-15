//! Tests propres au serveur double — ceux qui visent le double lui-même, en HTTP
//! brut, sans passer par `sky_compte`.
//!
//! Inclus par `faux_serveur_test.rs` SEULEMENT (revue finale, m1). Ils vivaient
//! auparavant dans un `mod tests` de `faux_serveur/mod.rs` : chacun des cinq binaires
//! qui incluent le double par `#[path]` les compilait et les exécutait — 38 tests
//! exécutés cinq fois, 351 exécutions pour 199 tests distincts, et des totaux
//! comparés comme s'ils mesuraient la couverture. Déplacés à l'octet près (texte
//! recollé identique à l'original, vérifié), seul l'import ci-dessous a changé.
#[cfg(test)]
mod tests {
    use crate::faux_serveur::*;
    use base64::Engine;
    use serde_json::json;

    // « Qu'est-ce qui, précisément, ferait échouer ce test ? » Réponse pour
    // chacun des tests ci-dessous en commentaire, avec la neutralisation
    // pratiquée pour le prouver — voir task-4-report.md pour le relevé
    // complet des neutralisations et de leur rougissement.

    /// Enregistre un appareil via `POST /api/sky/devices` avec `jeton`, en
    /// HTTP brut (pas via `sky_compte`, ces tests visent le double
    /// directement) — AJOUT DE LA RONDE DE CORRECTION 1 (IMPORTANT 2) :
    /// plusieurs tests de ce module doivent maintenant faire correspondre
    /// un jeton à un appareil AVANT de vérifier qu'une enveloppe le
    /// rejoint, puisque `gerer_sync` ne livre plus qu'à l'appareil associé
    /// (`EtatFaux::jetons_appareil`). Rend l'identifiant attribué par le
    /// double, à utiliser comme `destinataire_device_id`.
    fn enregistrer_appareil_http(s: &FauxServeur, jeton: &str) -> i64 {
        let cle = base64::engine::general_purpose::STANDARD.encode([5u8; 32]);
        let recu: serde_json::Value = ureq::post(&format!("{}/api/sky/devices", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&format!(r#"{{"publicKey":"{cle}","nom":"Appareil de test","plateforme":"windows"}}"#))
            .expect("enregistrement d'appareil attendu en succes dans ce test")
            .into_json()
            .expect("reponse JSON attendue");
        recu["id"].as_i64().expect("id attendu comme entier")
    }

    #[test]
    fn le_double_rend_un_tableau_vide_jamais_absent() {
        // Échoue si `appareils` est sérialisé absent (Option::None -> champ
        // manquant) plutôt que `[]`, ou si AmiFaux ne construit pas un
        // tableau vide par défaut.
        let s = FauxServeur::demarrer();
        s.etat_mut().amis.push(AmiFaux::sans_appareil("bob"));
        let jeton = s.jeton_de_test();

        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();

        assert_eq!(recu["amis"][0]["appareils"], serde_json::json!([]));
    }

    #[test]
    fn le_double_refuse_sans_distinguer() {
        // Les quatre causes d'echec rendent la MEME reponse, comme le vrai.
        // Échoue si le double distingue "code inconnu" d'un autre refus, ou
        // s'il répond autre chose que 401 / ce corps exact.
        let s = FauxServeur::demarrer();
        for corps in [r#"{"code":"inconnu","secret":"x"}"#, r#"{"code":"","secret":""}"#] {
            let e = ureq::post(&format!("{}/api/auth/native", s.url()))
                .send_string(corps)
                .unwrap_err();
            let reponse = match e {
                ureq::Error::Status(_, r) => r,
                _ => panic!("attendu 401"),
            };
            assert_eq!(reponse.status(), 401);
            assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Code refuse"}"#);
        }
    }

    #[test]
    fn chaque_serveur_double_choisit_un_port_different() {
        // Échoue si `demarrer()` écoutait sur un port fixe : deux instances
        // en parallèle se disputeraient alors le même port, et l'une des
        // deux échouerait à démarrer plutôt que de rendre une URL distincte.
        let a = FauxServeur::demarrer();
        let b = FauxServeur::demarrer();
        assert_ne!(a.url(), b.url());
    }

    #[test]
    fn refuser_le_premier_appel_ne_refuse_que_le_premier() {
        // Échoue si le refus n'est pas consommé (tous les appels
        // refuseraient) ou s'il est consommé trop tôt (aucun appel ne
        // refuserait). Jeton reconnu attaché aux deux appels : le refus
        // testé ici doit venir de `refuser_le_premier_appel`, jamais d'un
        // jeton absent ou inconnu (voir `sync_exige_un_jeton` pour cette
        // autre cause).
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().refuser_le_premier_appel = true;

        let premier = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call();
        let reponse_premier = premier.unwrap_err().into_response().unwrap();
        assert_eq!(reponse_premier.status(), 401);
        assert_eq!(reponse_premier.into_string().unwrap(), r#"{"error":"Acces refuse"}"#);

        let second = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call();
        assert_eq!(second.unwrap().status(), 200);
    }

    #[test]
    fn sync_exige_un_jeton() {
        // Échoue si `GET /api/sky/sync` répondait avec succès sans aucun
        // en-tête `Authorization` — exactement la réserve fermée par cette
        // ronde de correction : un client qui oublierait son jeton ne doit
        // plus jamais passer contre ce double.
        let s = FauxServeur::demarrer();
        let reponse = ureq::get(&format!("{}/api/sky/sync", s.url())).call();
        let reponse = reponse.unwrap_err().into_response().unwrap();
        assert_eq!(reponse.status(), 401);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Non authentifie"}"#);
    }

    #[test]
    fn sync_refuse_un_jeton_inconnu() {
        // Échoue si un jeton quelconque, jamais délivré par ce double,
        // était accepté — distinct du test précédent (jeton ABSENT) : ici
        // un en-tête est bien présent, mais ne correspond à rien.
        let s = FauxServeur::demarrer();
        let reponse = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", "Bearer nimporte-quoi")
            .call();
        let reponse = reponse.unwrap_err().into_response().unwrap();
        assert_eq!(reponse.status(), 401);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Non authentifie"}"#);
    }

    #[test]
    fn sync_accepte_un_jeton_reconnu() {
        // Échoue si un jeton pourtant présent dans `jetons_acceptes`
        // était quand même refusé — la contre-preuve des deux tests
        // précédents : le rejet vient bien de la reconnaissance du jeton,
        // pas d'un refus systématique de toute authentification.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let reponse = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call();
        assert_eq!(reponse.unwrap().status(), 200);
    }

    #[test]
    fn refuser_tout_refuse_aussi_le_renouvellement() {
        // Échoue si `refuser_tout` laissait passer `POST /api/auth/refresh`
        // — exactement la distinction avec `refuser_le_premier_appel`, qui
        // elle épargne le renouvellement. Jeton de renouvellement RECONNU
        // attaché : le refus testé ici doit venir de `refuser_tout`, jamais
        // d'un jeton de renouvellement inconnu (voir
        // `refresh_refuse_un_jeton_de_renouvellement_inconnu`).
        let s = FauxServeur::demarrer();
        let jeton_renouvellement = s.jeton_de_renouvellement_de_test();
        s.etat_mut().refuser_tout = true;

        let reponse = ureq::post(&format!("{}/api/auth/refresh", s.url()))
            .send_string(&format!(r#"{{"refresh":"{jeton_renouvellement}"}}"#))
            .unwrap_err();
        assert_eq!(reponse.into_response().unwrap().status(), 401);
        assert_eq!(s.etat_mut().appels_de_renouvellement, 1);
    }

    #[test]
    fn renouvellement_rend_jeton_neuf_et_compte_lappel() {
        // Échoue si la valeur rendue n'est pas exactement "jeton-neuf-1"
        // (premier appel — distincte à chaque appel depuis la vague de
        // correction finale), ou si le compteur ne progresse pas.
        let s = FauxServeur::demarrer();
        let jeton_renouvellement = s.jeton_de_renouvellement_de_test();
        let recu: serde_json::Value = ureq::post(&format!("{}/api/auth/refresh", s.url()))
            .send_string(&format!(r#"{{"refresh":"{jeton_renouvellement}"}}"#))
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu["acces"], "jeton-neuf-1");
        assert_eq!(s.etat_mut().appels_de_renouvellement, 1);
    }

    #[test]
    fn refresh_refuse_un_jeton_de_renouvellement_inconnu() {
        // Échoue si un jeton de renouvellement quelconque, jamais délivré
        // par ce double, était accepté — c'est l'angle mort signalé par le
        // relecteur : avant cette ronde, N'IMPORTE QUELLE chaîne réussissait
        // ici tant que `refuser_tout` n'était pas actif.
        let s = FauxServeur::demarrer();
        let reponse = ureq::post(&format!("{}/api/auth/refresh", s.url()))
            .send_string(r#"{"refresh":"peu importe"}"#)
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 401);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Renouvellement refuse"}"#);
    }

    #[test]
    fn refresh_400_sur_champ_manquant() {
        // Échoue si un corps sans `refresh` (ou dont `refresh` n'est pas
        // une chaîne) produisait autre chose qu'un 400 — même ordre que le
        // site : la forme est vérifiée AVANT toute décision de refus.
        let s = FauxServeur::demarrer();
        for corps in [r#"{}"#, r#"{"refresh":42}"#, r#"pas du json"#] {
            let reponse =
                ureq::post(&format!("{}/api/auth/refresh", s.url())).send_string(corps).unwrap_err();
            let reponse = reponse.into_response().unwrap();
            assert_eq!(reponse.status(), 400, "corps testé : {corps}");
            assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Requete invalide"}"#);
        }
    }

    #[test]
    fn depot_denveloppe_est_compte_et_relu_par_sync() {
        // Échoue si `depots_recus` ne progresse pas, ou si l'enveloppe
        // déposée ne réapparaît pas dans `GET /api/sky/sync` avec `id`
        // typé chaîne et les deux device_id typés nombre.
        //
        // RONDE DE CORRECTION 1 : le jeton qui relève doit désormais avoir
        // un appareil associé (`jetons_appareil`) — sans quoi `gerer_sync`
        // rend `enveloppes: []` par construction. `enregistrer_appareil_http`
        // fournit cet appareil et son identifiant réel, utilisé ci-dessous
        // comme `destinataire_device_id` au lieu d'un `9` arbitraire.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let id_appareil = enregistrer_appareil_http(&s, &jeton);
        let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&format!(
                r#"{{"expediteur_device_id":7,"destinataire_device_id":{id_appareil},"charge":"YWJj"}}"#
            ))
            .unwrap();
        assert_eq!(reponse.status(), 204);
        assert_eq!(s.etat_mut().depots_recus, 1);

        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        let enveloppe = &recu["enveloppes"][0];
        assert!(enveloppe["id"].is_string());
        assert_eq!(enveloppe["expediteur_device_id"], 7);
        assert_eq!(enveloppe["destinataire_device_id"], id_appareil);
        assert_eq!(enveloppe["charge"], "YWJj");
    }

    #[test]
    fn depot_denveloppe_exige_aussi_un_jeton() {
        // Échoue si `POST /api/sky/envelopes` acceptait un dépôt sans
        // authentification — même réserve que `sync_exige_un_jeton`, sur
        // l'AUTRE route authentifiée du double.
        let s = FauxServeur::demarrer();
        let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
            .send_string(r#"{"expediteur_device_id":1,"destinataire_device_id":2,"charge":"YQ=="}"#);
        let reponse = reponse.unwrap_err().into_response().unwrap();
        assert_eq!(reponse.status(), 401);
        // Le dépôt n'a pas dû être compté : le refus d'authentification
        // précède toute lecture du corps.
        assert_eq!(s.etat_mut().depots_recus, 0);
    }

    #[test]
    fn sync_rend_inchange_a_version_connue_et_complet_sinon() {
        // Échoue si `{ inchange: true }` n'est jamais rendu, ou si il l'est
        // pour une version qui ne correspond pas à l'état courant.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().version = 42;

        let a_jour: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=42", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(a_jour, serde_json::json!({ "inchange": true }));

        let perime: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=1", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(perime["version"], 42);
    }

    #[test]
    fn auth_native_reussit_avec_le_code_enregistre_et_le_jeton_fonctionne_ensuite() {
        // Échoue si un couple (code, secret) enregistré dans
        // `code_natif_valide` ne produisait pas un succès, ou si le jeton
        // d'accès reçu n'était pas ensuite reconnu par une route
        // authentifiée — la moitié qui prouve que ce chemin de succès sert
        // réellement à quelque chose (tâche 6 : ranger un jeton utilisable
        // dans le coffre).
        let s = FauxServeur::demarrer();
        s.etat_mut().code_natif_valide =
            Some(("CODEVALIDE".to_string(), "secret-correct".to_string()));

        let recu: serde_json::Value = ureq::post(&format!("{}/api/auth/native", s.url()))
            .send_string(r#"{"code":"CODEVALIDE","secret":"secret-correct"}"#)
            .unwrap()
            .into_json()
            .unwrap();
        let acces = recu["acces"].as_str().expect("acces attendu comme chaine").to_string();
        assert!(!acces.is_empty());
        assert!(recu["refresh"].is_string());

        let reponse = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {acces}"))
            .call();
        assert_eq!(reponse.unwrap().status(), 200);
    }

    #[test]
    fn auth_native_refuse_un_secret_incorrect_meme_code_enregistre() {
        // Échoue si la comparaison acceptait un secret différent de celui
        // enregistré — le couple doit correspondre EXACTEMENT, pas
        // seulement le code.
        let s = FauxServeur::demarrer();
        s.etat_mut().code_natif_valide =
            Some(("CODEVALIDE".to_string(), "secret-correct".to_string()));

        let e = ureq::post(&format!("{}/api/auth/native", s.url()))
            .send_string(r#"{"code":"CODEVALIDE","secret":"mauvais-secret"}"#)
            .unwrap_err();
        let reponse = match e {
            ureq::Error::Status(_, r) => r,
            _ => panic!("attendu 401"),
        };
        assert_eq!(reponse.status(), 401);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Code refuse"}"#);
    }

    #[test]
    fn auth_native_400_sur_corps_malforme() {
        // Échoue si un corps où `code`/`secret` sont absents ou ne sont
        // pas des chaînes produisait un 401 plutôt qu'un 400 — le vrai
        // serveur distingue les deux (`typeof code !== "string" ...` sort
        // en 400 AVANT tout appel à `echangerCode`), ce double doit faire
        // pareil. Même avec `code_natif_valide` enregistré, ces corps ne
        // doivent jamais atteindre la comparaison : sinon un attaquant qui
        // envoie un `code` numérique apprendrait quelque chose du timing.
        let s = FauxServeur::demarrer();
        s.etat_mut().code_natif_valide =
            Some(("CODEVALIDE".to_string(), "secret-correct".to_string()));

        for corps in [
            r#"{"code":42,"secret":"secret-correct"}"#,
            r#"{"code":"CODEVALIDE","secret":42}"#,
            r#"{"code":"CODEVALIDE"}"#,
            r#"{}"#,
            r#"pas du json"#,
        ] {
            let reponse =
                ureq::post(&format!("{}/api/auth/native", s.url())).send_string(corps).unwrap_err();
            let reponse = reponse.into_response().unwrap();
            assert_eq!(reponse.status(), 400, "corps testé : {corps}");
            assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Requete invalide"}"#);
        }
    }

    #[test]
    fn depot_refuse_un_identifiant_expediteur_non_positif() {
        // AJOUT DE LA TÂCHE 9 : avant elle, `gerer_depot` acceptait
        // n'importe quel entier, y compris nul ou négatif. Échoue si le
        // double redevenait plus permissif que le site
        // (`idAppareilValide`, `envelopes/route.ts`).
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        for id in [0i64, -1] {
            let corps =
                json!({"expediteur_device_id": id, "destinataire_device_id": 2, "charge": "YQ=="})
                    .to_string();
            let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
                .set("Authorization", &format!("Bearer {jeton}"))
                .send_string(&corps)
                .unwrap_err();
            let reponse = reponse.into_response().unwrap();
            assert_eq!(reponse.status(), 400, "identifiant testé : {id}");
            assert_eq!(
                reponse.into_string().unwrap(),
                r#"{"error":"expediteur_device_id invalide : attendu un entier positif"}"#
            );
        }
        assert_eq!(s.etat_mut().depots_recus, 0);
    }

    #[test]
    fn depot_refuse_un_identifiant_destinataire_non_positif() {
        // Même réserve que le test précédent, sur l'AUTRE champ — les deux
        // messages sont volontairement distincts (voir la route réelle),
        // donc les deux gardes doivent être vérifiées séparément.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        for id in [0i64, -1] {
            let corps =
                json!({"expediteur_device_id": 1, "destinataire_device_id": id, "charge": "YQ=="})
                    .to_string();
            let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
                .set("Authorization", &format!("Bearer {jeton}"))
                .send_string(&corps)
                .unwrap_err();
            let reponse = reponse.into_response().unwrap();
            assert_eq!(reponse.status(), 400, "identifiant testé : {id}");
            assert_eq!(
                reponse.into_string().unwrap(),
                r#"{"error":"destinataire_device_id invalide : attendu un entier positif"}"#
            );
        }
        assert_eq!(s.etat_mut().depots_recus, 0);
    }

    #[test]
    fn depot_refuse_une_charge_vide() {
        // AJOUT DE LA TÂCHE 9 : avant elle, une chaîne vide décodait vers 0
        // octet et était acceptée sans réserve — le site refuse
        // explicitement `charge.length === 0`.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let corps = json!({"expediteur_device_id": 1, "destinataire_device_id": 2, "charge": ""}).to_string();

        let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&corps)
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 400);
        assert_eq!(
            reponse.into_string().unwrap(),
            r#"{"error":"charge invalide : attendu une chaine base64 non vide"}"#
        );
        assert_eq!(s.etat_mut().depots_recus, 0);
    }

    #[test]
    fn depot_refuse_une_charge_scellee_trop_grosse_avec_le_404_uniforme() {
        // AJOUT DE LA TÂCHE 9 : avant elle, aucune taille n'était jamais
        // vérifiée. 4097 octets décodés dépasse `TAILLE_CHARGE_MAX` (4096)
        // d'exactement un octet — le refus doit être le MÊME 404 uniforme
        // que `refuser_depot`, jamais un message distinct qui dirait
        // "trop gros" (voir le commentaire de `gerer_depot`).
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let charge_trop_grosse = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 4097]);
        let corps = json!({
            "expediteur_device_id": 1,
            "destinataire_device_id": 2,
            "charge": charge_trop_grosse,
        })
        .to_string();

        let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&corps)
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 404);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Depot refuse"}"#);
        assert_eq!(s.etat_mut().depots_recus, 0);
    }

    #[test]
    fn depot_refuse_de_maniere_uniforme_quand_pilote() {
        // Échoue si `refuser_depot` ne produisait pas le refus UNIFORME
        // `404 {"error":"Depot refuse"}`, ou si le dépôt refusé était quand
        // même compté dans `depots_recus` — c'était le trou signalé par le
        // relecteur : avant cette ronde, aucun chemin ne menait à ce 404.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().refuser_depot = true;

        let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(r#"{"expediteur_device_id":1,"destinataire_device_id":2,"charge":"YQ=="}"#)
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 404);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Depot refuse"}"#);
        assert_eq!(s.etat_mut().depots_recus, 0);
    }

    // --- AJOUTS DE LA TÂCHE 8 (extension du double, brief le permet
    // explicitement — voir le commentaire de `EtatFaux`) -------------------

    #[test]
    fn sync_court_circuite_inchange_quand_une_enveloppe_attend() {
        // LE PIÈGE SIGNALÉ PAR LE CAHIER DES CHARGES DE LA TÂCHE 8 : une
        // enveloppe déposée entre deux appels à VERSION INCHANGÉE doit
        // continuer à voyager. Échoue si `gerer_sync` répondait `inchange`
        // dès que `version_connue == e.version`, sans regarder si une
        // enveloppe attend encore.
        //
        // RONDE DE CORRECTION 1 : le jeton qui synchronise doit avoir un
        // appareil associé — le court-circuit ne regarde désormais QUE les
        // enveloppes de CET appareil (voir `EtatFaux::jetons_appareil`),
        // donc l'enveloppe poussée ci-dessous vise l'identifiant que le
        // double vient réellement d'attribuer, pas un `2` arbitraire.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let id_appareil = enregistrer_appareil_http(&s, &jeton);
        s.etat_mut().version = 5;
        s.etat_mut().enveloppes.push(EnveloppeFausse {
            id: "1".to_string(),
            expediteur_device_id: 1,
            destinataire_device_id: id_appareil,
            charge: "YQ==".to_string(),
        });

        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=5", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();

        assert_ne!(recu, serde_json::json!({ "inchange": true }));
        assert_eq!(recu["enveloppes"][0]["id"], "1");
    }

    #[test]
    fn sync_ne_court_circuite_pas_inchange_pour_une_enveloppe_dun_autre_appareil() {
        // NOUVEAU (RONDE DE CORRECTION 1) : symétrique du test précédent —
        // une enveloppe qui attend un AUTRE appareil ne doit PAS empêcher
        // `inchange`. Avant cette ronde, `gerer_sync` regardait
        // `e.enveloppes.is_empty()` globalement : une enveloppe pour
        // n'importe quel appareil aurait fait échouer ce test-ci en
        // court-circuitant `inchange` à tort.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let mon_id = enregistrer_appareil_http(&s, &jeton);
        s.etat_mut().version = 5;
        s.etat_mut().enveloppes.push(EnveloppeFausse {
            id: "1".to_string(),
            expediteur_device_id: 1,
            destinataire_device_id: mon_id + 1000, // un AUTRE appareil, jamais le mien.
            charge: "YQ==".to_string(),
        });

        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=5", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();

        assert_eq!(recu, serde_json::json!({ "inchange": true }));
    }

    #[test]
    fn sync_efface_les_enveloppes_apres_livraison() {
        // Échoue si `gerer_sync` ne vidait pas `EtatFaux::enveloppes` après
        // les avoir servies : un second appel, à la MÊME version, verrait
        // alors encore l'enveloppe déjà livrée — soit en la retransmettant
        // (si le court-circuit ci-dessus manquait aussi), soit en
        // continuant à empêcher `inchange` pour toujours.
        //
        // RONDE DE CORRECTION 1 : même adaptation que le test précédent —
        // jeton associé à un appareil réel, enveloppe adressée à celui-ci.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let id_appareil = enregistrer_appareil_http(&s, &jeton);
        s.etat_mut().version = 5;
        s.etat_mut().enveloppes.push(EnveloppeFausse {
            id: "1".to_string(),
            expediteur_device_id: 1,
            destinataire_device_id: id_appareil,
            charge: "YQ==".to_string(),
        });

        let premier: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=5", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(premier["enveloppes"][0]["id"], "1");

        let second: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=5", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(second, serde_json::json!({ "inchange": true }));
    }

    #[test]
    fn sync_nefface_pas_les_enveloppes_dun_autre_appareil() {
        // NOUVEAU (RONDE DE CORRECTION 1) : la consommation à la livraison
        // ne doit effacer QUE les enveloppes livrées à CET appareil. Avant
        // cette ronde, `e.enveloppes.clear()` videait tout — une enveloppe
        // pour un autre appareil, jamais vue par ce jeton, disparaissait
        // quand même.
        let s = FauxServeur::demarrer();
        let jeton_a = s.jeton_de_test_pour("a-nefface-pas-autrui");
        let _id_a = enregistrer_appareil_http(&s, &jeton_a); // seul le jeton sert ici.
        let jeton_b = s.jeton_de_test_pour("b-nefface-pas-autrui");
        let id_b = enregistrer_appareil_http(&s, &jeton_b);
        s.etat_mut().version = 5;
        s.etat_mut().enveloppes.push(EnveloppeFausse {
            id: "1".to_string(),
            expediteur_device_id: 1,
            destinataire_device_id: id_b,
            charge: "YQ==".to_string(),
        });

        // A synchronise SANS `?version=` (délibérément, voir ci-dessous) :
        // ne doit RIEN voir (l'enveloppe est pour B), et ne doit RIEN
        // effacer.
        //
        // SANS `?version=` — PAS UN OUBLI : avec `?version=5` (= e.version),
        // et aucune enveloppe pour A, le court-circuit `inchange` renvoie
        // AVANT MÊME D'ATTEINDRE la ligne qui efface — la neutralisation
        // visée ici (retirer le filtre de `retain`) ne serait alors JAMAIS
        // exercée, et ce test resterait vert même cassé. Sans `?version=`,
        // `version_connue` vaut `None`, qui ne peut jamais égaler
        // `Some(e.version)` : la branche complète (et sa consommation) est
        // TOUJOURS empruntée.
        let _: serde_json::Value = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton_a}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(s.etat_mut().enveloppes.len(), 1, "l'enveloppe de B doit survivre à la synchronisation de A");

        // B synchronise ensuite : doit voir SON enveloppe, encore présente.
        let recu_b: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=5", s.url()))
            .set("Authorization", &format!("Bearer {jeton_b}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu_b["enveloppes"][0]["destinataire_device_id"], id_b);
    }

    #[test]
    fn devices_refuse_une_cle_publique_tronquee() {
        // Échoue si le double acceptait une clé publique qui ne décode pas
        // vers exactement 32 octets sous forme canonique — un double plus
        // permissif que le vrai serveur (`clePubliqueValide`) donnerait une
        // confiance imméritée aux tests des tâches suivantes.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let cle_tronquee = base64::engine::general_purpose::STANDARD.encode([1u8; 16]);

        let reponse = ureq::post(&format!("{}/api/sky/devices", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&format!(
                r#"{{"publicKey":"{cle_tronquee}","nom":"Mon PC","plateforme":"windows"}}"#
            ))
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 400);
    }

    #[test]
    fn devices_reussit_et_incremente_lidentifiant() {
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let cle = base64::engine::general_purpose::STANDARD.encode([2u8; 32]);

        let recu: serde_json::Value = ureq::post(&format!("{}/api/sky/devices", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&format!(r#"{{"publicKey":"{cle}","nom":"Mon PC","plateforme":"windows"}}"#))
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu["id"], 1);
    }

    #[test]
    fn friends_refuse_un_code_inconnu_et_reussit_avec_le_code_enregistre() {
        // "STRANGE9" et "BUDDY234" sont tous deux BIEN FORMÉS (8 caractères
        // de l'alphabet réel `ABCDEFGHJKMNPQRSTUVWXYZ23456789`) — ce test
        // vise la distinction 404 (forme correcte, inconnu) / succès, pas
        // la validation de forme elle-même (voir
        // `friends_refuse_un_code_mal_forme_avec_400` pour celle-ci).
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();

        let refuse = ureq::post(&format!("{}/api/sky/friends", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(r#"{"code":"STRANGE9"}"#)
            .unwrap_err();
        assert_eq!(refuse.into_response().unwrap().status(), 404);

        s.etat_mut().code_ami_valide = Some(("BUDDY234".to_string(), 42));
        let recu: serde_json::Value = ureq::post(&format!("{}/api/sky/friends", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(r#"{"code":"BUDDY234"}"#)
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu["id"], 42);
    }

    #[test]
    fn friends_refuse_un_code_mal_forme_avec_400() {
        // RONDE DE CORRECTION 1, « Important 1 » : "INCONNU1" contient `I`,
        // `O` et `1`, absents de l'alphabet réel — contre le vrai serveur
        // ce code reçoit un 400 de forme, JAMAIS le 404 métier qu'un
        // double moins strict rendrait. Neutralisation : retirer l'appel à
        // `normaliser_code_ami` dans `gerer_friends` (revenir à une
        // comparaison directe à `code_ami_valide`) fait rougir CE test
        // précis, seul — les codes bien formés des deux tests voisins
        // restent inchangés par une telle neutralisation.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();

        let refuse = ureq::post(&format!("{}/api/sky/friends", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(r#"{"code":"INCONNU1"}"#)
            .unwrap_err();
        let reponse = refuse.into_response().unwrap();
        assert_eq!(reponse.status(), 400);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"code invalide : forme inattendue"}"#);
    }

    #[test]
    fn devices_refuse_un_nom_avec_octet_nul() {
        // RONDE DE CORRECTION 1, « Important 2 » : le site refuse l'octet
        // NUL dans `nom` via `texteStockable` (400) avant que la valeur
        // n'atteigne Postgres, qui la refuserait par un 500. Échoue si le
        // double acceptait encore un `nom` qui le porte.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let cle = base64::engine::general_purpose::STANDARD.encode([9u8; 32]);
        let corps = serde_json::json!({"publicKey": cle, "nom": "a\u{0}b", "plateforme": "windows"});

        let reponse = ureq::post(&format!("{}/api/sky/devices", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&corps.to_string())
            .unwrap_err();
        assert_eq!(reponse.into_response().unwrap().status(), 400);
    }

    #[test]
    fn devices_mesure_la_longueur_du_nom_en_unites_utf16() {
        // RONDE DE CORRECTION 1 : `nomValide` mesure `nom.length` — des
        // unités de code UTF-16, pas des caractères Unicode. Un emoji
        // (U+1F600) compte pour 2 unités UTF-16 mais pour 1 seul `char`
        // Rust : 33 emoji, c'est 33 `chars()` (accepté par l'ANCIENNE
        // mesure de ce double, `chars().count() <= 64`) mais 66 unités
        // UTF-16 (refusé par la vraie mesure, > 64). Échoue si le double
        // mesurait encore en caractères.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let cle = base64::engine::general_purpose::STANDARD.encode([9u8; 32]);
        let nom_trop_long: String = "😀".repeat(33);
        assert_eq!(nom_trop_long.chars().count(), 33);
        assert_eq!(nom_trop_long.encode_utf16().count(), 66);

        let corps = serde_json::json!({"publicKey": cle, "nom": nom_trop_long, "plateforme": "windows"});
        let reponse = ureq::post(&format!("{}/api/sky/devices", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&corps.to_string())
            .unwrap_err();
        assert_eq!(reponse.into_response().unwrap().status(), 400);
    }

    #[test]
    fn friends_accept_refuse_un_identifiant_mal_forme_avec_400() {
        // RONDE DE CORRECTION 1 (Mineur) : un identifiant non entier,
        // négatif ou nul reçoit 400 côté site, pas le même 404 qu'un
        // identifiant bien formé mais inexistant.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().amitie_acceptable = Some(7);

        for id in ["0", "-3", "abc", "7.5"] {
            let reponse = ureq::post(&format!("{}/api/sky/friends/{id}/accept", s.url()))
                .set("Authorization", &format!("Bearer {jeton}"))
                .send_string("{}")
                .unwrap_err();
            let reponse = reponse.into_response().unwrap();
            assert_eq!(reponse.status(), 400, "identifiant testé : {id}");
            assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Identifiant invalide"}"#);
        }
    }

    #[test]
    fn friends_accept_reussit_pour_lidentifiant_pilote_et_refuse_sinon() {
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().amitie_acceptable = Some(7);

        let refuse = ureq::post(&format!("{}/api/sky/friends/8/accept", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string("{}")
            .unwrap_err();
        assert_eq!(refuse.into_response().unwrap().status(), 404);

        let recu: serde_json::Value = ureq::post(&format!("{}/api/sky/friends/7/accept", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string("{}")
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu["ok"], true);
    }

    // --- /api/auth/me (AJOUT DE LA TÂCHE 10) --------------------------
    //
    // Même discipline que le reste de ce module : chaque route a ses tests
    // HTTP directs, indépendants de `sky_compte` — ceux-là vivent dans
    // `identite_test.rs`. Sans ceux-ci, `MoiFaux::nouveau` n'était appelée
    // que par `identite_test.rs`, jamais par ce module : `cargo clippy`
    // le rendait mort dans les quatre AUTRES binaires de test qui incluent
    // ce fichier (`faux_serveur_test`, `session_test`, `boite_test`,
    // `annuaire_test`), chacun compilant ce module séparément via `#[path]`.

    #[test]
    fn me_exige_un_jeton() {
        // Échoue si `GET /api/auth/me` répondait sans en-tête `Authorization`
        // — même garde que `sync_exige_un_jeton`.
        let s = FauxServeur::demarrer();
        let reponse = ureq::get(&format!("{}/api/auth/me", s.url())).call();
        let reponse = reponse.unwrap_err().into_response().unwrap();
        assert_eq!(reponse.status(), 401);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Non authentifie"}"#);
    }

    #[test]
    fn me_rend_lidentifiant_et_le_nom_discord_en_camel_case() {
        // Échoue si `gerer_me` rendait `discord_name` (snake_case) plutôt
        // que `discordName` — forme exacte de la réponse du site.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().moi = Some(MoiFaux::nouveau(42, "Killian"));

        let recu: serde_json::Value = ureq::get(&format!("{}/api/auth/me", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();

        assert_eq!(recu["id"], 42);
        assert_eq!(recu["discordName"], "Killian");
        assert!(recu.get("discord_name").is_none());
    }

    #[test]
    fn me_sans_utilisateur_connu_rend_404() {
        // Même principe que `code_ami_valide`/`amitie_acceptable` : aucun
        // utilisateur n'est connu tant qu'un test ne l'a pas enregistré.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();

        let reponse = ureq::get(&format!("{}/api/auth/me", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 404);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Utilisateur introuvable"}"#);
    }

    #[test]
    fn me_avec_session_partielle_rend_403_avant_de_consulter_moi() {
        // Échoue si `gerer_me` consultait `moi` AVANT `session_partielle_totp`
        // — même ordre que le site (`requireAuth` puis `!totpVerified` puis
        // `findUserById`). `moi` est enregistré pour prouver que ce n'est
        // PAS son absence qui produit ce refus.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().moi = Some(MoiFaux::nouveau(1, "peu importe"));
        s.etat_mut().session_partielle_totp = true;

        let reponse = ureq::get(&format!("{}/api/auth/me", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 403);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"TOTP verification required"}"#);
    }

    // --- Sessions et révocation d'appareil (VAGUE DE CORRECTION FINALE, I1) ---

    /// Échange natif complet en HTTP brut : enregistre `code`, l'échange, rend
    /// (jeton d'accès, jeton de renouvellement).
    fn connexion_native_http(s: &FauxServeur, code: &str) -> (String, String) {
        s.etat_mut().code_natif_valide = Some((code.to_string(), "secret".to_string()));
        let recu: serde_json::Value = ureq::post(&format!("{}/api/auth/native", s.url()))
            .send_string(&json!({ "code": code, "secret": "secret" }).to_string())
            .expect("échange natif attendu en succès")
            .into_json()
            .unwrap();
        (recu["acces"].as_str().unwrap().to_string(), recu["refresh"].as_str().unwrap().to_string())
    }

    fn statut_sync(s: &FauxServeur, jeton: &str) -> u16 {
        match ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
        {
            Ok(r) => r.status(),
            Err(ureq::Error::Status(statut, _)) => statut,
            Err(e) => panic!("transport inattendu : {e}"),
        }
    }

    fn supprimer_appareil_http(s: &FauxServeur, jeton: Option<&str>, id: &str) -> (u16, String) {
        let mut requete = ureq::delete(&format!("{}/api/sky/devices/{id}", s.url()));
        if let Some(j) = jeton {
            requete = requete.set("Authorization", &format!("Bearer {j}"));
        }
        match requete.call() {
            Ok(r) => (r.status(), r.into_string().unwrap()),
            Err(ureq::Error::Status(statut, r)) => (statut, r.into_string().unwrap()),
            Err(e) => panic!("transport inattendu : {e}"),
        }
    }

    #[test]
    fn chaque_echange_natif_cree_une_session_distincte_et_le_code_ne_sert_qu_une_fois() {
        // Échoue si deux échanges portaient la même session (le site en crée une
        // par connexion), ou si un code déjà échangé était accepté une seconde fois
        // (le site : code à usage unique).
        let s = FauxServeur::demarrer();
        let (acces_1, _) = connexion_native_http(&s, "CODE-UN");
        let (acces_2, _) = connexion_native_http(&s, "CODE-DEUX");
        {
            let e = s.etat_mut();
            assert_ne!(e.session_du_jeton[&acces_1], e.session_du_jeton[&acces_2]);
        }

        s.etat_mut().code_natif_valide = Some(("CODE-TROIS".to_string(), "secret".to_string()));
        let corps = json!({ "code": "CODE-TROIS", "secret": "secret" }).to_string();
        assert!(ureq::post(&format!("{}/api/auth/native", s.url())).send_string(&corps).is_ok());
        let second = ureq::post(&format!("{}/api/auth/native", s.url())).send_string(&corps).unwrap_err();
        assert_eq!(second.into_response().unwrap().status(), 401);
    }

    #[test]
    fn un_renouvellement_garde_la_session_donc_l_appareil_courant() {
        // Échoue si le jeton renouvelé ne portait pas la session du jeton d'origine :
        // l'enveloppe de l'appareil enregistré avec le premier jeton ne serait plus
        // livrée au second.
        let s = FauxServeur::demarrer();
        let (acces, refresh) = connexion_native_http(&s, "CODE-RENOUV");
        let id = enregistrer_appareil_http(&s, &acces);
        s.etat_mut().enveloppes.push(EnveloppeFausse {
            id: "1".to_string(),
            expediteur_device_id: 1,
            destinataire_device_id: id,
            charge: "YQ==".to_string(),
        });

        let renouvele: serde_json::Value = ureq::post(&format!("{}/api/auth/refresh", s.url()))
            .send_string(&json!({ "refresh": refresh }).to_string())
            .unwrap()
            .into_json()
            .unwrap();
        let acces_neuf = renouvele["acces"].as_str().unwrap().to_string();
        assert_ne!(acces_neuf, acces);

        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {acces_neuf}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu["enveloppes"][0]["destinataire_device_id"], id);
    }

    #[test]
    fn supprimer_un_appareil_rend_204_sans_corps_et_revoque_la_session_qu_il_porte() {
        // Échoue si la révocation ne révoquait pas la session portée par l'appareil :
        // son jeton d'accès ET son jeton de renouvellement doivent être refusés
        // ensuite. Contre-preuve : la session qui a révoqué n'est pas touchée.
        let s = FauxServeur::demarrer();
        let (acces_ancien, refresh_ancien) = connexion_native_http(&s, "CODE-ANCIEN");
        let id = enregistrer_appareil_http(&s, &acces_ancien);
        let (acces_courant, _) = connexion_native_http(&s, "CODE-COURANT");
        assert_eq!(statut_sync(&s, &acces_ancien), 200, "avant révocation, l'ancienne session passe");

        let (statut, corps) = supprimer_appareil_http(&s, Some(&acces_courant), &id.to_string());
        assert_eq!(statut, 204);
        assert_eq!(corps, "");

        assert_eq!(statut_sync(&s, &acces_ancien), 401);
        let refus = ureq::post(&format!("{}/api/auth/refresh", s.url()))
            .send_string(&json!({ "refresh": refresh_ancien }).to_string())
            .unwrap_err();
        assert_eq!(refus.into_response().unwrap().status(), 401);

        assert_eq!(statut_sync(&s, &acces_courant), 200);
        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {acces_courant}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert!(recu["appareils"][0]["revoked_at"].is_string(), "l'appareil reste listé, marqué révoqué");
    }

    #[test]
    fn supprimer_un_appareil_deja_revoque_rend_encore_204() {
        let s = FauxServeur::demarrer();
        let (acces_ancien, _) = connexion_native_http(&s, "CODE-A");
        let id = enregistrer_appareil_http(&s, &acces_ancien);
        let (acces_courant, _) = connexion_native_http(&s, "CODE-B");
        assert_eq!(supprimer_appareil_http(&s, Some(&acces_courant), &id.to_string()).0, 204);
        assert_eq!(supprimer_appareil_http(&s, Some(&acces_courant), &id.to_string()).0, 204);
    }

    #[test]
    fn supprimer_un_appareil_inconnu_ou_d_autrui_rend_le_404_uniforme() {
        // Échoue si le double révoquait un identifiant qu'il n'a pas créé (55 n'est
        // qu'un appareil d'ami), ou dépassait la borne INTEGER sans rendre 404.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let cle = base64::engine::general_purpose::STANDARD.encode([5u8; 32]);
        s.etat_mut().amis.push(AmiFaux::avec_appareils("bob", vec![AppareilDAmiFaux { id: 55, public_key: cle }]));
        for id in ["999", "55", "2147483648"] {
            let (statut, corps) = supprimer_appareil_http(&s, Some(&jeton), id);
            assert_eq!(statut, 404, "identifiant testé : {id}");
            assert_eq!(corps, r#"{"error":"Appareil introuvable"}"#);
        }
    }

    #[test]
    fn supprimer_un_appareil_avec_un_identifiant_invalide_rend_400() {
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        for id in ["0", "-3", "abc", "7.5"] {
            let (statut, corps) = supprimer_appareil_http(&s, Some(&jeton), id);
            assert_eq!(statut, 400, "identifiant testé : {id}");
            assert_eq!(corps, r#"{"error":"Identifiant invalide"}"#);
        }
    }

    #[test]
    fn supprimer_un_appareil_exige_un_jeton_puis_une_session_complete() {
        let s = FauxServeur::demarrer();
        assert_eq!(
            supprimer_appareil_http(&s, None, "1"),
            (401, r#"{"error":"Non authentifie"}"#.to_string())
        );
        let jeton = s.jeton_de_test();
        s.etat_mut().session_partielle_totp = true;
        assert_eq!(
            supprimer_appareil_http(&s, Some(&jeton), "1"),
            (403, r#"{"error":"Verification TOTP requise"}"#.to_string())
        );
    }

    #[test]
    fn un_appareil_revoque_disparait_des_appareils_vus_par_les_amis() {
        // Le double ne modélise qu'un utilisateur : l'appareil créé ici est placé
        // aussi dans la liste d'un ami, pour figurer la vue qu'un ami a de lui.
        // Échoue si `gerer_sync` rendait encore l'appareil révoqué dans
        // `amis[].appareils` (le site : `d.revoked_at IS NULL`).
        let s = FauxServeur::demarrer();
        let jeton_a = s.jeton_de_test_pour("vu-par-un-ami-a");
        let id = enregistrer_appareil_http(&s, &jeton_a);
        let cle = base64::engine::general_purpose::STANDARD.encode([5u8; 32]);
        s.etat_mut().amis.push(AmiFaux::avec_appareils(
            "alice",
            vec![
                AppareilDAmiFaux { id, public_key: cle.clone() },
                AppareilDAmiFaux { id: id + 100, public_key: cle.clone() },
            ],
        ));
        let jeton_b = s.jeton_de_test_pour("vu-par-un-ami-b");

        assert_eq!(supprimer_appareil_http(&s, Some(&jeton_b), &id.to_string()).0, 204);

        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton_b}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu["amis"][0]["appareils"], json!([{ "id": id + 100, "public_key": cle }]));
    }
}
