//! Négociation complète entre deux liens, dans un seul processus.
//!
//! Ne teste pas la traversée de NAT — les deux bords sont sur la même machine.
//! Il teste tout le reste : format du bloc, compression, scellement, échange
//! ICE, ouverture du canal. Un échec ici désigne un défaut du code, sans
//! mobiliser deux personnes ni dépendre d'un réseau.

use std::time::{Duration, Instant};

use sky_crypto::Identity;
use sky_net::{LinkEvent, PeerLink};

const DELAI: Duration = Duration::from_secs(10);

pub fn run() -> anyhow::Result<()> {
    println!("Négociation locale entre deux liens du même processus.");
    println!("La traversée de NAT n'est pas testée ici ; tout le reste l'est.\n");

    let (mut hote, offre) = PeerLink::host(Identity::generate())?;
    println!("Offre produite      : {} caractères", offre.len());

    let (mut spectateur, reponse) = PeerLink::viewer(Identity::generate(), &offre)?;
    println!("Réponse produite    : {} caractères", reponse.len());

    hote.accept_answer(&reponse)?;
    println!("Réponse acceptée par l'hôte.\n");

    println!("Négociation...");
    let debut = Instant::now();
    let (mut hote_ok, mut spec_ok) = (false, false);

    while debut.elapsed() < DELAI && !(hote_ok && spec_ok) {
        // Les deux bords avancent en alternance : chacun doit voir les paquets
        // de l'autre pour que la négociation progresse.
        match hote.poll()? {
            LinkEvent::Connected => {
                hote_ok = true;
                println!("  hôte       : CONNECTÉ à {:.2} s", debut.elapsed().as_secs_f32());
            }
            LinkEvent::Failed(e) => {
                println!("  hôte       : ÉCHEC — {e}");
                break;
            }
            _ => {}
        }
        match spectateur.poll()? {
            LinkEvent::Connected => {
                spec_ok = true;
                println!(
                    "  spectateur : CONNECTÉ à {:.2} s",
                    debut.elapsed().as_secs_f32()
                );
            }
            LinkEvent::Failed(e) => {
                println!("  spectateur : ÉCHEC — {e}");
                break;
            }
            _ => {}
        }
    }

    let (he, hr, herr) = hote.trafic();
    let (se, sr, serr) = spectateur.trafic();
    println!();
    println!("               émis  reçus  erreurs");
    println!("  hôte       : {he:>5}  {hr:>5}  {herr:>7}");
    println!("  spectateur : {se:>5}  {sr:>5}  {serr:>7}");
    println!();

    if hote_ok && spec_ok {
        println!("VERDICT : le code fonctionne de bout en bout.");
        println!("Un échec entre deux machines vient donc du réseau, pas d'ici.");
    } else if he > 0 && hr == 0 && se > 0 && sr == 0 {
        println!("VERDICT : les deux bords émettent sans jamais rien recevoir,");
        println!("alors qu'ils partagent la même machine. Les paquets n'atteignent");
        println!("pas leur destination : le défaut est dans notre code.");
    } else {
        println!("VERDICT : négociation incomplète en {} s.", DELAI.as_secs());
        println!("Les compteurs ci-dessus disent qui a émis et qui a reçu.");
    }
    Ok(())
}
