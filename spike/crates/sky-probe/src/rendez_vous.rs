//! Le rendez-vous par la boîte aux lettres : toute la logique de la
//! négociation qui ne touche ni le réseau ni l'horloge réelle.
//!
//! `cmd_host` et `cmd_view` ne gardent que la colle : ouvrir le lien,
//! déposer, afficher. Ce qui décide — à qui sceller la réponse, quelle
//! enveloppe retenir, quand cesser d'attendre — vit ici, et se teste sur des
//! `Etat` et des `Message` fabriqués, sans serveur ni attente réelle.
//!
//! # Deux étages de clés, à ne pas confondre
//!
//! - L'**intérieur** d'une réponse est scellé pour la clé **éphémère** portée
//!   en clair par l'offre. `PeerLink::repondant` le fait déjà.
//! - L'**enveloppe** qui transporte cette réponse est scellée par `deposer`
//!   pour la clé **d'annuaire** de l'appareil expéditeur — celle que le
//!   spectateur tient dans son coffre, et avec laquelle il relève.
//!
//! Sceller l'enveloppe pour la clé éphémère de l'offre produirait une
//! enveloppe que le spectateur ne peut pas ouvrir : `relever` l'écarterait
//! sans bruit, et `view` conclurait à tort « pas de réponse ». Les deux clés
//! diffèrent par conception ; aucun contrôle croisé ne les rapproche.

use std::time::{Duration, Instant};

use sky_compte::{AppareilDAmi, Etat, Message};
use sky_net::handshake::{decomprimer, Blob};

/// Points de départ argumentés, PAS des mesures : 2 s est sous le seuil où
/// une attente se remarque, 30 min couvre une session de jeu sans courir
/// indéfiniment, 60 s suffit à un hôte déjà disponible. À ajuster après le
/// premier essai réel plutôt qu'à deviner deux fois.
pub const CADENCE: Duration = Duration::from_secs(2);
pub const FENETRE_HOTE: Duration = Duration::from_secs(30 * 60);
pub const ATTENTE_SPECTATEUR: Duration = Duration::from_secs(60);

/// Le temps tel que la boucle d'interrogation le voit. Injecté pour que les
/// tests fassent avancer une horloge fictive au lieu de dormir.
pub trait Horloge {
    /// Temps écoulé depuis le début de l'attente.
    fn ecoule(&self) -> Duration;
    /// Laisse passer `duree`.
    fn attendre(&mut self, duree: Duration);
}

/// L'horloge de production : `Instant` et un vrai sommeil.
pub struct HorlogeReelle {
    debut: Instant,
}

impl HorlogeReelle {
    pub fn demarrer() -> Self {
        Self { debut: Instant::now() }
    }
}

impl Horloge for HorlogeReelle {
    fn ecoule(&self) -> Duration {
        self.debut.elapsed()
    }

    fn attendre(&mut self, duree: Duration) {
        std::thread::sleep(duree);
    }
}

/// Interroge `synchroniser` toutes les `cadence`, jusqu'à ce que `trier`
/// trouve ce qu'on attend ou jusqu'à `delai`.
///
/// L'état rendu par un tour est transmis au suivant comme `precedent`. C'est
/// ce qui permet au serveur de répondre « inchangé » tant que rien n'attend,
/// et c'est aussi ce qui garde `amis` connu d'un tour à l'autre : sur
/// « inchangé », `synchroniser` recopie le précédent, enveloppes vidées.
///
/// Chaque état n'est présenté qu'UNE fois à `trier`. Le serveur efface les
/// enveloppes en les livrant : un état ne les porte qu'au tour qui les a
/// reçues, et ce qui n'est pas retenu à ce tour-là est perdu — c'est voulu,
/// `trier` a déjà écarté ce qui ne servait à rien.
///
/// Rend `Ok(None)` au délai, après une dernière interrogation faite à
/// l'échéance. Une erreur de synchronisation interrompt l'attente et remonte
/// telle quelle.
pub fn interroger<T, E>(
    initial: Option<Etat>,
    mut synchroniser: impl FnMut(Option<&Etat>) -> Result<Etat, E>,
    mut trier: impl FnMut(&Etat) -> Option<T>,
    horloge: &mut impl Horloge,
    cadence: Duration,
    delai: Duration,
) -> Result<Option<T>, E> {
    let mut precedent = initial;
    loop {
        let etat = synchroniser(precedent.as_ref())?;
        if let Some(trouve) = trier(&etat) {
            return Ok(Some(trouve));
        }
        precedent = Some(etat);

        let ecoule = horloge.ecoule();
        if ecoule >= delai {
            return Ok(None);
        }
        horloge.attendre(cadence.min(delai - ecoule));
    }
}

/// L'appareil pour lequel sceller l'enveloppe de la réponse : celui de
/// l'annuaire dont l'identifiant est l'expéditeur de l'offre — et donc sa
/// clé d'annuaire, jamais la clé éphémère que l'offre porte en clair (voir
/// l'en-tête de ce module).
///
/// `None` si l'expéditeur n'est l'appareil d'aucun ami connu de `etat` :
/// l'offre est alors écartée. On ne répond pas, adresses comprises, à un
/// appareil qu'on ne sait pas rattacher à un ami.
pub fn destinataire_de_la_reponse(etat: &Etat, expediteur_device_id: i64) -> Option<AppareilDAmi> {
    etat.amis
        .iter()
        .flat_map(|ami| ami.appareils.iter())
        .find(|appareil| appareil.id == expediteur_device_id)
        .cloned()
}

/// Une offre retenue par l'hôte : le bloc tel que reçu, et à qui répondre.
///
/// Ne dérive pas `Debug` : `texte` porte les adresses du spectateur. Même
/// discipline que `sky_compte::Message`.
pub struct OffreRecue {
    pub texte: String,
    pub destinataire: AppareilDAmi,
}

/// Chez l'hôte : les offres recevables parmi les messages relevés, dans
/// l'ordre d'arrivée.
///
/// Écarte sans bruit — et donc sans faire échouer l'attente — un expéditeur
/// qui n'est l'appareil d'aucun ami, un clair qui n'est pas du texte, un
/// bloc illisible, et un bloc dont le contenu ne se décomprime pas en SDP.
/// Ce dernier cas est celui d'une RÉPONSE égarée dans la boîte : son SDP est
/// scellé, il ne se décomprime pas. Le préfixe `v=0` (première ligne
/// imposée à tout SDP) ferme la porte à un contenu chiffré qui se
/// décomprimerait par hasard en quelques octets de texte.
///
/// Recevable ne veut pas dire utilisable : `PeerLink::repondant` peut encore
/// refuser le SDP. C'est alors à la colle d'essayer l'offre suivante.
pub fn offres_recevables(etat: &Etat, messages: Vec<Message>) -> Vec<OffreRecue> {
    messages
        .into_iter()
        .filter_map(|message| {
            let destinataire = destinataire_de_la_reponse(etat, message.expediteur_device_id)?;
            let texte = String::from_utf8(message.clair).ok()?;
            let blob = Blob::from_text(&texte).ok()?;
            let sdp = decomprimer(&blob.sealed_sdp).ok()?;
            if !sdp.starts_with("v=0") {
                return None;
            }
            Some(OffreRecue { texte, destinataire })
        })
        .collect()
}

/// Chez le spectateur : la réponse à SON offre en cours, s'il y en a une.
///
/// Écarte sans bruit un message qui ne vient d'aucun appareil de l'ami
/// sollicité, un clair qui n'est pas un bloc, et surtout un bloc dont
/// l'identifiant de session n'est pas celui de l'offre. Une réponse d'un
/// essai précédent ne doit ni interrompre `view`, ni atteindre
/// `accepter_reponse`, qui la refuserait en erreur au lieu de l'ignorer.
pub fn reponse_a_l_offre(
    messages: Vec<Message>,
    session: [u8; 4],
    appareils_de_l_ami: &[AppareilDAmi],
) -> Option<String> {
    messages.into_iter().find_map(|message| {
        if !appareils_de_l_ami.iter().any(|a| a.id == message.expediteur_device_id) {
            return None;
        }
        let texte = String::from_utf8(message.clair).ok()?;
        let blob = Blob::from_text(&texte).ok()?;
        (blob.session == session).then_some(texte)
    })
}

/// L'identifiant de session d'un bloc produit par `PeerLink::offrant`.
pub fn session_de(bloc: &str) -> anyhow::Result<[u8; 4]> {
    Ok(Blob::from_text(bloc)?.session)
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use sky_compte::{relever, Ami, EnveloppeRecue};
    use sky_crypto::Identity;
    use sky_net::handshake::comprimer;

    /// SDP minimal : seule sa première ligne compte pour le tri.
    const SDP: &str = "v=0\r\no=- 1 2 IN IP4 0.0.0.0\r\ns=-\r\n";

    const SESSION: [u8; 4] = [1, 2, 3, 4];
    const SESSION_PERIMEE: [u8; 4] = [9, 9, 9, 9];

    fn b64(cle: [u8; 32]) -> String {
        STANDARD.encode(cle)
    }

    fn appareil(id: i64, identite: &Identity) -> AppareilDAmi {
        AppareilDAmi { id, public_key: b64(identite.public_key()) }
    }

    fn etat(amis: Vec<Ami>, enveloppes: Vec<EnveloppeRecue>) -> Etat {
        Etat {
            version: 1,
            code: "ABCDEFGH".to_string(),
            amis,
            demandes: Vec::new(),
            appareils: Vec::new(),
            enveloppes,
        }
    }

    /// Un bloc d'offre tel que `PeerLink::offrant` le produit : SDP
    /// comprimé, en clair, sous la clé éphémère `cle`.
    fn bloc_offre(session: [u8; 4], cle: [u8; 32]) -> String {
        Blob { session, public_key: cle, sealed_sdp: comprimer(SDP) }.to_text()
    }

    /// Un bloc de réponse tel que `PeerLink::repondant` le produit : SDP
    /// comprimé PUIS scellé pour `pour`.
    fn bloc_reponse(session: [u8; 4], pour: [u8; 32]) -> String {
        let repondant = Identity::generate();
        Blob {
            session,
            public_key: repondant.public_key(),
            sealed_sdp: repondant.seal(&pour, &comprimer(SDP)),
        }
        .to_text()
    }

    fn message(id: &str, expediteur: i64, clair: &[u8]) -> Message {
        Message { id: id.to_string(), expediteur_device_id: expediteur, clair: clair.to_vec() }
    }

    /// Ce que `deposer` met sur le fil : `clair` scellé pour la clé de
    /// `destinataire`, en base64.
    fn enveloppe(id: &str, expediteur: i64, destinataire: &AppareilDAmi, clair: &[u8]) -> EnveloppeRecue {
        let cle: [u8; 32] = STANDARD.decode(&destinataire.public_key).unwrap().try_into().unwrap();
        EnveloppeRecue {
            id: id.to_string(),
            expediteur_device_id: expediteur,
            destinataire_device_id: destinataire.id,
            charge: STANDARD.encode(Identity::generate().seal(&cle, clair)),
        }
    }

    // --- (a) le destinataire de la réponse --------------------------------

    #[test]
    fn la_reponse_est_scellee_pour_la_cle_d_annuaire_de_l_expediteur() {
        // Le spectateur relève avec l'identité DURABLE de son coffre ; son
        // offre porte une clé ÉPHÉMÈRE. L'enveloppe de la réponse doit être
        // ouvrable par la première. Échoue si le destinataire est pris ailleurs
        // que dans l'annuaire à l'identifiant de l'expéditeur : clé de l'offre,
        // autre appareil du même ami, appareil d'un autre ami.
        let spectateur_durable = Identity::generate();
        let spectateur_ephemere = Identity::generate();
        let autre_appareil_de_l_ami = Identity::generate();
        let appareil_d_un_autre_ami = Identity::generate();

        let etat_hote = etat(
            vec![
                Ami {
                    id: 1,
                    discord_name: "alice".to_string(),
                    appareils: vec![
                        appareil(10, &autre_appareil_de_l_ami),
                        appareil(11, &spectateur_durable),
                    ],
                },
                Ami {
                    id: 2,
                    discord_name: "bob".to_string(),
                    appareils: vec![appareil(20, &appareil_d_un_autre_ami)],
                },
            ],
            Vec::new(),
        );

        let offre = bloc_offre(SESSION, spectateur_ephemere.public_key());
        let offres = offres_recevables(&etat_hote, vec![message("1", 11, offre.as_bytes())]);
        assert_eq!(offres.len(), 1, "l'offre d'un appareil d'ami doit être retenue");
        let destinataire = &offres[0].destinataire;
        assert_eq!(destinataire.id, 11);

        // Ce que le spectateur verra à sa prochaine synchronisation.
        let etat_spectateur = etat(
            Vec::new(),
            vec![enveloppe("2", 20, destinataire, b"la reponse")],
        );
        let releves = relever(&etat_spectateur, &spectateur_durable);
        assert_eq!(
            releves.len(),
            1,
            "le spectateur ne peut pas ouvrir l'enveloppe de la réponse avec l'identité de son \
             coffre : elle a été scellée pour une autre clé que sa clé d'annuaire"
        );
        assert_eq!(releves[0].clair, b"la reponse");
    }

    #[test]
    fn une_offre_d_un_appareil_qui_n_est_pas_celui_d_un_ami_est_ecartee() {
        let ami = Identity::generate();
        let etat_hote = etat(
            vec![Ami { id: 1, discord_name: "alice".to_string(), appareils: vec![appareil(10, &ami)] }],
            Vec::new(),
        );
        let offre = bloc_offre(SESSION, Identity::generate().public_key());
        assert!(offres_recevables(&etat_hote, vec![message("1", 99, offre.as_bytes())]).is_empty());
        assert_eq!(offres_recevables(&etat_hote, vec![message("1", 10, offre.as_bytes())]).len(), 1);
    }

    // --- (b) la boucle d'interrogation ------------------------------------

    /// Horloge qui n'avance que lorsqu'on lui demande d'attendre.
    #[derive(Default)]
    struct HorlogeFictive {
        maintenant: Duration,
        attentes: Vec<Duration>,
    }

    impl Horloge for HorlogeFictive {
        fn ecoule(&self) -> Duration {
            self.maintenant
        }

        fn attendre(&mut self, duree: Duration) {
            self.maintenant += duree;
            self.attentes.push(duree);
        }
    }

    /// Garde contre une boucle qui ne s'arrêterait pas : un test qui pend ne
    /// rougit pas, il bloque la suite.
    const APPELS_MAX: usize = 1000;

    fn etat_version(version: u64) -> Etat {
        Etat { version, ..etat(Vec::new(), Vec::new()) }
    }

    #[test]
    fn chaque_tour_recoit_l_etat_rendu_par_le_tour_precedent() {
        let mut vus: Vec<Option<u64>> = Vec::new();
        let mut horloge = HorlogeFictive::default();

        let trouve = interroger(
            Some(etat_version(7)),
            |precedent| {
                vus.push(precedent.map(|e| e.version));
                assert!(vus.len() < APPELS_MAX, "la boucle ne s'arrête pas");
                Ok::<_, ()>(etat_version(precedent.map_or(0, |e| e.version) + 1))
            },
            |_| None::<()>,
            &mut horloge,
            Duration::from_secs(2),
            Duration::from_secs(4),
        );

        assert_eq!(trouve, Ok(None));
        assert_eq!(vus, vec![Some(7), Some(8), Some(9)]);
    }

    #[test]
    fn la_boucle_s_arrete_a_la_premiere_trouvaille() {
        let mut appels = 0;
        let mut horloge = HorlogeFictive::default();

        let trouve = interroger(
            None,
            |_| {
                appels += 1;
                assert!(appels < APPELS_MAX, "la boucle ne s'arrête pas");
                Ok::<_, ()>(etat_version(appels as u64))
            },
            |e| (e.version >= 3).then_some(e.version),
            &mut horloge,
            Duration::from_secs(2),
            Duration::from_secs(60),
        );

        assert_eq!(trouve, Ok(Some(3)));
        assert_eq!(appels, 3, "la boucle a continué d'interroger après avoir trouvé");
        assert_eq!(horloge.attentes, vec![Duration::from_secs(2); 2]);
    }

    #[test]
    fn la_boucle_s_arrete_au_delai_sans_dormir_pour_de_vrai() {
        let debut_reel = Instant::now();
        let mut appels = 0;
        let mut horloge = HorlogeFictive::default();

        let trouve = interroger(
            None,
            |_| {
                appels += 1;
                assert!(appels < APPELS_MAX, "la boucle ne s'arrête pas au délai");
                Ok::<_, ()>(etat_version(1))
            },
            |_| None::<()>,
            &mut horloge,
            Duration::from_secs(2),
            Duration::from_secs(9),
        );

        assert_eq!(trouve, Ok(None));
        // 0, 2, 4, 6, 8, puis une dernière à l'échéance exacte : 9.
        assert_eq!(appels, 6);
        assert_eq!(horloge.maintenant, Duration::from_secs(9));
        assert!(debut_reel.elapsed() < Duration::from_secs(1), "l'attente n'est pas passée par l'horloge injectée");
    }

    #[test]
    fn une_erreur_de_synchronisation_interrompt_l_attente() {
        let mut appels = 0;
        let mut horloge = HorlogeFictive::default();

        let trouve = interroger(
            None,
            |_| {
                appels += 1;
                if appels == 2 {
                    Err("réseau")
                } else {
                    Ok(etat_version(1))
                }
            },
            |_| None::<()>,
            &mut horloge,
            Duration::from_secs(2),
            Duration::from_secs(60),
        );

        assert_eq!(trouve, Err("réseau"));
        assert_eq!(appels, 2);
    }

    // --- (c) le tri, dans la boucle ---------------------------------------

    #[test]
    fn chez_l_hote_ce_qui_n_est_pas_une_offre_n_interrompt_pas_l_attente() {
        // Premier tour : trois enveloppes d'un appareil d'ami, aucune n'est
        // une offre — un clair binaire, un texte qui n'est pas un bloc, et une
        // RÉPONSE périmée (bloc lisible, SDP scellé). Second tour : la vraie
        // offre. L'attente doit la trouver au second tour, sans erreur.
        let hote = Identity::generate();
        let spectateur = Identity::generate();
        let appareil_hote = appareil(1, &hote);
        let appareil_spectateur = appareil(11, &spectateur);
        let amis = vec![Ami {
            id: 5,
            discord_name: "alice".to_string(),
            appareils: vec![appareil_spectateur.clone()],
        }];

        let offre = bloc_offre(SESSION, Identity::generate().public_key());
        let reponse_perimee = bloc_reponse(SESSION_PERIMEE, Identity::generate().public_key());

        let mut tour = 0;
        let mut horloge = HorlogeFictive::default();
        let trouve = interroger(
            None,
            |_| {
                tour += 1;
                assert!(tour < APPELS_MAX, "la boucle ne s'arrête pas");
                let enveloppes = match tour {
                    1 => vec![
                        enveloppe("1", 11, &appareil_hote, &[0xFF, 0xFE, 0x00]),
                        enveloppe("2", 11, &appareil_hote, b"bonjour"),
                        enveloppe("3", 11, &appareil_hote, reponse_perimee.as_bytes()),
                    ],
                    2 => vec![enveloppe("4", 11, &appareil_hote, offre.as_bytes())],
                    _ => Vec::new(),
                };
                Ok::<_, ()>(etat(amis.clone(), enveloppes))
            },
            |e| offres_recevables(e, relever(e, &hote)).into_iter().next(),
            &mut horloge,
            CADENCE,
            ATTENTE_SPECTATEUR,
        );

        let trouve = trouve.expect("aucune erreur attendue").expect("l'offre du second tour est perdue");
        assert_eq!(
            tour, 2,
            "l'attente ne s'est pas arrêtée au tour de l'offre : soit une enveloppe qui n'en est pas \
             une a été retenue avant, soit la boucle a continué après l'avoir trouvée"
        );
        assert_eq!(trouve.texte, offre);
        assert_eq!(trouve.destinataire, appareil_spectateur);
    }

    #[test]
    fn chez_le_spectateur_une_reponse_perimee_est_ecartee_et_la_bonne_retenue() {
        // Premier tour : la réponse d'un essai précédent (autre session).
        // Second tour : la réponse à l'offre en cours. `view` doit attendre
        // la seconde, pas s'arrêter sur la première.
        let spectateur = Identity::generate();
        let appareil_spectateur = appareil(11, &spectateur);
        let appareils_de_l_ami = vec![appareil(1, &Identity::generate())];

        let perimee = bloc_reponse(SESSION_PERIMEE, Identity::generate().public_key());
        let bonne = bloc_reponse(SESSION, Identity::generate().public_key());

        let mut tour = 0;
        let mut horloge = HorlogeFictive::default();
        let trouve = interroger(
            None,
            |_| {
                tour += 1;
                assert!(tour < APPELS_MAX, "la boucle ne s'arrête pas");
                let enveloppes = match tour {
                    1 => vec![enveloppe("1", 1, &appareil_spectateur, perimee.as_bytes())],
                    2 => vec![enveloppe("2", 1, &appareil_spectateur, bonne.as_bytes())],
                    _ => Vec::new(),
                };
                Ok::<_, ()>(etat(Vec::new(), enveloppes))
            },
            |e| reponse_a_l_offre(relever(e, &spectateur), SESSION, &appareils_de_l_ami),
            &mut horloge,
            CADENCE,
            ATTENTE_SPECTATEUR,
        );

        assert_eq!(trouve, Ok(Some(bonne)));
        assert_eq!(tour, 2);
    }

    #[test]
    fn chez_le_spectateur_une_reponse_d_un_appareil_etranger_a_l_ami_est_ecartee() {
        let ami = vec![appareil(1, &Identity::generate())];
        let reponse = bloc_reponse(SESSION, Identity::generate().public_key());

        assert_eq!(reponse_a_l_offre(vec![message("1", 2, reponse.as_bytes())], SESSION, &ami), None);
        assert_eq!(
            reponse_a_l_offre(vec![message("1", 1, reponse.as_bytes())], SESSION, &ami),
            Some(reponse)
        );
    }

    #[test]
    fn la_session_lue_est_celle_du_bloc() {
        assert_eq!(session_de(&bloc_offre(SESSION, [0; 32])).unwrap(), SESSION);
        assert!(session_de("pas un bloc").is_err());
    }
}
