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
//! 2. **La crate `str0m` est compilée avec sa fonctionnalité `pii`**, qui
//!    remplace les adresses par `{REDACTED}` dans ses propres traces. Le spike
//!    n'installe de toute façon aucun collecteur `tracing`, donc rien n'est émis
//!    par défaut ; la fonctionnalité protège le jour où quelqu'un en branche un.

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
    /// Vrai dès qu'un datagramme exploitable est arrivé du correspondant.
    contact: bool,
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
        let mut rtc = nouveau_rtc();
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
                contact: false,
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

        let mut rtc = nouveau_rtc();
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
                contact: false,
            },
            reponse.to_text(),
        ))
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
                    // pas fatal : ICE réémettra. L'erreur nommerait la destination.
                    let _ = self.socket.send_to(&t.contents, t.destination);
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
                buf.truncate(n);
                // Un datagramme illisible (parasite, scan de port) est ignoré :
                // il ne doit ni interrompre la négociation ni être décrit.
                if let Ok(contents) = buf.as_slice().try_into() {
                    let recu = Receive {
                        proto: Protocol::Udp,
                        source,
                        destination: self.locale,
                        contents,
                    };
                    if self.rtc.handle_input(Input::Receive(now(), recu)).is_err() {
                        return Ok(LinkEvent::Failed("connexion interrompue".into()));
                    }
                    self.contact = true;
                }
            }
            Err(ref e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            // Sous Windows, un ICMP « port unreachable » remonte en erreur sur le
            // socket UDP. Ce n'est pas fatal pendant les sondages ICE.
            Err(_) => {}
        }

        if self.rtc.handle_input(Input::Timeout(now())).is_err() {
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

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Vrai dès que le correspondant nous a envoyé quelque chose d'exploitable.
    ///
    /// Le spectateur s'en sert pour savoir quand démarrer son compte à rebours :
    /// tant que l'hôte n'a pas collé la réponse, il n'y a rien à attendre, et
    /// faire courir le délai de 8 s pendant le copier-coller le condamnerait.
    pub fn contact_recu(&self) -> bool {
        self.contact
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

fn now() -> Instant {
    Instant::now()
}

fn nouveau_rtc() -> Rtc {
    // `str0m` exige qu'un fournisseur cryptographique soit installé pour le
    // processus. L'appel est idempotent : le premier gagne, les suivants sont
    // ignorés sans erreur.
    str0m::crypto::from_feature_flags().install_process_default();
    Rtc::new(now())
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
