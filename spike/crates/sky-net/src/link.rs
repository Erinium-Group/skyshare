//! Le lien pair-à-pair : négociation ICE/DTLS et canal de données.
//!
//! # Confidentialité
//!
//! Ce module est le seul du spike à manipuler de vraies adresses réseau. Aucune
//! d'elles ne doit atteindre une sortie console, un journal ou un fichier
//! (spec §4.2). Deux conséquences concrètes sur le code ci-dessous :
//!
//! 1. **Aucune erreur de `str0m` n'est reformatée vers l'appelant.** Plusieurs
//!    variantes de `RtcError` — `RemoteSdp(String)`, `Sdp(..)` — transportent
//!    des morceaux de SDP, donc des adresses. On leur substitue systématiquement
//!    un message fixe en français.
//! 2. **Le spike n'installe aucun collecteur `tracing`.** C'est là, et nulle
//!    part ailleurs, que réside la garantie : `str0m` émet ses traces via les
//!    macros de `tracing`, qui sont inertes tant qu'aucun collecteur n'est
//!    enregistré. Rien n'est donc journalisé, quelle que soit la valeur de
//!    `RUST_LOG`.
//!
//!    La fonctionnalité `pii` de `str0m` est activée en complément, mais **elle
//!    ne suffirait pas** : elle ne masque que les emplacements que la
//!    bibliothèque enveloppe explicitement dans `Pii<T>`, et ses traces les plus
//!    bavardes formatent source et destination avec un `Debug` ordinaire. Qui
//!    brancherait un collecteur en se croyant couvert par `pii` verrait des
//!    adresses. La seule protection sur laquelle compter est l'absence de
//!    collecteur.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::net::{SocketAddr, UdpSocket};
use std::time::Instant;

use anyhow::{anyhow, Context};
use str0m::change::{SdpAnswer, SdpOffer, SdpPendingOffer};
use str0m::channel::ChannelId;
use str0m::format::CodecExtra;
use str0m::media::{Direction, MediaKind, MediaTime, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc};

use sky_crypto::Identity;

use crate::controle::MessageControle;
use crate::format::{
    FormatVideo, NIVEAU_HEVC_6_0, PROFIL_HEVC_444, PROFIL_HEVC_MAIN, PROFIL_NIVEAU_H264, TIER_MAIN,
};
use crate::handshake::Blob;
use crate::stun;

/// Pourquoi un message n'a pas pu partir. Messages fixes : aucune adresse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErreurEnvoi {
    /// Le canal de données n'est pas (ou plus) ouvert.
    CanalFerme,
    /// Le tampon d'émission est plein : l'appelant réessaie ou renonce.
    TamponPlein,
    /// `str0m` a refusé l'écriture sur le CANAL DE DONNÉES.
    EcritureRefusee,
    /// `str0m` a refusé une image sur la PISTE MÉDIA, pour une autre raison
    /// que la file de paquetisation pleine (`TropDImagesEnAttente`).
    ///
    /// Séparée d'`EcritureRefusee` par la vague finale (revue de branche, M4a) :
    /// les deux partageaient « écriture impossible sur le canal », et l'hôte
    /// lisait « Connexion interrompue : écriture impossible sur le canal » quand
    /// c'était la piste qui refusait — sur un canal de données parfaitement
    /// vivant.
    ImageRefusee,
    /// La file de paquetisation de la piste média est pleine : plus de cent
    /// images attendent d'être découpées en paquets RTP.
    ///
    /// **Récupérable, et c'est tout l'intérêt de la distinguer.** L'appelant
    /// appelle `poll`, puis réécrit la même image ; il ne l'abandonne pas.
    ///
    /// Un `poll` libère **une** place, pas la file : `str0m` ne dépile qu'une
    /// image par tour (`Media::do_payload`, un `pop_front`). Un appelant très en
    /// retard peut donc être refusé plusieurs fois de suite, et ce n'est pas un
    /// signe d'échec — c'est le rythme d'une seule image par `poll` qui reprend.
    ///
    /// C'est le seul refus que la sonde du 27/09/2026 ait jamais vu sur ce
    /// chemin, et elle ne l'a vu qu'en écrivant sans poller : 0 refus sur 2593
    /// écritures à 12 Mbps et 0 sur 21552 à 100 Mbps quand la boucle sert le
    /// réseau entre deux images.
    ///
    /// Le pendant du canal de données est `TamponPlein`, que `hote.rs` pilote
    /// déjà de cette façon.
    TropDImagesEnAttente,
    /// Le message n'a pas pu être sérialisé.
    Serialisation,
    /// La piste média n'est pas encore négociée, ou ne l'est plus.
    PisteFermee,
    /// La piste est installée mais le format négocié n'est pas parmi les types
    /// de charge du rédacteur.
    ///
    /// Distinct de `PisteFermee` : là, la session n'a pas encore le média ; ici
    /// elle l'a, et c'est l'entente sur le codec qui manque. **Aucun test ne
    /// l'exerce** : il faudrait un correspondant qui accepte la piste sans
    /// retenir un de nos formats, et `str0m` écarte plutôt la ligne de média
    /// entière quand aucun codec ne concorde. Le cas est nommé plutôt que replié sur
    /// `ImageRefusee` parce que la cause est diagnostiquable et que la
    /// confondre avec un refus d'écriture égarerait le diagnostic.
    CodecNonNegocie,
}

impl std::fmt::Display for ErreurEnvoi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::CanalFerme => "canal de données pas ouvert",
            Self::TamponPlein => "tampon d'émission plein",
            Self::EcritureRefusee => "écriture impossible sur le canal de données",
            Self::ImageRefusee => "la piste vidéo a refusé l'image",
            Self::TropDImagesEnAttente => "trop d'images en attente de paquetisation",
            Self::Serialisation => "message impossible à sérialiser",
            Self::PisteFermee => "piste vidéo pas encore négociée",
            Self::CodecNonNegocie => "aucun codec vidéo commun avec le correspondant",
        })
    }
}

impl std::error::Error for ErreurEnvoi {}

/// Étiquette du canal de données. Une seule pour tout le spike.
const CANAL: &str = "sky";

/// Taille maximale d'un datagramme UDP accepté (limite interne de `str0m`).
const TAILLE_DATAGRAMME: usize = 2000;

/// Fréquence de l'horloge RTP vidéo, en unités par seconde (RFC 3551).
const HORLOGE_RTP: u64 = 90_000;

/// Ce que la boucle d'appel apprend à chaque tour de `poll`.
pub enum LinkEvent {
    /// ICE a trouvé un chemin et DTLS est établi.
    Connected,
    /// Une unité d'accès vidéo complète est arrivée sur la piste média.
    ///
    /// `donnees` est de l'Annex-B, tel que le dépaquetiseur (RFC 7798 pour HEVC,
    /// RFC 6184 pour H.264) l'a réassemblé — donc tel que l'encodeur l'avait
    /// produit.
    Image {
        donnees: Vec<u8>,
        /// Horodatage RTP ramené en millisecondes. Son origine est celle que
        /// l'émetteur a choisie : c'est une durée depuis SON départ, pas une
        /// heure.
        horodatage_ms: u64,
        /// Image clé, au sens du dépaquetiseur (HEVC ou H.264).
        cle: bool,
        /// Faux dès qu'un paquet RTP a manqué entre l'image précédente et
        /// celle-ci : le flux a un trou, et cette image ne se décode
        /// peut-être pas.
        ///
        /// **Branché par le spectateur** (`sky-partage/src/spectateur.rs`,
        /// `Visionnage::sur_image`) depuis la tâche 9. Sous GOP infini — un seul IDR pour 901
        /// images, mesuré au jalon 0 — c'est le SEUL moyen d'apprendre qu'un
        /// morceau manque et qu'il faut demander une image clé
        /// (`MessageControle::DemandeImageCle`). Sans lui, le spectateur ne
        /// découvrirait la perte que si le décodeur échouait — or, mesuré au
        /// jalon 2 (`sky-decode/tests/aller_retour.rs`), NVDEC rend des images
        /// sans aucune erreur sur un flux privé de son point d'accès : la perte
        /// pourrait ne jamais se signaler. Qui le supprimerait rouvrirait ce trou.
        sans_perte: bool,
        /// Le format lu sur le paquet. Il sert à prouver que les deux côtés
        /// s'accordent (tests) ; le spectateur choisit son décodeur par
        /// `PeerLink::format_negocie`.
        format: Option<FormatVideo>,
    },
    /// Un message de contrôle est arrivé sur le canal de données.
    Controle(MessageControle),
    /// Rien de neuf ; rappeler `poll` sous peu.
    Idle,
    /// Le lien est perdu ou n'a jamais pu s'établir. Message déjà rédigé pour
    /// l'utilisateur, et garanti sans adresse.
    Failed(String),
}

pub struct PeerLink {
    rtc: Rtc,
    socket: UdpSocket,
    /// Adresse depuis laquelle nous émettons, telle qu'annoncée à `str0m`.
    /// Jamais affichée.
    locale: SocketAddr,
    identity: Identity,
    /// L'offre en attente de réponse, côté offrant uniquement.
    pending: Option<SdpPendingOffer>,
    canal: Option<ChannelId>,
    /// Identifiant de la piste média vidéo.
    ///
    /// Deux sources, et elles ne datent pas du même instant : l'offrant le tient
    /// de son propre `add_media`, donc dès sa construction, avant toute
    /// négociation ; le répondant le tient de `Event::MediaAdded`, donc une fois
    /// l'offre acceptée. Le `Mid` est le même des deux côtés.
    ///
    /// Renseigné ne veut donc pas dire utilisable, et ce n'est pas ce champ qui
    /// le garde : `Rtc::writer` ne délivre rien avant que la session ait
    /// installé le média, et `ecrire_image` refuse alors — mesuré des deux côtés
    /// par `ecrire_une_image_avant_la_negociation_est_refuse`.
    piste: Option<Mid>,
    peer_key: Option<[u8; 32]>,
    connected: bool,
    /// Erreurs d'émission et de réception avalées sur le port UDP.
    ///
    /// Elles ne sont pas fatales — ICE réémet — mais les taire entièrement
    /// ferait attribuer au NAT un échec qui vient peut-être d'ailleurs. On les
    /// compte pour que le diagnostic final puisse nuancer sa conclusion.
    erreurs_socket: u64,
    /// Adresses des serveurs STUN, pour distinguer leurs réponses de celles du
    /// pair. Sans ce filtre, le battement de maintien gonfle le compteur de
    /// paquets reçus et fait croire que le correspondant répond.
    serveurs_stun: Vec<SocketAddr>,
    /// Identifiant de la négociation en cours, pour refuser une réponse
    /// destinée à une offre précédente.
    session: [u8; 4],
    /// Datagrammes réellement émis et reçus sur le port UDP.
    ///
    /// Sans ces deux nombres, un échec de négociation est indiscernable : on ne
    /// sait pas si l'on émet dans le vide, si l'on reçoit sans pouvoir répondre,
    /// ou si rien ne circule du tout. Chacun de ces cas a une cause différente.
    paquets_emis: u64,
    /// Repartition des emissions selon la nature de la destination.
    ///
    /// Un SDP annonce deux adresses : celle du reseau local et celle vue depuis
    /// internet. Emettre vers la premiere revient a chercher le correspondant
    /// dans son propre reseau — les paquets n'en sortent jamais. Sans cette
    /// distinction, « 12 emis / 0 recu » ne dit pas si l'on vise le bon endroit.
    emis_vers_prive: u64,
    emis_vers_public: u64,
    /// Paquets du pair recus AVANT que l'agent ne soit reveille.
    ///
    /// Ils sont mis de cote puis injectes au premier `poll`, pour ne rien
    /// perdre du tout premier echange.
    en_attente: Vec<(Vec<u8>, SocketAddr)>,
    /// Adresses annoncees par le pair, extraites du SDP.
    ///
    /// Servent a percer le NAT *local* pendant l'attente : chaque paquet emis
    /// vers ces adresses ouvre un passage entrant pour elles dans notre box.
    /// Sans cela, le cote qui attend passivement garde sa box fermee et les
    /// paquets de l'autre sont jetes a l'entree.
    ///
    /// Depuis l'inversion (D2), ce cote qui attend est le **repondant** — donc
    /// l'hote : il rend sa reponse, puis il patiente jusqu'au premier paquet de
    /// l'offrant. C'est pourquoi `repondant` remplit ce champ des sa
    /// construction, a partir du SDP de l'offre recue. L'offrant, lui, ne
    /// connait les adresses du pair qu'a `accepter_reponse` — mais il n'en a pas
    /// besoin avant, puisque c'est lui qui prend l'initiative d'emettre.
    /// Verrouille par `le_repondant_connait_les_cibles_des_sa_construction`.
    cibles_pair: Vec<SocketAddr>,
    paquets_recus: u64,
    /// Messages arrivés sur le canal de données sans être un message de
    /// contrôle connu. Ignorés, jamais propagés : une version plus récente de
    /// l'application en enverra que celle-ci ne comprend pas, et couper le lien
    /// pour cela serait un défaut.
    messages_illisibles: u64,
    /// Origine du temps tel que `str0m` le voit.
    horloge: Instant,
    /// Instant réel du premier `poll`. Voir `maintenant`.
    depart: Option<Instant>,
}

impl PeerLink {
    /// Côté offrant : produit l'offre à envoyer au correspondant.
    ///
    /// C'est un rôle de **signaling**, pas un rôle média : celui qui offre n'est
    /// pas forcément celui qui envoie la vidéo. Les deux étaient confondus dans
    /// les noms `host`/`viewer`, ce qui est devenu faux dès que le sens de la
    /// négociation s'est inversé.
    ///
    /// Le `Pacer` n'intervient pas ici : le pilotage du débit appartient à la
    /// commande, pas au lien.
    ///
    /// # Confidentialité — ce que cette fonction expose, et de qui
    ///
    /// L'offre voyage **en clair** : `offrant` ne reçoit aucune clé de
    /// destinataire, donc il n'a rien avec quoi sceller. Seule la réponse l'est.
    /// Quiconque tient le bloc lit les adresses annoncées dedans.
    ///
    /// Ce que l'inversion change, c'est **de qui** ces adresses sont. Avant, le
    /// bloc en clair était produit par l'hôte : il publiait ses adresses avant
    /// de savoir à qui il parlait — deux adresses lisibles, mesuré sur le spike.
    /// Depuis la décision D2, c'est le spectateur qui offre : le bloc en clair
    /// porte désormais les adresses de **celui qui demande à regarder**, et
    /// l'hôte n'en publie aucune tant qu'il n'a pas ouvert l'offre.
    ///
    /// L'exposition n'est donc pas supprimée, elle est déplacée — et elle le
    /// reste après cette tâche. Sceller l'offre devient possible à ce jalon,
    /// parce que l'offrant peut y obtenir la clé du destinataire depuis la boîte
    /// aux lettres ; mais cela demande une clé de plus en paramètre, et ce n'est
    /// pas cette tâche-ci qui l'ajoute. Ne pas lire ce commentaire comme une
    /// promesse tenue.
    ///
    /// `formats` : les formats vidéo que ce côté sait **décoder**, dans l'ordre
    /// de l'offre ; liste vide refusée.
    pub fn offrant(identity: Identity, formats: &[FormatVideo]) -> anyhow::Result<(Self, String)> {
        anyhow::ensure!(!formats.is_empty(), "aucun format vidéo à annoncer");
        let (mut rtc, horloge) = nouveau_rtc(formats);
        let (socket, locale) = Self::socket_et_candidats(&mut rtc)?;

        let mut change = rtc.sdp_api();
        // Canal ordonne ET fiable (reglage par defaut de str0m), choisi pour
        // l'usage du canal : des messages de controle, rares et minuscules.
        //
        // Ce n'etait pas le reglage du jalon 0. La, le canal portait la video,
        // et un canal fiable etait le pire choix (RTT de 800 a 1600 ms, debit
        // effondre a 1 Mbps pour une cible de 10, tampon d'emission sature) :
        // on l'avait rendu non ordonne avec une duree de vie de 150 ms, ce qui
        // avait fait passer le debit a 12,4 Mbps.
        //
        // Depuis la tache 8, ce canal ne porte PLUS de video : l'hote ecrit ses
        // unites d'acces sur la piste media (`ecrire_image`), et son dernier
        // appelant de `envoyer_octets_bruts` a disparu avec le decoupage maison.
        // Ce reglage n'est donc justifie que par le controle.
        //
        // Garder l'ancien reglage ferait perdre definitivement un message
        // arrive trop tard — un `PartageArrete` jamais reemis laisserait le
        // spectateur devant un flux fige sans explication. On retransmet donc.
        // Le cout, non mesure, est la latence d'une retransmission. On compte
        // qu'elle restera acceptable parce que la spec veut des messages rares
        // (au plus un par seconde) ; cette limitation n'est pas encore
        // implementee, elle viendra avec les taches 8 et 9.
        change.add_channel_with_config(str0m::channel::ChannelConfig {
            label: CANAL.to_string(),
            reliability: str0m::channel::Reliability::Reliable,
            ..Default::default()
        });

        // La vidéo, elle, voyage sur une piste média RTP — c'est tout l'objet du
        // jalon 2. Le jalon 0 l'avait fait passer par le canal de données, ce
        // pour quoi il n'a jamais été fait : mesuré sur deux machines et deux
        // réseaux, 16 % d'échecs d'envoi (2611 sur 16349), parce que le tampon
        // d'émission SCTP se remplit et que le retard n'est pas récupérable.
        //
        // `RecvOnly` est le sens réel, et il est honnête : l'offrant est le
        // SPECTATEUR (décision D2), il ne diffuse rien. Le répondant — l'hôte —
        // se retrouve donc en émission dans sa réponse.
        let piste = change.add_media(MediaKind::Video, Direction::RecvOnly, None, None, None);

        let (offer, pending) = change
            .apply()
            .ok_or_else(|| anyhow!("aucun changement à négocier"))?;

        let mut session = [0u8; 4];

        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut session);

        let blob = Blob {
            session,
            public_key: identity.public_key(),
            sealed_sdp: crate::handshake::comprimer(&offer.to_sdp_string()),
        };

        Ok((
            Self {
                rtc,
                socket,
                locale,
                identity,
                pending: Some(pending),
                canal: None,
                piste: Some(piste),
                peer_key: None,
                connected: false,
                erreurs_socket: 0,
                serveurs_stun: stun::serveurs_autorises(),
                session,
                paquets_emis: 0,
                emis_vers_prive: 0,
                emis_vers_public: 0,
                en_attente: Vec::new(),
                cibles_pair: Vec::new(),
                paquets_recus: 0,
                messages_illisibles: 0,
                horloge,
                depart: None,
            },
            blob.to_text(),
        ))
    }

    /// Côté répondant : consomme l'offre, produit la réponse scellée.
    ///
    /// Rôle de signaling lui aussi : le répondant peut parfaitement être celui
    /// qui émettra ensuite la vidéo.
    ///
    /// `formats` : les formats vidéo que ce côté sait **encoder** ; liste vide
    /// refusée.
    pub fn repondant(
        identity: Identity,
        offre_texte: &str,
        formats: &[FormatVideo],
    ) -> anyhow::Result<(Self, String)> {
        anyhow::ensure!(!formats.is_empty(), "aucun format vidéo à annoncer");
        let blob = Blob::from_text(offre_texte)?;
        let sdp = crate::handshake::decomprimer(&blob.sealed_sdp)?;
        // L'erreur de `str0m` cite le SDP fautif : on ne la propage pas.
        let offer =
            SdpOffer::from_sdp_string(&sdp).map_err(|_| anyhow!("bloc illisible ou incomplet"))?;

        let (mut rtc, horloge) = nouveau_rtc(formats);
        let (socket, locale) = Self::socket_et_candidats(&mut rtc)?;

        let cibles_pair = cibles_depuis_sdp(&sdp);
        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|_| anyhow!("bloc refusé — il ne décrit pas une session utilisable"))?;

        // La réponse porte nos adresses : elle est scellée avec la clé de l'hôte,
        // donc seul lui pourra la lire, même si le bloc transite par ailleurs.
        // Comprimer AVANT de sceller : un contenu chiffré ne se comprime pas.
        let sealed = identity.seal(
            &blob.public_key,
            &crate::handshake::comprimer(&answer.to_sdp_string()),
        );
        let reponse = Blob {
            // Renvoyer l'identifiant recu prouve a l'emetteur que cette
            // reponse concerne bien son offre en cours.
            session: blob.session,
            public_key: identity.public_key(),
            sealed_sdp: sealed,
        };

        Ok((
            Self {
                rtc,
                socket,
                locale,
                identity,
                pending: None,
                canal: None,
                // Le répondant apprend sa piste par `Event::MediaAdded` : il ne
                // la crée pas, il accepte celle de l'offre.
                piste: None,
                peer_key: Some(blob.public_key),
                connected: false,
                erreurs_socket: 0,
                serveurs_stun: stun::serveurs_autorises(),
                session: blob.session,
                paquets_emis: 0,
                emis_vers_prive: 0,
                emis_vers_public: 0,
                en_attente: Vec::new(),
                cibles_pair,
                paquets_recus: 0,
                messages_illisibles: 0,
                horloge,
                depart: None,
            },
            reponse.to_text(),
        ))
    }

    /// Maintient le port ouvert tant que le garde rendu est vivant.
    ///
    /// À démarrer avant toute attente d'une saisie humaine. Le mapping NAT du
    /// port qu'on vient d'annoncer expire en 30 à 120 s sans trafic, alors que
    /// l'échange des blocs prend couramment plusieurs minutes.
    pub fn maintenir_mapping(&self) -> anyhow::Result<GardeMapping> {
        let socket = self
            .socket
            .try_clone()
            .context("duplication du port UDP impossible")?;
        // On vise les serveurs STUN (pour garder notre adresse publique stable)
        // ET le pair lui-meme (pour ouvrir notre box a ses paquets). Ce second
        // point est ce qui manquait : un cote qui attend sans jamais emettre
        // garde sa box fermee, et les paquets de l'autre sont jetes a l'entree.
        // Ces envois ne passent pas par l'agent, donc son horloge ne demarre
        // pas.
        //
        // `cibles_pair` est lu ici, a l'appel : il est vide tant qu'on ne
        // connait pas les adresses du pair. Chez l'offrant c'est le cas jusqu'a
        // `accepter_reponse`, et ce n'est pas genant — c'est le repondant qui
        // attend, et lui les connait des sa construction.
        let serveurs = stun::serveurs_autorises();
        let cibles = self.cibles_pair.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let drapeau = stop.clone();

        // 15 s : bien en deçà des 30 s du mapping le plus court observé.
        let handle = std::thread::spawn(move || {
            while !drapeau.load(Ordering::Relaxed) {
                stun::battement(&socket, &serveurs);
                stun::percer(&socket, &cibles);
                for _ in 0..30 {
                    if drapeau.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        });

        Ok(GardeMapping {
            stop,
            handle: Some(handle),
        })
    }

    /// Compteurs de circulation sur le port UDP : émis, reçus, erreurs.
    ///
    /// Destinés au message d'échec : ils disent lequel des trois scénarios
    /// s'est produit, là où « NAT strict » n'était qu'une conjecture.
    pub fn trafic(&self) -> (u64, u64, u64) {
        (self.paquets_emis, self.paquets_recus, self.erreurs_socket)
    }

    /// Emissions vers une adresse de reseau local, puis vers internet.
    pub fn destinations(&self) -> (u64, u64) {
        (self.emis_vers_prive, self.emis_vers_public)
    }

    /// Côté offrant : intègre la réponse du répondant.
    pub fn accepter_reponse(&mut self, texte: &str) -> anyhow::Result<()> {
        let blob = Blob::from_text(texte)?;
        if blob.session != self.session {
            return Err(anyhow!(
                "cette réponse concerne une autre négociation — c'est probablement                  le bloc d'un essai précédent. Relance chacun votre commande et                  échangez des blocs frais."
            ));
        }
        let sdp = self.identity.open(&blob.sealed_sdp)?;
        let sdp = crate::handshake::decomprimer(&sdp)?;
        self.cibles_pair = cibles_depuis_sdp(&sdp);
        let publiques = self.cibles_pair.len();
        let total = sdp.lines().filter(|l| l.starts_with("a=candidate:")).count();
        println!("  Adresses annoncees par le correspondant : {total} au total, dont {publiques} joignables depuis internet.");
        if publiques == 0 {
            println!(">>> Aucune adresse publique de son cote : il n'annonce que son");
            println!("        reseau local, ou nous ne pourrons jamais l'atteindre.");
            println!("        Sa decouverte d'adresse publique a echoue — qu'il lance");
            println!("        `sky-probe netcheck` : son pare-feu bloque probablement UDP.");
        }
        let answer = SdpAnswer::from_sdp_string(&sdp)
            .map_err(|_| anyhow!("réponse illisible ou incomplète"))?;

        let pending = self
            .pending
            .take()
            .ok_or_else(|| anyhow!("aucune offre en attente de réponse"))?;

        self.rtc
            .sdp_api()
            .accept_answer(pending, answer)
            .map_err(|_| anyhow!("réponse refusée — elle ne correspond pas à l'offre envoyée"))?;

        self.peer_key = Some(blob.public_key);
        Ok(())
    }

    /// L'instant à présenter à `str0m`, sur une horloge qui ne démarre qu'au
    /// premier `poll`.
    ///
    /// `str0m` est sans-IO : son temps n'avance que lorsqu'on lui en donne. Ses
    /// minuteries — dont la poignée de main DTLS, qui abandonne au bout d'une
    /// trentaine de secondes — ne courent donc que pendant qu'on l'interroge.
    ///
    /// C'est décisif ici. Entre la création du lien et le premier paquet du
    /// correspondant, il s'écoule le temps qu'un humain fasse transiter un bloc
    /// de 3 800 caractères par une messagerie. Si l'horloge partait de la
    /// construction, deux choses casseraient :
    ///
    /// - le répondant, qui doit scruter pendant cette attente, verrait sa
    ///   poignée de main DTLS expirer avant que l'offrant n'ait commencé
    ///   (mesuré : abandon à 30 s, quoi qu'on règle côté ICE) ;
    /// - l'offrant, resté bloqué sur son invite de saisie, présenterait d'un
    ///   coup à `str0m` un bond de plusieurs minutes au premier `poll`.
    ///
    /// En faisant démarrer l'horloge au premier `poll`, les deux disparaissent.
    /// Cela n'assouplit **aucun** délai d'abandon : les 8 secondes du document
    /// d'architecture sont comptées sur l'horloge réelle, dans `cmd_host`.
    fn maintenant(&mut self) -> Instant {
        let depart = *self.depart.get_or_insert_with(Instant::now);
        self.horloge + depart.elapsed()
    }

    /// Y a-t-il un datagramme en attente, sans faire avancer l'horloge ?
    ///
    /// C'est ce que le côté qui attend appelle pendant qu'il guette son
    /// correspondant : le paquet est seulement observé, pas consommé — le
    /// `poll` suivant le lira normalement — et `str0m` n'est pas touché, donc
    /// aucune de ses minuteries ne court.
    /// Vrai dès qu'un datagramme **du pair** a été reçu.
    ///
    /// L'ancienne version regardait simplement si un datagramme quelconque
    /// attendait sur le port. Depuis l'ajout du battement de maintien, les
    /// serveurs STUN répondent en permanence : le côté qui attend prenait ces
    /// réponses pour un contact du correspondant, sautait son attente et
    /// abandonnait avant même que l'autre ait collé son bloc.
    ///
    /// On s'appuie donc sur le compteur alimenté par `poll`, qui filtre déjà
    /// les réponses des serveurs. L'appelant doit appeler `poll` en boucle —
    /// ce qui est de toute façon nécessaire pour que l'agent ICE émette.
    pub fn contact_etabli(&self) -> bool {
        self.paquets_recus > 0
    }

    /// Un tour de la boucle : vider les sorties de `str0m`, écouter le socket,
    /// avancer le temps. Ne bloque jamais.
    pub fn poll(&mut self) -> anyhow::Result<LinkEvent> {
        if !self.rtc.is_alive() {
            return Ok(LinkEvent::Failed("session déclarée morte par l'agent".into()));
        }

        // Injecter d'abord ce qui a ete recueilli pendant l'attente patiente.
        if !self.en_attente.is_empty() {
            let differes = std::mem::take(&mut self.en_attente);
            let destination = self.locale;
            for (donnees, source) in differes {
                let instant = self.maintenant();
                if let Ok(contents) = donnees.as_slice().try_into() {
                    let _ = self.rtc.handle_input(Input::Receive(
                        instant,
                        Receive {
                            proto: Protocol::Udp,
                            source,
                            destination,
                            contents,
                        },
                    ));
                }
            }
        }

        // 1. Vider les sorties de str0m.
        loop {
            let sortie = match self.rtc.poll_output() {
                Ok(s) => s,
                // Le message d'origine peut contenir du SDP, donc des adresses.
                Err(e) => {
                    // On nomme la CATEGORIE sans le message : celui-ci peut
                    // contenir du SDP, donc des adresses.
                    let categorie = match &e {
                        str0m::RtcError::Dtls(_) => "poignée de main chiffrée (DTLS)",
                        str0m::RtcError::Ice(_) => "agent ICE",
                        str0m::RtcError::Io(_) => "entrée/sortie réseau",
                        str0m::RtcError::Net(_) => "lecture d'un paquet",
                        str0m::RtcError::Sdp(_) => "description de session",
                        str0m::RtcError::RemoteSdp(_) => "description distante",
                        str0m::RtcError::Rtp(_) => "flux RTP",
                        _ => "autre",
                    };
                    return Ok(LinkEvent::Failed(format!("erreur en sortie — {categorie}")));
                }
            };

            match sortie {
                Output::Timeout(_) => break,
                Output::Transmit(t) => {
                    // Un envoi qui échoue (réseau momentanément indisponible) n'est
                    // pas fatal : ICE réémettra. L'erreur nommerait la destination,
                    // on ne la propage donc pas — mais on la compte.
                    if self.socket.send_to(&t.contents, t.destination).is_err() {
                        self.erreurs_socket += 1;
                    } else {
                        self.paquets_emis += 1;
                        if adresse_privee(&t.destination) {
                            self.emis_vers_prive += 1;
                        } else {
                            self.emis_vers_public += 1;
                        }
                    }
                }
                Output::Event(e) => match e {
                    Event::IceConnectionStateChange(etat) => {
                        // Comparaison sur la variante réelle, pas sur son affichage
                        // de débogage : ce dernier n'est pas une interface stable.
                        if matches!(
                            etat,
                            IceConnectionState::Connected | IceConnectionState::Completed
                        ) && !self.connected
                        {
                            self.connected = true;
                            return Ok(LinkEvent::Connected);
                        }
                        if etat == IceConnectionState::Disconnected && self.connected {
                            return Ok(LinkEvent::Failed("connexion interrompue".into()));
                        }
                    }
                    Event::ChannelOpen(id, _) => self.canal = Some(id),
                    Event::ChannelData(d) => match serde_json::from_slice(&d.data) {
                        Ok(message) => return Ok(LinkEvent::Controle(message)),
                        // Illisible : on compte, on ne décrit pas (le contenu
                        // vient du pair) et on poursuit le vidage des sorties.
                        Err(_) => self.messages_illisibles += 1,
                    },
                    Event::ChannelClose(_) => self.canal = None,
                    Event::MediaAdded(media) => {
                        if media.kind == MediaKind::Video {
                            self.piste = Some(media.mid);
                        }
                    }
                    Event::MediaData(donnees) => {
                        // `is_keyframe` vient du dépaquetiseur (HEVC ou H.264),
                        // qui lit l'en-tête de NAL. On ne le devine pas depuis
                        // les octets ici.
                        let cle = matches!(
                            donnees.codec_extra,
                            CodecExtra::H265(extra) if extra.is_keyframe
                        ) || matches!(
                            donnees.codec_extra,
                            CodecExtra::H264(extra) if extra.is_keyframe
                        );
                        return Ok(LinkEvent::Image {
                            donnees: donnees.data.to_vec(),
                            horodatage_ms: en_millisecondes(donnees.time),
                            cle,
                            sans_perte: donnees.contiguous,
                            format: FormatVideo::depuis_parametres(&donnees.params),
                        });
                    }
                    _ => {}
                },
            }
        }

        // 2. Injecter ce qui arrive du socket.
        let mut buf = vec![0u8; TAILLE_DATAGRAMME];
        match self.socket.recv_from(&mut buf) {
            Ok((n, source)) => {
                // Une réponse d'un serveur STUN est le retour de notre propre
                // battement de maintien, pas un signe de vie du correspondant. La
                // compter fausserait le diagnostic, et la passer à l'agent ICE
                // n'aurait aucun sens.
                // Sortir ici court-circuiterait le `Input::Timeout` de fin de
                // fonction : l'agent ICE cesserait d'avancer tant que des reponses
                // STUN arrivent. On ignore le paquet sans quitter le cycle.
                let percage = n == 1 && buf[0] == stun::OCTET_PERCAGE;
                let du_pair = !self.serveurs_stun.contains(&source) && !percage;
                if du_pair {
                    self.paquets_recus += 1;
                }
                buf.truncate(n);
                // Un datagramme illisible (parasite, scan de port) est ignoré :
                // il ne doit ni interrompre la négociation ni être décrit.
                if du_pair {
                    if let Ok(contents) = buf.as_slice().try_into() {
                        let instant = self.maintenant();
                        let recu = Receive {
                            proto: Protocol::Udp,
                            source,
                            destination: self.locale,
                            contents,
                        };
                        if self
                            .rtc
                            .handle_input(Input::Receive(instant, recu))
                            .is_err()
                        {
                            return Ok(LinkEvent::Failed("erreur à l'injection d'un paquet reçu".into()));
                        }
                    }
                }
            }
            Err(ref e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            // Sous Windows, un ICMP « port unreachable » remonte en erreur sur le
            // socket UDP. Ce n'est pas fatal pendant les sondages ICE, mais un
            // échec final mérite de le savoir.
            Err(_) => self.erreurs_socket += 1,
        }

        let instant = self.maintenant();
        if self.rtc.handle_input(Input::Timeout(instant)).is_err() {
            return Ok(LinkEvent::Failed("connexion interrompue".into()));
        }

        Ok(LinkEvent::Idle)
    }

    /// Envoie un message de contrôle sur le canal de données.
    ///
    /// Échoue tant que le canal n'est pas ouvert, et quand le tampon d'émission
    /// est plein — l'appelant décide alors de réessayer ou de laisser tomber le
    /// message.
    pub fn envoyer_controle(&mut self, message: &MessageControle) -> Result<(), ErreurEnvoi> {
        // Ne peut pas échouer pour l'énumération actuelle (variantes sans
        // donnée) ; le cas est nommé pour ce qu'il est si elle évolue.
        let octets = serde_json::to_vec(message).map_err(|_| ErreurEnvoi::Serialisation)?;
        self.ecrire(&octets)
    }

    /// Écrit une unité d'accès vidéo sur la piste média, dans le format négocié
    /// (`format_negocie`).
    ///
    /// `unite` est de l'**Annex-B**, tel que NVENC le produit : le paquetiseur
    /// RFC 7798 (HEVC) ou RFC 6184 (H.264) de `str0m` le consomme sans
    /// conversion. `horodatage_ms` est une
    /// durée depuis le départ de l'émetteur, pas une heure.
    ///
    /// # Ce qui remplace le canal de données, et pourquoi
    ///
    /// Le jalon 0 envoyait la vidéo par le canal de données : mesuré sur deux
    /// machines et deux réseaux, **16 % d'échecs d'envoi** (2611 sur 16349), le
    /// tampon d'émission SCTP se remplissant sans que le retard soit
    /// récupérable. Sur la piste média, la sonde du 27/09/2026 n'a mesuré
    /// **aucun refus** — 0 sur 2593 écritures à 12 Mbps, 0 sur 21552 à
    /// 100 Mbps — **en boucle locale** : la comparaison avec les 16 % n'est pas
    /// faite à réseau égal.
    ///
    /// # Le seul refus possible, et le geste attendu de l'appelant
    ///
    /// `str0m` refuse l'écriture au-delà de cent images en attente de
    /// paquetisation (`RtcError::WriteWithoutPoll`). Ce refus rend
    /// **`TropDImagesEnAttente`, et lui seul** : il est récupérable, l'appelant
    /// appelle `poll` puis **réécrit la même image**, il ne l'abandonne pas.
    /// Attention à ce qu'un `poll` fait au juste — il libère **une** place et non
    /// la file, `str0m` ne dépilant qu'une image par tour : plusieurs refus
    /// d'affilée sont normaux quand on a beaucoup de retard. Toute autre erreur
    /// rend `ImageRefusee` et n'a pas de relance connue.
    ///
    /// **Cette file est LOCALE.** Elle se vide pendant nos propres `poll`
    /// (`Media::do_payload`), sans pacer (`NullPacer`, `enable_bwe` n'étant pas
    /// activé) et sans rien attendre du correspondant. Un refus dit donc que la
    /// boucle de l'hôte n'a pas servi le lien assez souvent — pas que le réseau
    /// est lent.
    ///
    /// La sonde du 27/09/2026 n'a mesuré **aucun** refus (0 sur 21552
    /// écritures à 100 Mbps), **en boucle locale**, avec une boucle qui sert le
    /// réseau entre deux images — ce que fait `hote.rs`. C'est une mesure, pas
    /// une garantie : ce commentaire disait « ne rencontre jamais », et
    /// renvoyait au canal de données, que la vidéo n'emprunte plus.
    pub fn ecrire_image(&mut self, unite: &[u8], horodatage_ms: u64) -> Result<(), ErreurEnvoi> {
        let mid = self.piste.ok_or(ErreurEnvoi::PisteFermee)?;
        // Avant `writer`, qui emprunte `self.rtc` : `maintenant` emprunte `self`.
        let instant = self.maintenant();
        // Avant `writer` aussi : `format_negocie` lit `self.rtc`. Le refus
        // n'est tranché qu'après `writer` : tant que la session n'a pas installé
        // le média (offrant avant la réponse), c'est `PisteFermee`, pas un
        // défaut d'entente sur le codec.
        let format = self.format_negocie();

        let writer = self.rtc.writer(mid).ok_or(ErreurEnvoi::PisteFermee)?;
        let format = format.ok_or(ErreurEnvoi::CodecNonNegocie)?;
        // Le type de charge n'est pas supposé : `payload_params` ne rend que ce
        // que le correspondant a effectivement retenu, et le format négocié doit
        // y figurer.
        let pt = writer
            .payload_params()
            .map(|params| params.pt())
            .find(|pt| *pt == format.type_de_charge().into())
            .ok_or(ErreurEnvoi::CodecNonNegocie)?;

        writer
            .write(
                pt,
                instant,
                MediaTime::from_90khz(horodatage_ms * HORLOGE_RTP / 1000),
                unite,
            )
            // La variante est lue, pas le message : celui de `RtcError` peut
            // porter du SDP, donc des adresses.
            .map_err(|e| match e {
                str0m::RtcError::WriteWithoutPoll => ErreurEnvoi::TropDImagesEnAttente,
                _ => ErreurEnvoi::ImageRefusee,
            })
    }

    /// Le format que les deux côtés utilisent : le premier de
    /// `FormatVideo::PREFERENCE` dont le type de charge a été retenu par la
    /// négociation. Même fonction des deux côtés, donc même réponse, sans
    /// dépendre de l'ordre de la réponse SDP (spec §4).
    pub fn format_negocie(&self) -> Option<FormatVideo> {
        let media = self.rtc.media(self.piste?)?;
        let retenus = media.remote_pts();
        FormatVideo::PREFERENCE
            .into_iter()
            .find(|f| retenus.contains(&f.type_de_charge().into()))
    }

    /// Vrai dès que la piste média vidéo est connue du lien.
    ///
    /// Côté offrant, c'est dès sa construction — il la crée. Côté répondant,
    /// c'est à `Event::MediaAdded`, donc quand la négociation a abouti. C'est ce
    /// second cas qui intéresse un diffuseur : depuis la décision D2, l'hôte est
    /// le répondant.
    pub fn piste_ouverte(&self) -> bool {
        self.piste.is_some()
    }

    fn ecrire(&mut self, data: &[u8]) -> Result<(), ErreurEnvoi> {
        let id = self.canal.ok_or(ErreurEnvoi::CanalFerme)?;
        let mut canal = self.rtc.channel(id).ok_or(ErreurEnvoi::CanalFerme)?;
        let accepte = canal
            .write(true, data)
            .map_err(|_| ErreurEnvoi::EcritureRefusee)?;
        if !accepte {
            return Err(ErreurEnvoi::TamponPlein);
        }
        Ok(())
    }

    /// Nombre de messages reçus sur le canal de données et ignorés faute d'être
    /// un message de contrôle connu.
    pub fn messages_illisibles(&self) -> u64 {
        self.messages_illisibles
    }

    /// Vrai dès qu'ICE a trouvé un chemin — c'est-à-dire dès que le perçage de
    /// NAT a réussi, indépendamment de ce que DTLS et SCTP feront ensuite.
    ///
    /// C'est la distinction qui permet à un diagnostic d'échec de ne pas accuser
    /// le NAT à tort.
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Nombre d'erreurs d'émission ou de réception avalées sur le port UDP.
    ///
    /// Un diagnostic d'échec doit se nuancer quand ce compteur n'est pas nul :
    /// la cause peut être locale et n'avoir rien à voir avec le NAT d'en face.
    pub fn erreurs_socket(&self) -> u64 {
        self.erreurs_socket
    }

    /// Vrai dès que le canal de données est utilisable par `send`.
    pub fn canal_ouvert(&self) -> bool {
        self.canal.is_some()
    }

    /// Clé publique du correspondant, connue dès la lecture de son bloc.
    pub fn peer_key(&self) -> Option<[u8; 32]> {
        self.peer_key
    }

    /// Adresses vers lesquelles le battement de maintien perce le NAT local.
    ///
    /// Réservé aux tests : le percage lui-même n'est observable que sur deux
    /// réseaux réels, mais le fait que le côté **qui attend** connaisse ces
    /// adresses dès sa construction, lui, est vérifiable ici. L'API publique de
    /// `PeerLink` reste inchangée.
    #[cfg(test)]
    fn cibles_pair(&self) -> &[SocketAddr] {
        &self.cibles_pair
    }

    /// Port UDP local, seul identifiant qui distingue deux liens d'un même
    /// processus. Réservé aux tests — il ne doit jamais atteindre une sortie.
    #[cfg(test)]
    fn port_local(&self) -> u16 {
        self.locale.port()
    }

    /// Ouvre le socket et déclare nos candidats ICE.
    ///
    /// Deux candidats au plus :
    /// - **hôte** : notre adresse sur le réseau local, seule utile entre deux
    ///   machines du même réseau (et pour le test en boucle locale) ;
    /// - **réfléchi par le serveur** : ce que le monde extérieur voit, obtenu
    ///   par STUN. C'est le seul qui rende possible la traversée de NAT.
    ///
    /// L'absence du second n'est pas une erreur : on tente quand même, ce qui
    /// laisse `poll` conclure à l'échec dans le délai imparti plutôt que de
    /// renoncer avant d'avoir essayé.
    fn socket_et_candidats(rtc: &mut Rtc) -> anyhow::Result<(UdpSocket, SocketAddr)> {
        // IPv4 seulement : le spike ne teste qu'une famille d'adresses.
        let socket = UdpSocket::bind("0.0.0.0:0").context("ouverture du port UDP impossible")?;
        let port = socket.local_addr().context("port UDP illisible")?.port();

        // Une seule résolution DNS pour tout le lien : elle sert à la fois à
        // choisir l'interface de sortie et à interroger les serveurs STUN.
        let serveurs = stun::serveurs_autorises();

        // `local_addr` rend 0.0.0.0, que str0m refuse comme candidat. On demande
        // au système quelle interface il emprunterait pour sortir.
        let locale = SocketAddr::new(interface_sortante(&serveurs)?, port);
        let candidat_hote =
            Candidate::host(locale, "udp").map_err(|_| anyhow!("candidat local invalide"))?;
        rtc.add_local_candidate(candidat_hote);

        // Le socket est encore bloquant : c'est ce dont la découverte STUN a besoin.
        //
        // Deux echecs sont possibles ici et tous deux etaient silencieux : la
        // decouverte peut ne rien rendre, ou la creation du candidat peut etre
        // refusee. Dans les deux cas on n'annonce que l'adresse locale, et le
        // correspondant ne peut jamais nous joindre — sans qu'aucun message ne
        // le signale. On les distingue desormais.
        match stun::adresse_publique(&socket, &serveurs) {
            Some(publique) => match Candidate::server_reflexive(publique, locale, "udp") {
                Ok(c) => {
                    rtc.add_local_candidate(c);
                    println!("  Mon adresse publique est decouverte et annoncee.");
                }
                Err(e) => {
                    println!(">>> Adresse publique decouverte mais REFUSEE comme candidat : {e}");
                    println!("    Nous n'annoncerons que notre reseau local, et le");
                    println!("    correspondant ne pourra pas nous joindre.");
                }
            },
            None => {
                println!(">>> Adresse publique INTROUVABLE : aucun serveur n'a repondu");
                println!("    dans le delai imparti. Nous n'annoncerons que notre reseau");
                println!("    local, et le correspondant ne pourra pas nous joindre.");
                println!("    (`sky-probe netcheck` peut reussir la ou ceci echoue : il");
                println!("     dispose de plus de temps.)");
            }
        }
        // `locale` reste l'adresse du candidat hôte, et pas l'adresse publique :
        // l'agent ICE n'apparie un paquet entrant qu'avec un candidat de type
        // hôte ou relayé dont l'adresse égale la destination annoncée. Un
        // candidat réfléchi ne sert qu'à l'émission — il porte l'adresse hôte
        // comme base. Annoncer l'adresse publique ici ferait jeter tous les
        // sondages entrants.

        socket
            .set_read_timeout(None)
            .context("configuration du port UDP impossible")?;
        socket
            .set_nonblocking(true)
            .context("configuration du port UDP impossible")?;

        Ok((socket, locale))
    }
}

/// Horodatage RTP ramené en millisecondes.
///
/// Le changement de base est délégué à `str0m` plutôt que refait à la main : la
/// fréquence est portée par la valeur reçue au lieu d'être supposée à 90 kHz, et
/// `rebase` calcule en `i128`, donc sans débordement sur le produit
/// intermédiaire. Une division écrite ici aurait aussi demandé de se garder d'un
/// dénominateur nul — garde inutile, `Frequency` étant un `NonZeroU32`.
fn en_millisecondes(temps: MediaTime) -> u64 {
    temps.rebase(str0m::media::Frequency::MILLIS).numer()
}

/// Crée l'instance `str0m` et rend l'origine de son temps.
///
/// L'origine est conservée par le lien : c'est à partir d'elle que
/// `PeerLink::maintenant` construit les instants présentés à `str0m`.
///
/// Les réglages de temporisation d'ICE et de DTLS restent ceux par défaut de la
/// bibliothèque. Il a été tentant d'allonger la patience d'ICE
/// (`set_max_stun_retransmits`) pour couvrir l'attente humaine ; mesuré, cela ne
/// change rien, car ce qui expire à ~30 s est la poignée de main DTLS, que
/// `str0m` 0.23.1 n'expose pas. C'est l'horloge différée qui règle le problème,
/// et elle le règle pour les deux mécanismes à la fois.
///
/// Les formats annoncés sont ceux de `formats`, dans l'ordre reçu.
fn nouveau_rtc(formats: &[FormatVideo]) -> (Rtc, Instant) {
    // `str0m` exige qu'un fournisseur cryptographique soit installé pour le
    // processus. L'appel est idempotent : le premier gagne, les suivants sont
    // ignorés sans erreur.
    str0m::crypto::from_feature_flags().install_process_default();

    let origine = Instant::now();

    // Le réglage par défaut vise la visioconférence : l'agent abandonne après
    // environ trente secondes sans réponse. Or ici l'échange des blocs passe par
    // deux humains et une messagerie — mesuré à 33 s sur un essai réel, et le
    // spectateur mourait juste avant que l'émetteur colle sa réponse.
    //
    // On allonge donc la persistance : davantage de tentatives, et un délai
    // maximal entre deux qui reste court pour que la connexion s'établisse vite
    // une fois l'autre bord présent.
    let mut config = Rtc::builder();
    config.set_max_stun_retransmits(120);
    config.set_max_stun_rto(std::time::Duration::from_secs(3));

    // Seuls les formats demandés sont annoncés (au plus HEVC 4:4:4, HEVC 4:2:0
    // et H.264), chacun dans le profil que l'encodeur produit réellement.
    //
    // `clear` retire tout le catalogue par défaut (Opus, VP8, VP9, AV1, les sept
    // variantes de H264, H266…). Deux raisons, aucune cosmétique :
    //
    // 1. **Honnêteté.** N'annoncer que ce que nous savons produire ou lire — ni
    //    repli logiciel, ni codec sans encodeur ni décodeur. Annoncer le reste
    //    inviterait le correspondant à choisir un format que nous ne saurions
    //    pas traiter.
    // 2. **Taille du bloc.** L'offre traverse une messagerie, collée à la main,
    //    et un test la borne (`l_offre_reelle_tient_sous_la_borne`). Le
    //    catalogue complet ajoute une trentaine de lignes `a=rtpmap` /
    //    `a=fmtp` / `a=rtcp-fb` à la description de la piste vidéo ; trois
    //    formats en coûtent encore, d'où une borne recalibrée.
    //
    // **Une seule entrée par famille concordable.** Deux entrées locales qui
    // concordent avec le même type de charge distant font paniquer `str0m`
    // (`assert_claim_once`, « Pt locked multiple times »). HEVC 4:4:4 et HEVC
    // Main ne concordent jamais entre eux (profil exact exigé) : deux entrées
    // H265 sont donc sûres ; H.264 n'en a qu'une.
    //
    // Les profils sont déclarés explicitement : `enable_h265` de `str0m` poserait
    // `profile-id=1` (Main), or NVENC produit du 4:4:4. La paquetisation
    // fonctionnerait avec un profil faux — c'est précisément pourquoi la garde
    // est un test sur la RÉPONSE SDP (`tests/piste_media.rs`) et non sur le
    // transport : la paquetisation RFC 7798 ne lit pas le contenu du NAL.
    let codecs = config.codec_config();
    codecs.clear();
    for format in formats {
        match format {
            FormatVideo::Hevc444 => codecs.add_h265(
                format.type_de_charge().into(),
                Some(format.retransmission().into()),
                PROFIL_HEVC_444,
                TIER_MAIN,
                NIVEAU_HEVC_6_0,
            ),
            FormatVideo::Hevc420 => codecs.add_h265(
                format.type_de_charge().into(),
                Some(format.retransmission().into()),
                PROFIL_HEVC_MAIN,
                TIER_MAIN,
                NIVEAU_HEVC_6_0,
            ),
            FormatVideo::H264 => codecs.add_h264(
                format.type_de_charge().into(),
                Some(format.retransmission().into()),
                true,
                PROFIL_NIVEAU_H264,
            ),
        }
    }

    // `enable_bwe` reste éteint, et ce n'est pas un oubli : sans lui `str0m`
    // installe un `NullPacer`, donc le contrôle de congestion reste celui du
    // projet (`crate::pacer`). L'activer mettrait deux régulateurs en série.

    (config.build(origine), origine)
}

/// Adresse de l'interface que le système emprunterait pour sortir.
///
/// Aucun paquet n'est émis : `connect` sur un socket UDP ne fait que fixer une
/// destination, ce qui suffit à faire choisir une route au noyau.
fn interface_sortante(serveurs: &[SocketAddr]) -> anyhow::Result<std::net::IpAddr> {
    let cible = serveurs
        .first()
        .ok_or_else(|| anyhow!("réseau indisponible"))?;
    let sonde = UdpSocket::bind("0.0.0.0:0").context("ouverture du port UDP impossible")?;
    sonde.connect(cible).context("réseau indisponible")?;
    Ok(sonde.local_addr().context("réseau indisponible")?.ip())
}


/// Tant qu'il vit, le port annoncé reste ouvert. Sa destruction arrête le
/// battement.
///
/// Usage asymétrique entre les deux bords. Le spectateur (`cmd_view`) le
/// relâche dès qu'il tient une réponse, avant `etablir` : la négociation ICE
/// prend alors le relais du trafic. L'hôte (`cmd_host`), lui, le garde vivant
/// PENDANT `etablir` : il vient de répondre, mais le spectateur ne relèvera
/// sa réponse qu'à sa prochaine synchronisation — sans ce battement, notre
/// box se refermerait avant même que le spectateur ait pu nous joindre. Ce
/// n'est pas un vol de paquet : le battement ne fait qu'**émettre**
/// (`stun::battement`/`stun::percer`, aucun `recv`), et `poll` filtre déjà ce
/// qu'il reçoit (serveurs STUN, octet de perçage) — voir `cmd_host::run`.
pub struct GardeMapping {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for GardeMapping {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Vrai pour une adresse de reseau local (RFC 1918 et lien-local).
///
/// Emettre vers une telle adresse depuis un autre reseau ne mene nulle part :
/// le paquet reste dans le reseau de l'emetteur.
fn adresse_privee(a: &SocketAddr) -> bool {
    match a.ip() {
        std::net::IpAddr::V4(v4) => {
            v4.is_private() || v4.is_link_local() || v4.is_loopback()
        }
        std::net::IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// Extrait les adresses des candidats annonces dans un SDP.
///
/// On ne s'appuie pas sur `str0m` pour cela : ces adresses servent a emettre
/// hors de l'agent, precisement pour ne pas demarrer son horloge.
///
/// Les adresses de reseau local sont ecartees : viser le 192.168.x.x du pair
/// depuis un autre reseau ne perce rien et peut deranger une machine tierce.
pub fn cibles_depuis_sdp(sdp: &str) -> Vec<SocketAddr> {
    let mut out = Vec::new();
    for ligne in sdp.lines() {
        let Some(reste) = ligne.strip_prefix("a=candidate:") else {
            continue;
        };
        let champs: Vec<&str> = reste.split_whitespace().collect();
        if champs.len() < 6 {
            continue;
        }
        let Ok(ip) = champs[4].parse::<std::net::IpAddr>() else {
            continue;
        };
        let Ok(port) = champs[5].parse::<u16>() else {
            continue;
        };
        let adresse = SocketAddr::new(ip, port);
        if !adresse_privee(&adresse) && !out.contains(&adresse) {
            out.push(adresse);
        }
    }
    out
}

#[cfg(test)]
mod tests_cibles {
    use super::cibles_depuis_sdp;

    const SDP: &str = "v=0
a=candidate:aaa 1 udp 2130706175 192.168.1.4 53339 typ host ufrag xyz
a=candidate:bbb 1 udp 1686109951 82.67.25.85 23022 typ srflx raddr 0.0.0.0 rport 0
";

    #[test]
    fn retient_l_adresse_internet() {
        let c = cibles_depuis_sdp(SDP);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].port(), 23022);
    }

    #[test]
    fn ecarte_le_reseau_local() {
        // Viser le 192.168.x.x du pair depuis un autre reseau ne perce rien.
        assert!(cibles_depuis_sdp(SDP).iter().all(|a| a.port() != 53339));
    }

    #[test]
    fn tolere_un_sdp_sans_candidat() {
        assert!(cibles_depuis_sdp("v=0
s=-
").is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une unité d'accès H.264 minimale en Annex-B : SPS, PPS, puis une tranche
    /// IDR (types de NAL 7, 8 et 5). Le dépaquetiseur RFC 6184 de `str0m` ne lit
    /// que l'en-tête de chaque NAL : l'unité n'a pas à être décodable.
    const UNITE_H264_IDR: &[u8] = &[
        0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x34, 0xAC, 0xD9, //
        0x00, 0x00, 0x00, 0x01, 0x68, 0xEE, 0x3C, 0x80, //
        0x00, 0x00, 0x00, 0x01, 0x65, 0x88, 0x84, 0x00, 0x10, 0xFF,
    ];

    #[test]
    fn le_spectateur_offre_et_l_hote_repond() {
        // Le sens exige par la spec d'architecture (D2, 23/08) : c'est celui qui
        // veut REGARDER qui produit l'offre. L'hote ne publie jamais d'adresse
        // avant d'avoir ouvert l'offre et su a qui il parle.
        //
        // CE QUE CE TEST NE PROUVE PAS — mesure, pas supposition : remis dans
        // l'ancien sens (l'hote offre), il passe tout autant. La mecanique du
        // canal est symetrique, donc ce test ne peut pas distinguer les deux
        // sens. Ce qu'il garde vraiment : que le canal s'ouvre des DEUX cotes,
        // alors qu'un seul le cree et que l'autre ne fait que l'apprendre par
        // `Event::ChannelOpen` — le chemin d'emission est indifferent au role.
        // Le sens, lui, est verrouille par
        // `le_bloc_en_clair_ne_porte_que_les_adresses_du_spectateur`.
        let spectateur_id = Identity::generate();
        let hote_id = Identity::generate();

        let (mut spectateur, offre) =
            PeerLink::offrant(spectateur_id, &FormatVideo::PREFERENCE).unwrap();
        let (mut hote, reponse) =
            PeerLink::repondant(hote_id, &offre, &FormatVideo::PREFERENCE).unwrap();
        spectateur.accepter_reponse(&reponse).unwrap();

        // Le canal doit s'ouvrir meme si c'est desormais l'offrant qui le cree
        // et le repondant qui le recoit par Event::ChannelOpen.
        let limite = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < limite {
            let _ = spectateur.poll();
            let _ = hote.poll();
            if spectateur.canal_ouvert() && hote.canal_ouvert() {
                return;
            }
        }
        panic!("le canal ne s'est pas ouvert dans les 10 s");
    }

    /// Adresse publique fictive injectee dans l'offre. Choisie dans TEST-NET-3
    /// (RFC 5737), reservee a la documentation : aucune machine ne la porte.
    const CANDIDAT_FICTIF: &str =
        "a=candidate:ffff 1 udp 1686109951 203.0.113.7 40404 typ srflx raddr 0.0.0.0 rport 0";

    /// Reecrit une offre en y ajoutant un candidat joignable depuis internet.
    ///
    /// Necessaire parce qu'un lien construit sur cette machine n'annonce une
    /// adresse publique que si STUN repond — une condition d'environnement, pas
    /// une propriete du code. Sans ce trucage, le test passerait au vert en
    /// n'ayant rien a trouver, ce qui ne prouverait rien.
    fn offre_avec_candidat_public(offre: &str) -> String {
        let blob = Blob::from_text(offre).unwrap();
        let sdp = crate::handshake::decomprimer(&blob.sealed_sdp).unwrap();
        let truque = format!("{}{CANDIDAT_FICTIF}\r\n", sdp);
        Blob {
            session: blob.session,
            public_key: blob.public_key,
            sealed_sdp: crate::handshake::comprimer(&truque),
        }
        .to_text()
    }

    #[test]
    fn le_repondant_connait_les_cibles_des_sa_construction() {
        // Apres l'inversion, c'est l'HOTE qui attend passivement le premier
        // paquet — et une box ne laisse entrer que ce a quoi elle a d'abord
        // emis. Le repondant doit donc tenir les adresses du pair des sa
        // construction, sans quoi son battement de maintien ne perce rien et
        // les sondages de l'offrant sont jetes a l'entree de sa box.
        let (_, offre) = PeerLink::offrant(Identity::generate(), &FormatVideo::PREFERENCE).unwrap();
        let offre = offre_avec_candidat_public(&offre);

        let (hote, _) =
            PeerLink::repondant(Identity::generate(), &offre, &FormatVideo::PREFERENCE).unwrap();

        let attendue: SocketAddr = "203.0.113.7:40404".parse().unwrap();
        assert!(
            hote.cibles_pair().contains(&attendue),
            "le repondant n'a pas retenu l'adresse publique de l'offre : sa box \
             restera fermee pendant qu'il attend. Cibles retenues : {}",
            hote.cibles_pair().len()
        );
    }

    /// Ports de TOUS les candidats d'un SDP, sans le filtre de confidentialite
    /// de `cibles_depuis_sdp` : ici on veut savoir a qui appartient le bloc, pas
    /// vers qui emettre.
    fn ports_des_candidats(sdp: &str) -> Vec<u16> {
        sdp.lines()
            .filter_map(|l| l.strip_prefix("a=candidate:"))
            .filter_map(|reste| {
                let champs: Vec<&str> = reste.split_whitespace().collect();
                champs.get(5)?.parse::<u16>().ok()
            })
            .collect()
    }

    #[test]
    fn le_bloc_en_clair_ne_porte_que_les_adresses_du_spectateur() {
        // LE test de sens, celui que `le_spectateur_offre_et_l_hote_repond` ne
        // fait pas : ce dernier passe aussi bien dans l'ancien sens, parce que
        // la mecanique du canal est symetrique. Ici non.
        //
        // Toute la raison d'etre de la decision D2 tient dans cette assertion :
        // l'offre est le SEUL bloc qui voyage en clair, et depuis l'inversion
        // ce sont les adresses de celui qui DEMANDE A REGARDER qu'elle expose.
        // Remettre l'ancien sens fait rougir ce test, parce que l'offre porte
        // alors le port de l'hote.
        let (spectateur, offre) =
            PeerLink::offrant(Identity::generate(), &FormatVideo::PREFERENCE).unwrap();
        let (hote, _reponse) =
            PeerLink::repondant(Identity::generate(), &offre, &FormatVideo::PREFERENCE).unwrap();
        assert_ne!(spectateur.port_local(), hote.port_local());

        // Aucune cle n'est necessaire : c'est bien cela, « en clair ».
        let blob = Blob::from_text(&offre).unwrap();
        let sdp = crate::handshake::decomprimer(&blob.sealed_sdp).unwrap();
        let ports = ports_des_candidats(&sdp);

        assert!(
            ports.contains(&spectateur.port_local()),
            "l'offre en clair ne porte pas les adresses du spectateur : ce n'est \
             pas lui qui offre, et le sens de la negociation a ete remis a l'envers"
        );
        assert!(
            !ports.contains(&hote.port_local()),
            "l'offre en clair porte une adresse de l'hote"
        );
    }

    #[test]
    fn l_offrant_n_a_aucune_cible_avant_la_reponse() {
        // Le pendant du test precedent : il montre que le premier ne passe pas
        // pour une raison qui vaudrait aussi bien des deux cotes. L'offrant ne
        // peut rien connaitre du pair tant qu'il n'a pas sa reponse — et il n'en
        // a pas besoin, puisque c'est lui qui prend l'initiative d'emettre.
        let (offrant, _) = PeerLink::offrant(Identity::generate(), &FormatVideo::PREFERENCE).unwrap();
        assert!(offrant.cibles_pair().is_empty());
    }

    /// Longueur maximale admise d'un bloc réel, en caractères — autant
    /// d'octets, le bloc étant de l'ASCII. Fixée par la mesure (tâche 11 du
    /// jalon C2), pas devinée.
    ///
    /// Mesuré au jalon C2 sur cinq négociations locales, quand le SDP ne
    /// décrivait qu'un canal de données : offre de 649 à 725 caractères, réponse
    /// de 677 à 753. La borne était alors de 900.
    ///
    /// **Re-mesuré à la tâche 6 du jalon 2**, la piste média vidéo ajoutée : sur
    /// six négociations locales, offre de 1 173 à 1 249 caractères, réponse de
    /// 1 293 à 1 301 (SDP de 1 880 à 2 008 octets ; `socket_et_candidats` ne
    /// déclare jamais plus de deux candidats). La description de la piste vidéo
    /// coûte donc un peu plus de 500 caractères de bloc, une fois comprimée.
    /// Le catalogue de codecs est déjà réduit à HEVC seul (voir `nouveau_rtc`) ;
    /// sans cela le coût serait plusieurs fois supérieur.
    ///
    /// **Re-mesuré le 02/10/2026, tâche 1 du sous-jalon « toutes cartes »**, les
    /// trois formats annoncés (HEVC 4:4:4, HEVC 4:2:0, H.264 : trois entrées
    /// `rtpmap`/`fmtp`/RTX) : sur six négociations locales, offre de 1 389 à
    /// 1 405 caractères, réponse de 1 441 à 1 453 (SDP d'offre de 2 526 à
    /// 2 535 octets). Les deux formats ajoutés coûtent donc environ 150 à 200
    /// caractères de bloc, une fois comprimés.
    ///
    /// 1 750 laisse environ 20 % au-dessus du plus grand bloc mesuré (1 453) et
    /// reste sous le plus petit bloc non comprimé — mesuré à 3 425 caractères
    /// sur six offres : c'est une garde de la COMPRESSION, et elle discrimine
    /// encore. La limite de la boîte aux lettres,
    /// `sky_compte::boite::TAILLE_MAX_CLAIR` (4 048 octets), est bien plus
    /// haute et tient ; elle n'est pas importée pour ne pas faire dépendre
    /// `sky-net` de `sky-compte`.
    const BORNE_BLOC_REEL: usize = 1750;

    #[test]
    fn l_offre_reelle_tient_sous_la_borne() {
        // Rougit seul si `offrant` cesse de comprimer son SDP.
        let (_, offre) = PeerLink::offrant(Identity::generate(), &FormatVideo::PREFERENCE).unwrap();
        println!("offre reelle : {} caracteres", offre.len());
        assert!(
            offre.len() < BORNE_BLOC_REEL,
            "offre de {} caracteres pour une borne de {BORNE_BLOC_REEL} : le SDP n'est-il plus comprime ?",
            offre.len()
        );
    }

    use std::time::Duration;

    /// Deux liens en boucle locale, canal ouvert des deux côtés : `(hôte,
    /// spectateur)`. Même montage que `le_spectateur_offre_et_l_hote_repond`.
    fn paire_connectee() -> (PeerLink, PeerLink) {
        let (mut spectateur, offre) =
            PeerLink::offrant(Identity::generate(), &FormatVideo::PREFERENCE).unwrap();
        let (mut hote, reponse) =
            PeerLink::repondant(Identity::generate(), &offre, &FormatVideo::PREFERENCE).unwrap();
        spectateur.accepter_reponse(&reponse).unwrap();

        let limite = Instant::now() + Duration::from_secs(10);
        while Instant::now() < limite {
            let _ = spectateur.poll();
            let _ = hote.poll();
            if spectateur.canal_ouvert() && hote.canal_ouvert() {
                return (hote, spectateur);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("le canal ne s'est pas ouvert dans les 10 s");
    }

    /// Fait tourner les deux liens jusqu'au premier message de contrôle ou à la
    /// première image reçus par `receveur`, ou jusqu'à l'expiration de `duree`,
    /// et rend ce que `receveur` a vu (hors `Idle`). Le délai est côté client :
    /// sans lui, un correspondant qui ne répond plus ferait figer la suite au
    /// lieu de la faire rougir.
    fn pomper_jusqu_a(
        receveur: &mut PeerLink,
        autre: &mut PeerLink,
        duree: Duration,
    ) -> Vec<LinkEvent> {
        let limite = Instant::now() + duree;
        let mut vus = Vec::new();
        while Instant::now() < limite {
            let _ = autre.poll();
            match receveur.poll() {
                Ok(LinkEvent::Idle) | Err(_) => {}
                Ok(evenement) => {
                    let fini = matches!(
                        evenement,
                        LinkEvent::Controle(_) | LinkEvent::Image { .. } | LinkEvent::Failed(_)
                    );
                    vus.push(evenement);
                    if fini {
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        vus
    }

    /// Comme `pomper_jusqu_a`, mais attend que `receveur` ait écarté `attendu`
    /// messages illisibles (ou qu'un `Failed` survienne, ou que `duree` expire).
    fn pomper_jusqu_a_illisibles(
        receveur: &mut PeerLink,
        autre: &mut PeerLink,
        attendu: u64,
        duree: Duration,
    ) -> Vec<LinkEvent> {
        let limite = Instant::now() + duree;
        let mut vus = Vec::new();
        while receveur.messages_illisibles() < attendu && Instant::now() < limite {
            let _ = autre.poll();
            if let Ok(e) = receveur.poll() {
                if !matches!(e, LinkEvent::Idle) {
                    let echec = matches!(e, LinkEvent::Failed(_));
                    vus.push(e);
                    if echec {
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        vus
    }

    #[test]
    fn un_message_illisible_n_interrompt_pas_le_lien() {
        let (mut hote, mut spectateur) = paire_connectee();
        hote.ecrire(b"ceci n'est pas du JSON").expect("envoi");

        // Attendre que le message soit réellement arrivé et compté : sans cela,
        // les assertions suivantes passeraient aussi bien s'il s'était perdu.
        let evenements =
            pomper_jusqu_a_illisibles(&mut spectateur, &mut hote, 1, Duration::from_secs(5));
        assert!(
            !evenements.iter().any(|e| matches!(e, LinkEvent::Failed(_))),
            "un message illisible a fait tomber le lien"
        );
        assert_eq!(
            spectateur.messages_illisibles(),
            1,
            "le message illisible n'est pas arrivé"
        );

        // Et le lien accepte encore un message valide ensuite.
        hote.envoyer_controle(&MessageControle::DemandeImageCle)
            .expect("envoi");
        let suite = pomper_jusqu_a(&mut spectateur, &mut hote, Duration::from_secs(5));
        assert!(!suite.iter().any(|e| matches!(e, LinkEvent::Failed(_))));
        assert!(suite
            .iter()
            .any(|e| matches!(e, LinkEvent::Controle(MessageControle::DemandeImageCle))));
    }

    /// Le cas visé par la conception : un JSON PARFAITEMENT VALIDE dont la
    /// variante est inconnue, tel qu'une version plus récente de l'application
    /// en enverra. `serde` le refuse (voir `controle.rs`) ; ce test prouve que
    /// le LIEN, lui, survit à ce refus et continue de livrer.
    #[test]
    fn une_variante_inconnue_en_json_valide_n_interrompt_pas_le_lien() {
        let (mut hote, mut spectateur) = paire_connectee();
        hote.ecrire(br#""RoucouleDuPigeon""#).expect("envoi");

        let evenements =
            pomper_jusqu_a_illisibles(&mut spectateur, &mut hote, 1, Duration::from_secs(5));
        assert!(
            !evenements.iter().any(|e| matches!(e, LinkEvent::Failed(_))),
            "une variante inconnue a fait tomber le lien"
        );
        assert_eq!(
            spectateur.messages_illisibles(),
            1,
            "la variante inconnue n'est pas arrivée, ou n'a pas été comptée"
        );

        // Et un message valide passe ENSUITE.
        hote.envoyer_controle(&MessageControle::PartageArrete)
            .expect("envoi");
        let suite = pomper_jusqu_a(&mut spectateur, &mut hote, Duration::from_secs(5));
        assert!(
            suite
                .iter()
                .any(|e| matches!(e, LinkEvent::Controle(MessageControle::PartageArrete))),
            "le lien n'a pas livré le message valide envoyé après la variante inconnue"
        );
        assert_eq!(spectateur.messages_illisibles(), 1);
    }

    #[test]
    fn un_message_de_controle_traverse_le_lien_dans_le_sens_spectateur_vers_hote() {
        // C'est le sens réel de la demande d'image clé : le spectateur la formule.
        let (mut hote, mut spectateur) = paire_connectee();
        spectateur
            .envoyer_controle(&MessageControle::DemandeImageCle)
            .expect("envoi");
        let vus = pomper_jusqu_a(&mut hote, &mut spectateur, Duration::from_secs(5));
        assert!(vus
            .iter()
            .any(|e| matches!(e, LinkEvent::Controle(MessageControle::DemandeImageCle))));
        assert_eq!(hote.messages_illisibles(), 0);
    }

    #[test]
    fn envoyer_avant_l_ouverture_du_canal_est_refuse() {
        let (mut offrant, _) =
            PeerLink::offrant(Identity::generate(), &FormatVideo::PREFERENCE).unwrap();
        assert_eq!(
            offrant.envoyer_controle(&MessageControle::PartageArrete),
            Err(ErreurEnvoi::CanalFerme)
        );
    }

    /// La première unité d'accès du flux HEVC 4:4:4 mesuré au jalon 0, en
    /// Annex-B tel que NVENC l'a produite : jeux de paramètres (VPS, SPS, PPS)
    /// puis la tranche IDR de la première image. 63 003 octets, une cinquantaine
    /// de paquets RTP.
    ///
    /// **Versionnée exprès, et c'est le point.** Le flux complet
    /// (`spike/cmp-hevc-444.h265`, 17,5 Mo) est exclu par `spike/.gitignore` : un
    /// test qui en dépendait ne pouvait tourner que sur la machine du
    /// propriétaire, alors que celui-ci est purement logiciel — il n'a besoin
    /// d'aucune carte NVIDIA et doit tourner partout, puisqu'il porte la
    /// démonstration que le transport ne voit pas un profil menteur.
    ///
    /// Régénérable par `head -c 63003 spike/cmp-hevc-444.h265`, la limite étant
    /// la tranche qui ouvre l'image suivante (`first_slice_segment_in_pic_flag`,
    /// ITU-T H.265 §7.3.6.1). SHA-256 :
    /// `899166c8de5e1508e7721561702eeeab62abdbdf464031bfb49ab95968ca19ef`.
    ///
    /// Chemin relatif à la racine du paquet, où Cargo place le répertoire
    /// courant d'un test.
    const UNITE_JALON_0: &str = "tests/donnees/premiere-unite-hevc-444.h265";

    /// CE QUE CE TEST NE PROUVE PAS — mesuré, pas supposé. Le dépaquetiseur
    /// RFC 7798 **normalise les codes de départ** : il reconnaît `00 00 01`
    /// comme `00 00 00 01` et ré-émet toujours la forme longue. L'unité écrite
    /// amputée de son premier octet ressort donc identique à l'originale.
    /// L'égalité ne vaut qu'au niveau des NAL, et elle ne tient octet pour octet
    /// que parce que cette unité emploie la forme longue partout : si NVENC
    /// émettait un jour des codes de trois octets, ce test rougirait pour une
    /// raison bénigne. Ce qui le fait rougir utilement, c'est une altération du
    /// CONTENU d'un NAL — vérifié en retirant le dernier octet.
    #[test]
    fn une_unite_d_acces_traverse_la_piste_intacte() {
        let (mut hote, mut spectateur) = paire_avec_piste();
        let premiere = std::fs::read(UNITE_JALON_0).expect("unité d'accès du jalon 0");
        println!("premiere unite d'acces : {} octets", premiere.len());

        // Un horodatage NON NUL, et une valeur ronde : c'est ce qui rend
        // l'assertion discriminante. Avec 0, toute conversion fautive qui rend 0
        // pour 0 passerait — dénominateur erroné, facteur 90 oublié, numérateur
        // et dénominateur inversés. N'importe quel nombre entier de
        // millisecondes est exactement représentable à 90 kHz.
        const HORODATAGE_ECRIT: u64 = 1234;

        hote.ecrire_image(&premiere, HORODATAGE_ECRIT)
            .expect("écriture");
        // Délai côté client : sans lui, une neutralisation figerait la suite au
        // lieu de la faire rougir (leçon du 16/09/2026).
        let recues = pomper_jusqu_a(&mut spectateur, &mut hote, Duration::from_secs(5));
        let (donnees, horodatage_ms, cle, sans_perte) = recues
            .iter()
            .find_map(|e| match e {
                LinkEvent::Image {
                    donnees,
                    horodatage_ms,
                    cle,
                    sans_perte,
                    ..
                } => Some((donnees, *horodatage_ms, *cle, *sans_perte)),
                _ => None,
            })
            .expect("une image doit arriver");
        assert_eq!(
            &premiere[..],
            &donnees[..],
            "l'unité d'accès doit ressortir intacte"
        );
        assert_eq!(
            horodatage_ms, HORODATAGE_ECRIT,
            "l'horodatage doit traverser sans décalage : `str0m` écrit le temps \
             RTP brut, sans base aléatoire"
        );
        assert!(
            cle,
            "la première unité d'accès est une image clé : le dépaquetiseur HEVC \
             doit le dire, sinon le drapeau ne vient pas de `CodecExtra::H265`"
        );
        assert!(
            sans_perte,
            "une seule image écrite en boucle locale : rien n'a pu manquer, donc \
             `sans_perte` doit être vrai. Faux ici signifierait que le drapeau ne \
             vient pas de `MediaData::contiguous`"
        );
    }

    #[test]
    fn ecrire_une_image_avant_la_negociation_est_refuse() {
        // Le pendant d'`envoyer_avant_l_ouverture_du_canal_est_refuse`, côté
        // piste : l'écriture doit être refusée plutôt qu'avalée en silence.
        //
        // Les deux bords sont vérifiés, et c'est ce test qui a corrigé une
        // supposition : l'offrant connaît son `Mid` dès sa construction, puisque
        // c'est lui qui crée la piste — on attendait donc de lui un refus pour
        // codec manquant. Mesuré, il refuse aussi pour piste fermée : `str0m`
        // ne délivre pas de `writer` avant que la session ait réellement
        // installé le média, ce qui n'arrive qu'à l'acceptation de la réponse.
        let (mut spectateur, offre) =
            PeerLink::offrant(Identity::generate(), &FormatVideo::PREFERENCE).unwrap();
        let (mut hote, _) =
            PeerLink::repondant(Identity::generate(), &offre, &FormatVideo::PREFERENCE).unwrap();
        assert_eq!(
            hote.ecrire_image(b"pas encore", 0),
            Err(ErreurEnvoi::PisteFermee)
        );
        assert_eq!(
            spectateur.ecrire_image(b"pas encore", 0),
            Err(ErreurEnvoi::PisteFermee)
        );
    }

    /// Amène la paire au point où l'hôte peut écrire : canal ouvert **et** piste
    /// négociée. `paire_connectee` n'attend que le canal, et la piste n'arrive
    /// qu'à `Event::MediaAdded`. Délai côté client.
    fn paire_avec_piste() -> (PeerLink, PeerLink) {
        let (mut hote, mut spectateur) = paire_connectee();
        let limite = Instant::now() + Duration::from_secs(5);
        while !hote.piste_ouverte() && Instant::now() < limite {
            let _ = hote.poll();
            let _ = spectateur.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            hote.piste_ouverte(),
            "la piste média n'a pas été négociée dans les 5 s"
        );
        (hote, spectateur)
    }

    #[test]
    fn un_refus_de_la_piste_ne_se_dit_pas_comme_un_refus_du_canal() {
        // Revue finale, M4a : les deux refus partageaient « écriture impossible
        // sur le canal », et l'hôte accusait un canal vivant. Neutralisation :
        // rendre le texte d'`EcritureRefusee` pour `ImageRefusee` — ce test
        // rougit. Ce qu'il ne prouve pas : que `ecrire_image` rend bien
        // `ImageRefusee` — aucun refus de `str0m` autre que la file pleine
        // n'est provocable ici ; le branchement se lit dans le code.
        let piste = ErreurEnvoi::ImageRefusee.to_string();
        let canal = ErreurEnvoi::EcritureRefusee.to_string();
        assert!(piste.contains("piste"), "le refus de la piste la nomme : {piste}");
        assert!(!piste.contains("canal"), "et ne désigne pas le canal : {piste}");
        assert_ne!(piste, canal);
    }

    #[test]
    fn la_file_de_paquetisation_pleine_est_un_refus_recuperable() {
        // La propriété dont la tâche 8 dépendra : ce refus-là se relance, il ne
        // s'abandonne pas. Le confondre avec `ImageRefusee` obligerait
        // l'appelant à deviner — et sous GOP infini, abandonner une image casse
        // la chaîne de références pour tout le reste du flux.
        let (mut hote, mut spectateur) = paire_avec_piste();

        // Une unité d'accès minuscule suffit : ce qu'on remplit est une file
        // d'images en attente, comptée en images et non en octets. `str0m` refuse
        // au-delà de cent.
        let unite = [0x00, 0x00, 0x00, 0x01, 0x26, 0x01, 0xAF, 0x00, 0x00, 0x01];
        let mut refus = None;
        for image in 0..200u64 {
            match hote.ecrire_image(&unite, image) {
                Ok(()) => {}
                Err(e) => {
                    refus = Some((image, e));
                    break;
                }
            }
        }
        let (images_acceptees, erreur) = refus.expect(
            "écrire 200 images sans jamais poller doit finir par être refusé : \
             sinon la file de str0m ne se remplit pas et ce test ne mesure rien",
        );
        println!("refus apres {images_acceptees} images acceptees");
        assert_eq!(
            erreur,
            ErreurEnvoi::TropDImagesEnAttente,
            "la file pleine doit se distinguer d'un refus fatal"
        );

        // Et la relance marche : c'est ce qui fait de ce refus un « réessayer »
        // et non un « abandonner ». Un seul `poll` suffit ici, et c'est tout ce
        // que ce test affirme : il libère UNE place, pas la file. Avec deux
        // images de retard il aurait fallu deux tours.
        let _ = hote.poll();
        let _ = spectateur.poll();
        hote.ecrire_image(&unite, images_acceptees)
            .expect("après un poll, la même image doit repartir");
    }

    #[test]
    fn un_tier_flag_divergent_retire_h265_de_la_reponse() {
        // Le nom dit la propriété réelle, et elle est étroite : la piste ne « se
        // ferme » pas, elle n'est JAMAIS ouverte — ce qui est mesuré, c'est que la
        // réponse de l'hôte ne décrit plus H265.
        //
        // L'intérêt est ailleurs : cela répond à la question laissée ouverte par
        // `ErreurEnvoi::CodecNonNegocie`, en mesurant ce que `str0m` fait d'un
        // profil inconciliable au lieu de le supposer.
        //
        // L'offre voyage EN CLAIR : on la décomprime, on y fausse le palier
        // annoncé pour H265 — `tier-flag=1` interdit la concordance (RFC 7798
        // §7.2.2 veut une symétrie exacte) sans casser la syntaxe —, on
        // recomprime, et on répond à cette offre.
        // L'identité du spectateur est dédoublée par ses octets : `offrant` la
        // consomme, et il faut pouvoir desceller la réponse de l'hôte.
        let secret = Identity::generate().en_octets();
        // Un seul format de chaque côté : avec H.264 et HEVC 4:2:0 en plus, la
        // ligne média survivrait sur un autre type de charge et la preuve
        // (« la ligne média disparaît ») ne tiendrait plus.
        let (_spectateur, offre) =
            PeerLink::offrant(Identity::depuis_octets(&secret), &[FormatVideo::Hevc444]).unwrap();
        let blob = Blob::from_text(&offre).unwrap();
        let sdp = crate::handshake::decomprimer(&blob.sealed_sdp).unwrap();
        let truque = sdp.replace("tier-flag=0", "tier-flag=1");
        assert_ne!(truque, sdp, "l'offre n'annonçait aucun palier à fausser");
        let offre_truquee = Blob {
            session: blob.session,
            public_key: blob.public_key,
            sealed_sdp: crate::handshake::comprimer(&truque),
        }
        .to_text();

        let (mut hote, reponse) =
            PeerLink::repondant(Identity::generate(), &offre_truquee, &[FormatVideo::Hevc444])
                .unwrap();

        // L'observation porte sur la RÉPONSE, et non sur un `ecrire_image` après
        // pompage. Une première version de ce test faisait pomper la paire :
        // impossible, et pour une raison instructive — l'offrant n'a pas envoyé
        // l'offre truquée, donc il REFUSE la réponse qui y répond
        // (`accepter_reponse` rend « elle ne correspond pas à l'offre envoyée »).
        // Aucune négociation ne peut aboutir ici, et un `PisteFermee` mesuré sans
        // négociation ne dirait rien de plus que « pas encore » : le test aurait
        // été vert pour la mauvaise raison.
        //
        // La réponse, elle, est ce que l'hôte RETIENT, et elle se lit tout de
        // suite. MESURÉ : elle ne décrit plus H265 du tout — `str0m` écarte la
        // ligne de média entière quand aucun profil ne concorde, plutôt que de la
        // garder sans codec. `Event::MediaAdded` ne peut donc jamais partir
        // (`change/sdp.rs`, `need_open_event = is_offer && !is_rejected`) et le
        // refus d'`ecrire_image` reste `PisteFermee`.
        //
        // Conclusion sur `CodecNonNegocie` : une divergence de profil ne l'atteint
        // pas — c'est désormais mesuré, plus supposé. Il faudrait un pair qui
        // conserve la ligne en n'y laissant qu'un type de charge que nous ne
        // portons pas. La variante reste sans test, et son commentaire le dit.
        let spectateur_id = Identity::depuis_octets(&secret);
        let blob_reponse = Blob::from_text(&reponse).unwrap();
        let sdp_reponse =
            crate::handshake::decomprimer(&spectateur_id.open(&blob_reponse.sealed_sdp).unwrap())
                .unwrap();
        // C'EST CETTE ASSERTION, ET ELLE SEULE, QUI PORTE LA PREUVE. Elle rougit
        // si la réponse garde H265 — vérifié en faussant `level-id`, qui se
        // négocie par un minimum au lieu d'exiger la symétrie : la ligne survit
        // alors et le test rouge.
        assert!(
            !sdp_reponse.contains("H265"),
            "la réponse décrit encore H265 : str0m aurait gardé la ligne de média \
             sans codec commun, et `CodecNonNegocie` serait alors atteignable"
        );
        // Les deux suivantes sont des constats, pas des preuves : elles passeraient
        // aussi bien sans aucune négociation, puisque l'hôte n'a de piste qu'à
        // `Event::MediaAdded`. Elles disent l'effet observable du constat
        // ci-dessus ; les garder sans celle du haut ne prouverait plus rien.
        assert!(!hote.piste_ouverte());
        assert_eq!(
            hote.ecrire_image(b"rien a negocier", 0),
            Err(ErreurEnvoi::PisteFermee)
        );
    }

    /// `paire_avec_piste`, avec des listes de formats choisies de chaque côté.
    fn paire_avec_formats(spectateur: &[FormatVideo], hote: &[FormatVideo]) -> (PeerLink, PeerLink) {
        let (mut s, offre) = PeerLink::offrant(Identity::generate(), spectateur).unwrap();
        let (mut h, reponse) = PeerLink::repondant(Identity::generate(), &offre, hote).unwrap();
        s.accepter_reponse(&reponse).unwrap();
        let limite = Instant::now() + Duration::from_secs(10);
        while Instant::now() < limite {
            let _ = s.poll();
            let _ = h.poll();
            if s.canal_ouvert() && h.canal_ouvert() && h.piste_ouverte() {
                return (h, s);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("le canal ou la piste ne se sont pas ouverts dans les 10 s");
    }

    #[test]
    fn un_spectateur_h264_seul_obtient_h264_des_deux_cotes() {
        let (mut hote, mut spectateur) =
            paire_avec_formats(&[FormatVideo::H264], &FormatVideo::PREFERENCE);
        assert_eq!(hote.format_negocie(), Some(FormatVideo::H264));
        assert_eq!(spectateur.format_negocie(), Some(FormatVideo::H264));
        // Et le paquet arrive bien étiqueté H.264, et reconnu comme image clé :
        // les deux côtés s'accordent.
        hote.ecrire_image(UNITE_H264_IDR, 7).unwrap();
        let evenements = pomper_jusqu_a(&mut spectateur, &mut hote, Duration::from_secs(5));
        let (format, cle) = evenements
            .iter()
            .find_map(|e| match e {
                LinkEvent::Image { format, cle, .. } => Some((*format, *cle)),
                _ => None,
            })
            .expect("aucune image reçue");
        assert_eq!(format, Some(FormatVideo::H264));
        assert!(cle, "une IDR H.264 doit être vue comme image clé");
    }

    #[test]
    fn deux_cotes_complets_s_accordent_sur_le_444() {
        let (hote, spectateur) =
            paire_avec_formats(&FormatVideo::PREFERENCE, &FormatVideo::PREFERENCE);
        assert_eq!(hote.format_negocie(), Some(FormatVideo::Hevc444));
        assert_eq!(spectateur.format_negocie(), Some(FormatVideo::Hevc444));
    }

    #[test]
    fn un_hote_sans_444_et_un_spectateur_complet_s_accordent_sur_le_hevc_420() {
        let (hote, spectateur) = paire_avec_formats(
            &FormatVideo::PREFERENCE,
            &[FormatVideo::Hevc420, FormatVideo::H264],
        );
        assert_eq!(hote.format_negocie(), Some(FormatVideo::Hevc420));
        assert_eq!(spectateur.format_negocie(), Some(FormatVideo::Hevc420));
    }

    #[test]
    fn des_listes_disjointes_ne_negocient_aucun_format() {
        let secret = Identity::generate().en_octets();
        let (_, offre) =
            PeerLink::offrant(Identity::depuis_octets(&secret), &[FormatVideo::Hevc444]).unwrap();
        let (mut hote, reponse) =
            PeerLink::repondant(Identity::generate(), &offre, &[FormatVideo::H264]).unwrap();

        // C'EST CETTE ASSERTION QUI PORTE LA PREUVE : la réponse de l'hôte ne
        // décrit plus aucun codec vidéo. Les deux suivantes passeraient aussi
        // bien si `repondant` ignorait sa liste — l'hôte n'a de piste qu'à
        // `Event::MediaAdded` et pas d'`writer` avant la fin de DTLS —, elles ne
        // disent que l'effet observable. Vérifié par neutralisation : avec la
        // liste ignorée, la réponse retient H265 et ce test rougit ici.
        let spectateur_id = Identity::depuis_octets(&secret);
        let blob_reponse = Blob::from_text(&reponse).unwrap();
        let sdp_reponse =
            crate::handshake::decomprimer(&spectateur_id.open(&blob_reponse.sealed_sdp).unwrap())
                .unwrap();
        assert!(
            !sdp_reponse.contains("H265") && !sdp_reponse.contains("H264"),
            "la réponse décrit encore un codec : la liste de l'hôte est ignorée"
        );

        let _ = hote.poll();
        assert_eq!(hote.format_negocie(), None);
        assert_eq!(hote.ecrire_image(b"rien", 0), Err(ErreurEnvoi::PisteFermee));
    }

    #[test]
    fn offrir_une_liste_vide_est_refuse() {
        assert!(PeerLink::offrant(Identity::generate(), &[]).is_err());
    }

    #[test]
    fn repondre_avec_une_liste_vide_est_refuse() {
        let (_, offre) = PeerLink::offrant(Identity::generate(), &FormatVideo::PREFERENCE).unwrap();
        assert!(PeerLink::repondant(Identity::generate(), &offre, &[]).is_err());
    }

    #[test]
    fn la_reponse_reelle_tient_sous_la_borne() {
        // Rougit seul si `repondant` cesse de comprimer son SDP avant de le
        // sceller. L'offre consommée est une vraie sortie d'`offrant`.
        let (_, offre) = PeerLink::offrant(Identity::generate(), &FormatVideo::PREFERENCE).unwrap();
        let (_, reponse) =
            PeerLink::repondant(Identity::generate(), &offre, &FormatVideo::PREFERENCE).unwrap();
        println!("reponse reelle : {} caracteres", reponse.len());
        assert!(
            reponse.len() < BORNE_BLOC_REEL,
            "reponse de {} caracteres pour une borne de {BORNE_BLOC_REEL} : le SDP n'est-il plus comprime ?",
            reponse.len()
        );
    }
}
