//! Découverte de l'adresse publique par STUN.
//!
//! `str0m` est une bibliothèque « sans-IO » : elle joue l'agent ICE mais ne
//! ramasse aucun candidat elle-même (voir son README, section « NIC enumeration
//! and TURN (and STUN) »). C'est donc à nous d'obtenir le candidat réfléchi par
//! le serveur, celui sans lequel aucune traversée de NAT n'est possible.
//!
//! Le parseur STUN de `str0m` ne convient pas ici : il exige un attribut
//! MESSAGE-INTEGRITY sur toute réponse `Binding Success`, ce qu'un serveur STUN
//! public n'envoie jamais (cet attribut appartient aux échanges ICE entre pairs,
//! pas au dialogue avec le serveur). On décode donc nous-mêmes les 20 octets
//! d'en-tête et l'attribut XOR-MAPPED-ADDRESS — c'est une trentaine de lignes.
//!
//! Confidentialité : ce module ne journalise ni n'affiche jamais d'adresse. Il
//! rend un `SocketAddr` à l'appelant, qui le confie directement à `str0m`.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use rand_core::{OsRng, RngCore};

/// Les deux seuls serveurs que le plan autorise à joindre.
const SERVEURS: [&str; 2] = ["stun.l.google.com:19302", "stun.cloudflare.com:3478"];

/// Cookie magique RFC 5389.
const COOKIE: u32 = 0x2112_A442;
/// Binding Request : classe requête, méthode Binding.
const TYPE_REQUETE: u16 = 0x0001;
/// Binding Success Response.
const TYPE_SUCCES: u16 = 0x0101;
const ATTR_XOR_MAPPED: u16 = 0x0020;

/// Budget total accordé à la découverte, tous serveurs confondus.
///
/// Le document d'architecture interdit toute attente non bornée : si les deux
/// serveurs se taisent, on repart avec le seul candidat hôte plutôt que de
/// bloquer l'utilisateur.
const BUDGET: Duration = Duration::from_millis(1200);

/// Demande à un serveur STUN quelle adresse il voit, depuis ce socket précis.
///
/// Rend `None` si aucun serveur ne répond dans le budget : la connexion reste
/// tentée avec le seul candidat hôte (utile en réseau local, inutile derrière
/// deux box différentes).
///
/// Le socket doit être bloquant à l'appel ; il est laissé tel quel.
pub fn adresse_publique(socket: &UdpSocket, serveurs: &[SocketAddr]) -> Option<SocketAddr> {
    let debut = Instant::now();

    for cible in serveurs {
        let reste = BUDGET.checked_sub(debut.elapsed())?;
        if reste.is_zero() {
            return None;
        }
        // Un serveur au plus la moitié du budget, pour laisser sa chance au second.
        let part = reste.min(BUDGET / 2);
        if let Some(a) = interroger(socket, *cible, part) {
            return Some(a);
        }
    }
    None
}

/// Envoie une requête sans attendre la réponse, pour rafraîchir le mapping NAT.
///
/// Une box referme un mapping UDP après 30 à 120 s sans trafic. Or l'échange
/// des blocs passe par un humain et une messagerie : plusieurs minutes. Sans ce
/// battement, l'adresse annoncée dans l'offre n'existe plus quand la
/// négociation démarre — on émet alors depuis un port que le correspondant ne
/// connaît pas, et ses réponses arrivent sur un port fermé.
///
/// On ne lit pas la réponse : seul le paquet sortant compte, c'est lui qui
/// rouvre le mapping. Le socket peut être non bloquant.
pub fn battement(socket: &UdpSocket, serveurs: &[SocketAddr]) {
    let trans_id = identifiant_transaction();
    let requete = requete_binding(&trans_id);
    for cible in serveurs {
        let _ = socket.send_to(&requete, cible);
    }
}

/// Octet unique d'un paquet de percage.
///
/// WebRTC demultiplexe sur le premier octet : 0-3 pour STUN, 20-63 pour DTLS,
/// 128-191 pour RTP. La valeur 0x64 ne tombe dans aucune de ces plages, donc
/// l'agent d'en face l'ignore purement et simplement. C'est exactement ce
/// qu'on veut : ouvrir le passage dans notre box sans rien demander a l'autre.
///
/// Un binding request STUN ferait l'inverse : l'agent tenterait de le traiter,
/// le rejetterait faute des bons identifiants, et la negociation en patirait.
pub const OCTET_PERCAGE: u8 = 0x64;

/// Ouvre un passage entrant dans notre box pour les adresses visees.
///
/// Chaque paquet sortant autorise le trafic entrant depuis cette destination.
/// Sans cela, une machine qui attend passivement garde sa box fermee et les
/// paquets du correspondant sont jetes a l'entree.
pub fn percer(socket: &UdpSocket, cibles: &[SocketAddr]) {
    for cible in cibles {
        let _ = socket.send_to(&[OCTET_PERCAGE], cible);
    }
}

/// Résout les serveurs autorisés, une seule fois par lien.
///
/// Seule fonction du spike à faire une résolution DNS, et seul endroit qui
/// nomme les deux serveurs que le plan autorise à joindre. Les adresses rendues
/// servent aussi à choisir l'interface de sortie, ce qui évite une deuxième
/// résolution du même nom.
///
/// Le socket étant IPv4, les réponses AAAA sont écartées. Une liste vide n'est
/// pas une erreur en soi : l'appelant décidera.
pub fn serveurs_autorises() -> Vec<SocketAddr> {
    SERVEURS
        .iter()
        .filter_map(|s| {
            s.to_socket_addrs()
                .ok()?
                .find(|a| matches!(a, SocketAddr::V4(_)))
        })
        .collect()
}

/// Ce que le réseau local laisse espérer d'une connexion directe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeNat {
    /// Les deux serveurs voient la même adresse et le même port : le port public
    /// ne dépend pas du destinataire, donc l'adresse annoncée au pair sera la
    /// bonne. Le perçage peut aboutir.
    Traversable,
    /// Le port change selon le destinataire. L'adresse découverte auprès d'un
    /// serveur ne vaut pour personne d'autre : aucun perçage n'est possible,
    /// quel que soit le logiciel. C'est le comportement des VPN commerciaux et
    /// de nombreux réseaux mobiles.
    Symetrique,
    /// Un seul serveur a répondu, ou aucun. On ne conclut pas.
    Indetermine,
}

/// Décide du type de NAT à partir des adresses vues par chaque serveur.
///
/// Séparée de l'appel réseau pour être testable sans socket.
fn verdict(vues: &[SocketAddr]) -> TypeNat {
    match vues {
        [] | [_] => TypeNat::Indetermine,
        [a, reste @ ..] => {
            if reste.iter().all(|b| b == a) {
                TypeNat::Traversable
            } else {
                TypeNat::Symetrique
            }
        }
    }
}

/// Interroge tous les serveurs autorisés et compare ce que chacun voit.
///
/// C'est le seul test qui distingue « mon réseau empêche le perçage » de « le
/// sien l'empêche » : chacun peut le lancer de son côté, sans coordination.
///
/// Ne rend jamais d'adresse — seulement le verdict.
pub fn type_de_nat(socket: &UdpSocket, serveurs: &[SocketAddr]) -> TypeNat {
    let vues: Vec<SocketAddr> = serveurs
        .iter()
        .filter_map(|cible| interroger(socket, *cible, BUDGET / 2))
        .collect();
    verdict(&vues)
}

#[cfg(test)]
mod tests_nat {
    use super::{verdict, TypeNat};
    use std::net::SocketAddr;

    fn a(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn aucune_reponse_ne_conclut_pas() {
        assert_eq!(verdict(&[]), TypeNat::Indetermine);
    }

    #[test]
    fn une_seule_reponse_ne_conclut_pas() {
        // Avec un seul point de vue, rien ne distingue un NAT symétrique d'un
        // autre : il faut deux observateurs pour comparer.
        assert_eq!(verdict(&[a("192.0.2.1:5000")]), TypeNat::Indetermine);
    }

    #[test]
    fn meme_port_vu_des_deux_cotes_est_traversable() {
        assert_eq!(
            verdict(&[a("192.0.2.1:5000"), a("192.0.2.1:5000")]),
            TypeNat::Traversable
        );
    }

    #[test]
    fn port_different_est_symetrique() {
        // Le cas des VPN commerciaux : un port par destination.
        assert_eq!(
            verdict(&[a("192.0.2.1:5000"), a("192.0.2.1:7431")]),
            TypeNat::Symetrique
        );
    }

    #[test]
    fn adresse_differente_est_symetrique() {
        // Sortie par deux passerelles distinctes : même conséquence pratique.
        assert_eq!(
            verdict(&[a("192.0.2.1:5000"), a("198.51.100.9:5000")]),
            TypeNat::Symetrique
        );
    }
}

fn interroger(socket: &UdpSocket, cible: SocketAddr, budget: Duration) -> Option<SocketAddr> {
    let trans_id = identifiant_transaction();
    let requete = requete_binding(&trans_id);
    socket.send_to(&requete, cible).ok()?;

    let echeance = Instant::now() + budget;
    let mut buf = [0u8; 512];
    loop {
        let reste = echeance.checked_duration_since(Instant::now())?;
        // Un délai nul vaut « attente infinie » côté Windows : on s'arrête avant.
        if reste.is_zero() {
            return None;
        }
        socket.set_read_timeout(Some(reste)).ok()?;

        let (n, source) = socket.recv_from(&mut buf).ok()?;
        // Un paquet venu d'ailleurs que du serveur interrogé n'a rien à faire ici.
        if source != cible {
            continue;
        }
        if let Some(a) = lire_reponse(&buf[..n], &trans_id) {
            return Some(a);
        }
    }
}

fn requete_binding(trans_id: &[u8; 12]) -> [u8; 20] {
    let mut m = [0u8; 20];
    m[0..2].copy_from_slice(&TYPE_REQUETE.to_be_bytes());
    // Longueur du corps : aucun attribut.
    m[2..4].copy_from_slice(&0u16.to_be_bytes());
    m[4..8].copy_from_slice(&COOKIE.to_be_bytes());
    m[8..20].copy_from_slice(trans_id);
    m
}

/// Douze octets imprévisibles, tirés de la même source que les clés.
///
/// Un identifiant devinable laisserait un tiers hors-chemin nous faire annoncer
/// une adresse publique qui n'est pas la nôtre.
fn identifiant_transaction() -> [u8; 12] {
    let mut id = [0u8; 12];
    OsRng.fill_bytes(&mut id);
    id
}

/// Extrait XOR-MAPPED-ADDRESS d'une réponse `Binding Success`.
fn lire_reponse(paquet: &[u8], trans_id: &[u8; 12]) -> Option<SocketAddr> {
    if paquet.len() < 20 {
        return None;
    }
    if u16::from_be_bytes([paquet[0], paquet[1]]) != TYPE_SUCCES {
        return None;
    }
    if u32::from_be_bytes(paquet[4..8].try_into().ok()?) != COOKIE {
        return None;
    }
    if &paquet[8..20] != trans_id {
        return None;
    }
    let longueur = u16::from_be_bytes([paquet[2], paquet[3]]) as usize;
    let corps = paquet.get(20..20 + longueur)?;

    let mut i = 0;
    while i + 4 <= corps.len() {
        let typ = u16::from_be_bytes([corps[i], corps[i + 1]]);
        let len = u16::from_be_bytes([corps[i + 2], corps[i + 3]]) as usize;
        let valeur = corps.get(i + 4..i + 4 + len)?;

        if typ == ATTR_XOR_MAPPED {
            return decoder_xor_mapped(valeur);
        }
        // Les attributs sont alignés sur 4 octets.
        i += 4 + len.div_ceil(4) * 4;
    }
    None
}

fn decoder_xor_mapped(valeur: &[u8]) -> Option<SocketAddr> {
    // 0x01 = IPv4. On ignore l'IPv6 : notre socket est IPv4.
    if valeur.len() < 8 || valeur[1] != 0x01 {
        return None;
    }
    let port = u16::from_be_bytes([valeur[2], valeur[3]]) ^ (COOKIE >> 16) as u16;
    let brut = u32::from_be_bytes(valeur[4..8].try_into().ok()?) ^ COOKIE;
    Some(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::from(brut),
        port,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Adresse de documentation RFC 5737 : ne désigne aucune machine réelle.
    const EXEMPLE: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 42);

    fn reponse_succes(trans_id: &[u8; 12], addr: SocketAddrV4) -> Vec<u8> {
        let mut attr = vec![0u8, 0x01];
        attr.extend_from_slice(&(addr.port() ^ (COOKIE >> 16) as u16).to_be_bytes());
        attr.extend_from_slice(&(u32::from(*addr.ip()) ^ COOKIE).to_be_bytes());

        let mut p = Vec::new();
        p.extend_from_slice(&TYPE_SUCCES.to_be_bytes());
        p.extend_from_slice(&((4 + attr.len()) as u16).to_be_bytes());
        p.extend_from_slice(&COOKIE.to_be_bytes());
        p.extend_from_slice(trans_id);
        p.extend_from_slice(&ATTR_XOR_MAPPED.to_be_bytes());
        p.extend_from_slice(&(attr.len() as u16).to_be_bytes());
        p.extend_from_slice(&attr);
        p
    }

    #[test]
    fn decode_une_reponse_conforme() {
        let id = [3u8; 12];
        let attendu = SocketAddrV4::new(EXEMPLE, 51234);
        let p = reponse_succes(&id, attendu);
        assert_eq!(lire_reponse(&p, &id), Some(SocketAddr::V4(attendu)));
    }

    #[test]
    fn rejette_une_transaction_etrangere() {
        // Sans cette vérification, un paquet injecté par un tiers pourrait nous
        // faire annoncer une adresse qui n'est pas la nôtre.
        let p = reponse_succes(&[3u8; 12], SocketAddrV4::new(EXEMPLE, 51234));
        assert_eq!(lire_reponse(&p, &[9u8; 12]), None);
    }

    #[test]
    fn rejette_un_paquet_tronque() {
        let id = [3u8; 12];
        let p = reponse_succes(&id, SocketAddrV4::new(EXEMPLE, 51234));
        assert_eq!(lire_reponse(&p[..p.len() - 3], &id), None);
    }

    /// Second garde-fou : une réponse venue d'ailleurs que du serveur
    /// interrogé est ignorée, même si elle est parfaitement formée et porte
    /// le bon identifiant de transaction.
    ///
    /// L'identifiant seul ne suffit pas : un tiers **sur le chemin** le voit
    /// passer en clair et pourrait renvoyer une réponse conforme avant le
    /// serveur. Le contrôle d'origine est ce qui l'écarte — et il vit dans
    /// `interroger`, pas dans `lire_reponse`, donc il demande de vrais
    /// sockets pour être exercé.
    #[test]
    fn ignore_une_reponse_venue_dun_autre_expediteur() {
        let client = UdpSocket::bind("127.0.0.1:0").expect("socket client");
        let serveur = UdpSocket::bind("127.0.0.1:0").expect("socket serveur");
        let imposteur = UdpSocket::bind("127.0.0.1:0").expect("socket imposteur");

        let adresse_serveur = serveur.local_addr().expect("adresse serveur");
        let adresse_client = client.local_addr().expect("adresse client");

        // Adresse que l'imposteur essaie de nous faire annoncer, distincte de
        // celle du vrai serveur : si le contrôle d'origine sautait, c'est
        // elle que `interroger` rendrait.
        const USURPEE: Ipv4Addr = Ipv4Addr::new(198, 51, 100, 7);

        let complice = std::thread::spawn(move || {
            let mut buf = [0u8; 512];
            serveur
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("délai serveur");
            let (n, _) = serveur.recv_from(&mut buf).expect("requête reçue");
            assert!(n >= 20, "requête STUN trop courte");
            let mut trans_id = [0u8; 12];
            trans_id.copy_from_slice(&buf[8..20]);

            // L'imposteur répond le premier, avec le bon identifiant.
            let usurpee = reponse_succes(&trans_id, SocketAddrV4::new(USURPEE, 1));
            imposteur
                .send_to(&usurpee, adresse_client)
                .expect("envoi imposteur");

            // Puis le vrai serveur, un instant plus tard pour garantir l'ordre.
            std::thread::sleep(Duration::from_millis(50));
            let vraie = reponse_succes(&trans_id, SocketAddrV4::new(EXEMPLE, 51234));
            serveur
                .send_to(&vraie, adresse_client)
                .expect("envoi serveur");
        });

        let vu = interroger(&client, adresse_serveur, Duration::from_secs(5));
        complice.join().expect("thread complice");

        assert_eq!(
            vu,
            Some(SocketAddr::V4(SocketAddrV4::new(EXEMPLE, 51234))),
            "la réponse de l'imposteur a été retenue à la place de celle du serveur"
        );
    }
}
