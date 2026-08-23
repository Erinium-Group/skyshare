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
    /// Sans cela, un spectateur qui attend passivement garde sa box fermee et
    /// les paquets de l'emetteur sont jetes a l'entree.
    cibles_pair: Vec<SocketAddr>,
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
                horloge,
                depart: None,
            },
            blob.to_text(),
        ))
    }

    /// Côté spectateur : consomme l'offre, produit la réponse scellée.
    pub fn viewer(identity: Identity, offre_texte: &str) -> anyhow::Result<(Self, String)> {
        let blob = Blob::from_text(offre_texte)?;
        let sdp = crate::handshake::decomprimer(&blob.sealed_sdp)?;
        // L'erreur de `str0m` cite le SDP fautif : on ne la propage pas.
        let offer =
            SdpOffer::from_sdp_string(&sdp).map_err(|_| anyhow!("bloc illisible ou incomplet"))?;

        let (mut rtc, horloge) = nouveau_rtc();
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
        // point est ce qui manquait : un spectateur qui attend sans jamais
        // emettre garde sa box fermee, et les paquets de l'emetteur sont jetes
        // a l'entree. Ces envois ne passent pas par l'agent, donc son horloge
        // ne demarre pas.
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

    /// Attend un premier paquet **du pair**, sans reveiller l'agent.
    ///
    /// C'est la cle de la patience : l'horloge de `str0m` ne demarre qu'au
    /// premier `poll`, et le compte a rebours de la poignee de main chiffree
    /// avec elle. En lisant le socket nous-memes, on peut attendre aussi
    /// longtemps qu'on veut sans qu'aucune minuterie ne court — et le battement
    /// de maintien garde le port ouvert pendant ce temps.
    ///
    /// Les paquets recus sont mis de cote et injectes au premier `poll`, pour
    /// ne rien perdre du tout premier echange.
    ///
    /// Rend `true` des qu'un paquet du pair est arrive.
    pub fn guetter_le_pair(&mut self, patience: std::time::Duration) -> bool {
        let fin = Instant::now() + patience;
        let mut buf = vec![0u8; TAILLE_DATAGRAMME];

        while Instant::now() < fin {
            match self.socket.recv_from(&mut buf) {
                Ok((n, source)) => {
                    if self.serveurs_stun.contains(&source) {
                        continue; // reponse a notre propre battement
                    }
                    if n == 1 && buf[0] == stun::OCTET_PERCAGE {
                        continue; // percage du pair : il ouvre sa box, rien de plus
                    }
                    self.paquets_recus += 1;
                    self.en_attente.push((buf[..n].to_vec(), source));
                    return true;
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
            }
        }
        false
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

    /// Côté émetteur : intègre la réponse du spectateur.
    pub fn accept_answer(&mut self, texte: &str) -> anyhow::Result<()> {
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
    /// Vrai dès qu'un datagramme **du pair** a été reçu.
    ///
    /// L'ancienne version regardait simplement si un datagramme quelconque
    /// attendait sur le port. Depuis l'ajout du battement de maintien, les
    /// serveurs STUN répondent en permanence : le spectateur prenait ces
    /// réponses pour un contact du correspondant, sautait son attente et
    /// abandonnait avant même que l'autre ait collé sa réponse.
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
            return Ok(LinkEvent::Failed("session declaree morte par l agent".into()));
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
                        str0m::RtcError::Dtls(_) => "poignee de main chiffree (DTLS)",
                        str0m::RtcError::Ice(_) => "agent ICE",
                        str0m::RtcError::Io(_) => "entree/sortie reseau",
                        str0m::RtcError::Net(_) => "lecture d un paquet",
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
                            return Ok(LinkEvent::Failed("erreur a l injection d un paquet recu".into()));
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
/// battement — à laisser mourir dès que la négociation commence, car celle-ci
/// produit son propre trafic.
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
