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
}
