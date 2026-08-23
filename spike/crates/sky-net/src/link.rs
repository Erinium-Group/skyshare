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

use std::net::{SocketAddr, UdpSocket};
use std::time::Instant;

use anyhow::{anyhow, Context};
use str0m::change::{SdpAnswer, SdpOffer, SdpPendingOffer};
use str0m::channel::ChannelId;
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc};

use sky_crypto::Identity;

use crate::handshake::Blob;
use crate::stun;

/// Étiquette du canal de données. Une seule pour tout le spike.
const CANAL: &str = "sky";

/// Taille maximale d'un datagramme UDP accepté (limite interne de `str0m`).
const TAILLE_DATAGRAMME: usize = 2000;

/// Ce que la boucle d'appel apprend à chaque tour de `poll`.
pub enum LinkEvent {
    /// ICE a trouvé un chemin et DTLS est établi.
    Connected,
    /// Des octets sont arrivés sur le canal de données.
    Data(Vec<u8>),
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
    /// L'offre en attente de réponse, côté hôte uniquement.
    pending: Option<SdpPendingOffer>,
    canal: Option<ChannelId>,
    peer_key: Option<[u8; 32]>,
    connected: bool,
    /// Erreurs d'émission et de réception avalées sur le port UDP.
    ///
    /// Elles ne sont pas fatales — ICE réémet — mais les taire entièrement
    /// ferait attribuer au NAT un échec qui vient peut-être d'ailleurs. On les
    /// compte pour que le diagnostic final puisse nuancer sa conclusion.
    erreurs_socket: u64,
    /// Datagrammes réellement émis et reçus sur le port UDP.
    ///
    /// Sans ces deux nombres, un échec de négociation est indiscernable : on ne
    /// sait pas si l'on émet dans le vide, si l'on reçoit sans pouvoir répondre,
    /// ou si rien ne circule du tout. Chacun de ces cas a une cause différente.
    paquets_emis: u64,
    paquets_recus: u64,
    /// Origine du temps tel que `str0m` le voit.
    horloge: Instant,
    /// Instant réel du premier `poll`. Voir `maintenant`.
    depart: Option<Instant>,
}

impl PeerLink {
    /// Côté émetteur : produit l'offre à envoyer au spectateur.
    ///
    /// Le `Pacer` n'intervient pas ici : le pilotage du débit appartient à la
    /// commande, pas au lien.
    ///
    /// Note de confidentialité : dans le spike, l'offre voyage **en clair**
    /// (base64 lisible par quiconque reçoit le bloc), car le destinataire n'est
    /// pas encore connu et il n'existe donc aucune clé pour la sceller. Seule la
    /// réponse est scellée. Au jalon 1, la clé du destinataire viendra de la
    /// boîte aux lettres et l'offre sera scellée elle aussi.
    pub fn host(identity: Identity) -> anyhow::Result<(Self, String)> {
        let (mut rtc, horloge) = nouveau_rtc();
        let (socket, locale) = Self::socket_et_candidats(&mut rtc)?;

        let mut change = rtc.sdp_api();
        change.add_channel(CANAL.to_string());
        let (offer, pending) = change
            .apply()
            .ok_or_else(|| anyhow!("aucun changement à négocier"))?;

        let blob = Blob {
            public_key: identity.public_key(),
            sealed_sdp: offer.to_sdp_string().into_bytes(),
        };

        Ok((
            Self {
                rtc,
                socket,
                locale,
                identity,
                pending: Some(pending),
                canal: None,
                peer_key: None,
                connected: false,
                erreurs_socket: 0,
                paquets_emis: 0,
                paquets_recus: 0,
                horloge,
                depart: None,
            },
            blob.to_text(),
        ))
    }

    /// Côté spectateur : consomme l'offre, produit la réponse scellée.
    pub fn viewer(identity: Identity, offre_texte: &str) -> anyhow::Result<(Self, String)> {
        let blob = Blob::from_text(offre_texte)?;
        let sdp = String::from_utf8(blob.sealed_sdp).context("bloc illisible — texte invalide")?;
        // L'erreur de `str0m` cite le SDP fautif : on ne la propage pas.
        let offer =
            SdpOffer::from_sdp_string(&sdp).map_err(|_| anyhow!("bloc illisible ou incomplet"))?;

        let (mut rtc, horloge) = nouveau_rtc();
        let (socket, locale) = Self::socket_et_candidats(&mut rtc)?;

        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|_| anyhow!("bloc refusé — il ne décrit pas une session utilisable"))?;

        // La réponse porte nos adresses : elle est scellée avec la clé de l'hôte,
        // donc seul lui pourra la lire, même si le bloc transite par ailleurs.
        let sealed = identity.seal(&blob.public_key, answer.to_sdp_string().as_bytes());
        let reponse = Blob {
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
                peer_key: Some(blob.public_key),
                connected: false,
                erreurs_socket: 0,
                paquets_emis: 0,
                paquets_recus: 0,
                horloge,
                depart: None,
            },
            reponse.to_text(),
        ))
    }

    /// Compteurs de circulation sur le port UDP : émis, reçus, erreurs.
    ///
    /// Destinés au message d'échec : ils disent lequel des trois scénarios
    /// s'est produit, là où « NAT strict » n'était qu'une conjecture.
    pub fn trafic(&self) -> (u64, u64, u64) {
        (self.paquets_emis, self.paquets_recus, self.erreurs_socket)
    }

    /// Côté émetteur : intègre la réponse du spectateur.
    pub fn accept_answer(&mut self, texte: &str) -> anyhow::Result<()> {
        let blob = Blob::from_text(texte)?;
        let sdp = self.identity.open(&blob.sealed_sdp)?;
        let sdp = String::from_utf8(sdp).context("réponse illisible — texte invalide")?;
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
    /// - le spectateur, qui doit scruter pendant cette attente, verrait sa
    ///   poignée de main DTLS expirer avant que l'émetteur n'ait commencé
    ///   (mesuré : abandon à 30 s, quoi qu'on règle côté ICE) ;
    /// - l'émetteur, resté bloqué sur son invite de saisie, présenterait d'un
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
    /// C'est ce que le spectateur appelle pendant qu'il attend son
    /// correspondant : le paquet est seulement observé, pas consommé — le
    /// `poll` suivant le lira normalement — et `str0m` n'est pas touché, donc
    /// aucune de ses minuteries ne court.
    pub fn contact_en_attente(&self) -> bool {
        let mut buf = [0u8; TAILLE_DATAGRAMME];
        self.socket.peek_from(&mut buf).is_ok()
    }

    /// Un tour de la boucle : vider les sorties de `str0m`, écouter le socket,
    /// avancer le temps. Ne bloque jamais.
    pub fn poll(&mut self) -> anyhow::Result<LinkEvent> {
        if !self.rtc.is_alive() {
            return Ok(LinkEvent::Failed("connexion interrompue".into()));
        }

        // 1. Vider les sorties de str0m.
        loop {
            let sortie = match self.rtc.poll_output() {
                Ok(s) => s,
                // Le message d'origine peut contenir du SDP, donc des adresses.
                Err(_) => return Ok(LinkEvent::Failed("connexion interrompue".into())),
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
                    Event::ChannelData(d) => return Ok(LinkEvent::Data(d.data)),
                    Event::ChannelClose(_) => self.canal = None,
                    _ => {}
                },
            }
        }

        // 2. Injecter ce qui arrive du socket.
        let mut buf = vec![0u8; TAILLE_DATAGRAMME];
        match self.socket.recv_from(&mut buf) {
            Ok((n, source)) => {
                self.paquets_recus += 1;
                buf.truncate(n);
                // Un datagramme illisible (parasite, scan de port) est ignoré :
                // il ne doit ni interrompre la négociation ni être décrit.
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
                        return Ok(LinkEvent::Failed("connexion interrompue".into()));
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

    /// Écrit sur le canal de données.
    ///
    /// Échoue tant que le canal n'est pas ouvert, et quand le tampon d'émission
    /// est plein — l'appelant décide alors de réessayer ou de laisser tomber la
    /// donnée.
    pub fn send(&mut self, data: &[u8]) -> anyhow::Result<()> {
        let id = self
            .canal
            .ok_or_else(|| anyhow!("canal de données pas encore ouvert"))?;
        let mut canal = self
            .rtc
            .channel(id)
            .ok_or_else(|| anyhow!("canal de données fermé"))?;
        let accepte = canal
            .write(true, data)
            .map_err(|_| anyhow!("écriture impossible sur le canal"))?;
        if !accepte {
            return Err(anyhow!("tampon d'émission plein"));
        }
        Ok(())
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
        if let Some(publique) = stun::adresse_publique(&socket, &serveurs) {
            if let Ok(c) = Candidate::server_reflexive(publique, locale, "udp") {
                rtc.add_local_candidate(c);
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
fn nouveau_rtc() -> (Rtc, Instant) {
    // `str0m` exige qu'un fournisseur cryptographique soit installé pour le
    // processus. L'appel est idempotent : le premier gagne, les suivants sont
    // ignorés sans erreur.
    str0m::crypto::from_feature_flags().install_process_default();

    let origine = Instant::now();
    (Rtc::new(origine), origine)
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
