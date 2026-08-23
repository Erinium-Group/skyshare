use std::net::UdpSocket;

use sky_net::stun::{serveurs_autorises, type_de_nat, TypeNat};

/// Diagnostic réseau utilisable seul, sans correspondant.
///
/// Interroge deux serveurs STUN publics et compare ce que chacun voit. Si les
/// deux annoncent la même adresse, le port public ne dépend pas du destinataire
/// et le perçage peut aboutir. S'ils diffèrent, aucun logiciel ne percera.
///
/// Chacun lance cette commande de son côté : la comparaison des deux verdicts
/// désigne le réseau fautif sans avoir à se coordonner ni à se croire sur parole.
pub fn run() -> anyhow::Result<()> {
    println!("Test du réseau.");
    println!("Rien n'est envoyé à personne : on demande seulement à deux serveurs");
    println!("publics quelle adresse ils voient. Aucune adresse ne sera affichée.\n");

    let serveurs = serveurs_autorises();
    if serveurs.len() < 2 {
        println!("⚠️  VERDICT IMPOSSIBLE");
        println!();
        println!("Les serveurs de test ne sont pas joignables — DNS bloqué, pare-feu,");
        println!("ou absence de connexion. Rien ne peut être conclu.");
        return Ok(());
    }

    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|_| {
        anyhow::anyhow!("impossible d'ouvrir un port UDP — un pare-feu bloque peut-être")
    })?;

    match type_de_nat(&socket, &serveurs) {
        TypeNat::Traversable => {
            println!("✅  TON RÉSEAU EST COMPATIBLE");
            println!();
            println!("Les deux serveurs voient la même adresse : ton port public ne change");
            println!("pas selon le destinataire. La connexion directe peut aboutir de ton");
            println!("côté.");
            println!();
            println!("Si la connexion échoue malgré tout, le blocage vient de l'autre");
            println!("machine. Demande-lui de lancer cette même commande.");
        }
        TypeNat::Symetrique => {
            println!("🔴  TON RÉSEAU EMPÊCHE LA CONNEXION DIRECTE");
            println!();
            println!("Les deux serveurs voient des adresses différentes : ton réseau");
            println!("attribue un port différent à chaque destinataire. L'adresse que tu");
            println!("découvres ne vaut donc pour personne d'autre, et aucun logiciel ne");
            println!("peut percer ça — ni SkyShare, ni Discord, ni Zoom.");
            println!();
            println!("Causes, par fréquence :");
            println!("  1. Un VPN actif. L'exclure suffit — voir docs/vpn-split-tunneling.md,");
            println!("     sans oublier l'interrupteur de la fonction, pas seulement la liste.");
            println!("  2. Une connexion mobile (4G/5G) ou un partage de connexion.");
            println!("  3. Un réseau d'entreprise ou d'école, ou un fournisseur en CGNAT.");
            println!();
            println!("Relance cette commande après chaque changement : le verdict te dira");
            println!("tout de suite si c'est réglé, sans avoir besoin de l'autre personne.");
        }
        TypeNat::Indetermine => {
            println!("⚠️  VERDICT IMPOSSIBLE");
            println!();
            println!("Un seul des deux serveurs a répondu, ou aucun. Il en faut deux pour");
            println!("comparer : avec un seul point de vue, rien ne distingue un réseau");
            println!("compatible d'un réseau qui ne l'est pas.");
            println!();
            println!("Un pare-feu bloque probablement le trafic UDP sortant. Réessaie, et");
            println!("si le verdict reste impossible, c'est déjà une information : un");
            println!("réseau qui bloque UDP bloquera aussi la connexion directe.");
        }
    }
    Ok(())
}
