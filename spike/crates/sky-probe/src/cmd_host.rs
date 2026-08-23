//! Côté émetteur : produit l'offre, intègre la réponse, puis capture,
//! encode et envoie un flux vidéo réel — la chaîne complète de la Tâche 8.
//!
//! `WgcCapture::next_frame()` → `NvencEncoder::encode()` → `link.send()`,
//! avec le débit réseau réellement piloté par `Pacer::target_bps()`. C'est
//! ici, et seulement ici, que le `Pacer` entre en jeu : la Tâche 7 avait
//! interdiction de le câbler, le pilotage du débit appartient à la commande,
//! pas à `PeerLink`.

use std::io::Write;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sky_capture::wgc::WgcCapture;
use sky_crypto::Identity;
use sky_encode::{nvenc::NvencEncoder, Codec};
use sky_net::{LinkEvent, Pacer, PeerLink};

use crate::cmd_encode::{Source, TextureSynthetique, FPS};

/// Délai maximal d'établissement, imposé par le document d'architecture (§5.5).
/// Jamais d'attente indéfinie, jamais de roue qui tourne sans fin.
pub const DELAI_ETABLISSEMENT: Duration = Duration::from_secs(25);

/// En-tête préfixé à chaque MORCEAU de message envoyé : 8 octets
/// d'horodatage (microsecondes depuis l'époque Unix, horloge **système** —
/// pas `Instant`, propre à un seul processus) puis 1 octet drapeau (non nul
/// = premier morceau d'une image encodée).
///
/// Les deux bords tournent sur la même machine pendant ce spike, donc
/// l'horloge système est directement comparable entre eux — c'est ce qui
/// permet au spectateur de mesurer un temps de transit, et à nous de calculer
/// un RTT quand il nous renvoie l'horodatage tel quel dans son retour.
///
/// Ce préfixe ne quitte jamais le processus spectateur : `cmd_view` l'ôte
/// avant d'écrire quoi que ce soit dans `recu.h265`, qui reste un flux HEVC
/// brut, structurellement valide pour `ffprobe` — le découpage en morceaux
/// est un détail de transport, invisible dans le fichier écrit.
pub(crate) const EN_TETE_MORCEAU: usize = 9;

/// Taille maximale de la charge utile d'un morceau — reprend la taille de
/// message (1200 octets) avec laquelle la Tâche 7 a mesuré un débit soutenu
/// de 182 à 246 Mbps en boucle locale. Découverte de ce banc de test :
/// envoyer un paquet NVENC entier en un seul message (jusqu'à ~40 Ko à
/// 20 Mbps/60 i/s, plafonné par le tampon VBV d'une image) sature le tampon
/// d'émission de `str0m` de façon soutenue et fait échouer des envois — alors
/// que des messages de cette taille-ci s'écoulent sans accroc. Chaque paquet
/// NVENC est donc redécoupé ici ; le spectateur ne fait que recoller les
/// morceaux dans l'ordre, le flux Annex B écrit sur disque est identique.
pub(crate) const TAILLE_MORCEAU_PAYLOAD: usize = 1200 - EN_TETE_MORCEAU;

/// Granularité à laquelle le lien est servi pendant les attentes de la
/// boucle principale (cadence FPS, capture écran, relance d'un envoi en
/// échec). Découverte de banc de test : un seul sommeil de ~16 ms entre deux
/// trames (première version de cette boucle) laisse `str0m` sans service
/// pendant tout ce temps. `str0m` est sans-IO : ses accusés de réception ne
/// sont traités que lorsqu'on l'interroge, et son contrôle de flux SCTP a
/// besoin d'un service bien plus fréquent que 60 Hz pour garder sa fenêtre
/// ouverte. Mesuré avant correction : RTT médian ~1,1 s et ~27 % d'échecs
/// d'envoi à 20 Mbps en boucle locale — un artefact du banc de test, pas du
/// réseau. Avec un service toutes les millisecondes, ces deux chiffres
/// retombent à des valeurs de boucle locale plausibles (voir le rapport).
const GRANULARITE_SERVICE_RESEAU: Duration = Duration::from_millis(1);

/// Budget de relance pour un morceau déjà encodé qui échoue à partir — voir
/// `envoyer_ou_abandonner`. Généreux par rapport aux quelques millisecondes
/// de résorption observées en boucle locale une fois les morceaux réduits à
/// `TAILLE_MORCEAU_PAYLOAD` : ce budget n'est atteint qu'en cas de congestion
/// soutenue et anormale.
const BUDGET_RETRY_ENVOI: Duration = Duration::from_millis(300);

pub(crate) fn epoch_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

/// Paramètres de la chaîne complète. Regroupés dans une struct plutôt qu'une
/// longue liste d'arguments positionnels — `run` n'est appelée que depuis
/// `main.rs`, mais sept `u32`/`usize` à la file seraient une source d'erreur
/// silencieuse à l'appel.
pub struct Parametres {
    pub secondes: u64,
    pub codec: Codec,
    pub bitrate_mbps: u32,
    pub floor_mbps: u32,
    pub monitor: usize,
    pub source: Source,
    pub largeur_synth: u32,
    pub hauteur_synth: u32,
}

pub fn run(p: Parametres) -> anyhow::Result<()> {
    // Le régulateur est construit AVANT l'offre, et pas au moment où il servira :
    // il ne dépend que des deux arguments de ligne de commande, et sa validation
    // de bornes (plancher ≤ plafond) doit échouer tout de suite. Construit plus
    // bas, un `--floor-mbps 50 --bitrate-mbps 30` n'aurait été refusé qu'une fois
    // la connexion établie — donc après tout l'aller-retour humain de
    // copier-coller des blocs, pour une faute de frappe visible sans réseau.
    //
    // Le Pacer part au plancher (comportement documenté de `Pacer::new`) : la
    // cadence effective démarre donc réduite et remonte vers le plafond en
    // quelques secondes — visible dans l'affichage périodique plus bas.
    let mut pacer = Pacer::new(p.floor_mbps * 1_000_000, p.bitrate_mbps * 1_000_000)?;

    let (mut link, offre) = PeerLink::host(Identity::generate())?;

    println!("\n=== ÉTAPE 1 : envoie ce bloc à ton correspondant ===\n");
    // L'exposé, ici, c'est l'opérateur — pas le correspondant, dont la réponse
    // voyage scellée. C'est donc dans SA console que l'avertissement a sa place,
    // et pas dans le mode d'emploi de l'ami.
    println!("  /!\\  Ce bloc contient l'adresse publique de cette machine, en clair");
    println!("       pour qui sait le décoder. Envoie-le en message privé, à une");
    println!("       personne précise — jamais dans un salon ouvert ni sur un forum.\n");
    println!("{offre}\n");
    println!("=== ÉTAPE 2 : colle sa réponse ici puis Entrée ===\n");
    std::io::stdout().flush().ok();

    // Le mapping NAT du port qu'on vient d'annoncer expire en 30 a 120 s sans
    // trafic, alors que l'echange des blocs par messagerie prend couramment
    // plusieurs minutes. Sans ce battement, on negocie depuis un port que le
    // correspondant ne connait pas, et ses reponses arrivent sur un port ferme.
    println!("  Prends le temps qu'il te faut : son programme t'attend jusqu'a");
    println!("  dix minutes sans rien consommer. Colle sa reponse quand tu l'as.");
    println!();

    let garde = link.maintenir_mapping()?;

    let mut reponse = String::new();
    let debut_attente = Instant::now();
    std::io::stdin().read_line(&mut reponse)?;
    let attente = debut_attente.elapsed();
    println!("
(echange des blocs : {} s, port maintenu ouvert)", attente.as_secs());

    // La negociation produit son propre trafic : le battement n'a plus lieu d'etre.
    drop(garde);

    link.accept_answer(&reponse)?;

    println!("\nNégociation en cours...");
    std::io::stdout().flush().ok();

    let Some(duree) = etablir(&mut link)? else {
        return Ok(());
    };
    println!("CONNECTÉ en {:.1} s", duree.as_secs_f32());

    // --- Capture : écran réel, ou texture synthétique déterministe (Q4). ---
    // La texture synthétique est celle de la Tâche 3/cmd_encode : construite
    // sur le même device D3D11 que la capture, NVENC n'y voit aucune
    // différence avec une vraie image. Sur un écran figé, WGC ne livre presque
    // aucune image et la mesure de charge serait flatteuse sans être fausse —
    // d'où le choix de mesurer Q4 sur cette source, documenté dans le rapport.
    let mut cap = WgcCapture::new(p.monitor, None)?;
    let (largeur, hauteur, mut synth) = match p.source {
        Source::Ecran => {
            let attente_max = Instant::now() + Duration::from_secs(5);
            let premiere = loop {
                if let Some(f) = cap.next_frame(Duration::from_millis(200))? {
                    break f;
                }
                if Instant::now() >= attente_max {
                    anyhow::bail!("aucune image capturée en 5 s — l'écran est-il figé ?");
                }
            };
            (premiere.width, premiere.height, None)
        }
        Source::Synthetique => {
            let s = TextureSynthetique::new(cap.d3d_device(), p.largeur_synth, p.hauteur_synth)?;
            (p.largeur_synth, p.hauteur_synth, Some(s))
        }
    };

    let mut enc = NvencEncoder::new(
        cap.d3d_device(),
        p.codec,
        largeur,
        hauteur,
        FPS,
        p.bitrate_mbps * 1_000_000,
    )?;

    println!(
        "\nRésolution {largeur}x{hauteur}, {}, plancher {} Mbps, plafond {} Mbps.\n",
        p.codec.label(),
        p.floor_mbps,
        p.bitrate_mbps
    );
    std::io::stdout().flush().ok();

    let t0 = Instant::now();
    let fin = t0 + Duration::from_secs(p.secondes);
    let periode = Duration::from_micros(1_000_000 / FPS as u64);
    let mut prochaine_image = Instant::now();

    // Jetons du régulateur de débit, en octets. Rechargés à chaque tour au
    // rythme de `pacer.target_bps()`, jamais dépensés par un paquet déjà
    // produit : voir le commentaire dans la boucle.
    let mut budget_octets = 0.0f64;
    let mut dernier_budget = Instant::now();

    let mut envoyes_octets = 0u64;
    let mut images_encodees = 0u64;
    let mut images_sautees = 0u64;
    let mut retours = 0u64;
    let mut echecs_send = 0u64;
    let mut tentatives_send = 0u64;
    // Durée de chaque appel à `NvencEncoder::encode` (mesurée par la Tâche 3,
    // couvre enregistrement/mappage/encodage/démappage/désenregistrement) :
    // composante de la latence de bout en bout, rapportée en médiane/p99.
    let mut echantillons_encode_us: Vec<u64> = Vec::new();

    let mut dernier_rtt_ms: f64 = 0.0;
    let mut echantillons_rtt: Vec<f64> = Vec::new();

    let mut dernier_feedback = Instant::now();
    let mut fenetre_tentatives = 0u64;
    let mut fenetre_echecs = 0u64;

    let mut dernier_affichage = Instant::now();
    let mut octets_precedent = 0u64;

    while Instant::now() < fin {
        // 1. Recharger le budget, plafonné à 250 ms de crédit : une pause ne
        //    doit pas ensuite autoriser une rafale qui viderait le plancher.
        let maintenant = Instant::now();
        let dt = maintenant.duration_since(dernier_budget).as_secs_f64();
        dernier_budget = maintenant;
        budget_octets += dt * pacer.target_bps() as f64 / 8.0;
        budget_octets = budget_octets.min(pacer.target_bps() as f64 / 8.0 * 0.25);

        // 2. Image suivante : rythmée à FPS pour la source synthétique ;
        //    WGC ne livre une image que si le contenu de l'écran a changé.
        //    Le lien est servi PENDANT l'attente, pas seulement après.
        let image = match &mut synth {
            Some(s) => {
                loop {
                    let m = Instant::now();
                    if m >= prochaine_image {
                        break;
                    }
                    if let Some(raison) = servir_reseau(
                        &mut link,
                        &mut retours,
                        &mut dernier_rtt_ms,
                        &mut echantillons_rtt,
                    )? {
                        println!("\nÉCHEC : {raison}");
                        return Ok(());
                    }
                    std::thread::sleep(
                        GRANULARITE_SERVICE_RESEAU
                            .min(prochaine_image.saturating_duration_since(m)),
                    );
                }
                prochaine_image += periode;
                Some(s.prochaine_image()?)
            }
            None => {
                let mut trouvee = None;
                let echeance = Instant::now() + Duration::from_millis(50);
                while Instant::now() < echeance {
                    if let Some(raison) = servir_reseau(
                        &mut link,
                        &mut retours,
                        &mut dernier_rtt_ms,
                        &mut echantillons_rtt,
                    )? {
                        println!("\nÉCHEC : {raison}");
                        return Ok(());
                    }
                    if let Some(f) = cap.next_frame(GRANULARITE_SERVICE_RESEAU)? {
                        trouvee = Some(f);
                        break;
                    }
                }
                trouvee
            }
        };

        if let Some(image) = image {
            if budget_octets < 0.0 {
                // Budget épuisé : on saute la CAPTURE→ENCODAGE, jamais un
                // paquet déjà produit. NVENC chaîne ses images P sur la
                // dernière qu'il a réellement encodée (GOP infini,
                // frameIntervalP = 1) ; jeter un paquet après coup casserait
                // cette chaîne et rendrait indécodable tout ce qui suit.
                // Sauter l'image en amont laisse le flux envoyé parfaitement
                // cohérent — seule la cadence baisse.
                images_sautees += 1;
            } else if let Some(pkt) = enc.encode(&image)? {
                images_encodees += 1;
                budget_octets -= pkt.data.len() as f64;
                echantillons_encode_us.push(pkt.encode_us);

                // Redécoupé en morceaux de ≤ TAILLE_MORCEAU_PAYLOAD (voir la
                // doc de la constante) : un seul message pour tout le paquet
                // NVENC sature le tampon d'émission de `str0m` à ce débit.
                let horodatage = epoch_us();
                let nb_morceaux = pkt.data.chunks(TAILLE_MORCEAU_PAYLOAD).count().max(1);
                for (i, morceau) in pkt.data.chunks(TAILLE_MORCEAU_PAYLOAD).enumerate() {
                    let mut charge = Vec::with_capacity(EN_TETE_MORCEAU + morceau.len());
                    charge.extend_from_slice(&horodatage.to_le_bytes());
                    charge.push(if i == 0 { 1 } else { 0 });
                    charge.extend_from_slice(morceau);

                    tentatives_send += 1;
                    match envoyer_ou_abandonner(
                        &mut link,
                        &charge,
                        &mut envoyes_octets,
                        &mut echecs_send,
                        &mut fenetre_tentatives,
                        &mut fenetre_echecs,
                        &mut retours,
                        &mut dernier_rtt_ms,
                        &mut echantillons_rtt,
                    )? {
                        ResultatEnvoi::Envoye => {}
                        ResultatEnvoi::Abandonne => {
                            println!(
                                "\nÉCHEC : tampon d'émission saturé plus de {} ms \
                                 (morceau {}/{nb_morceaux}) — arrêt pour ne pas \
                                 produire un flux corrompu.",
                                BUDGET_RETRY_ENVOI.as_millis(),
                                i + 1,
                            );
                            return Ok(());
                        }
                        ResultatEnvoi::LienTombe(raison) => {
                            println!("\nÉCHEC : {raison}");
                            return Ok(());
                        }
                    }
                }
            }
        }

        // 3. Un dernier service réseau après l'encodage/envoi : capte au plus
        //    tôt le retour que notre propre envoi vient de déclencher. Voir
        //    `servir_reseau` — c'est elle qui prouve le sens inverse et tire
        //    un RTT réel de l'horodatage renvoyé par le spectateur.
        if let Some(raison) = servir_reseau(
            &mut link,
            &mut retours,
            &mut dernier_rtt_ms,
            &mut echantillons_rtt,
        )? {
            println!("\nÉCHEC : {raison}");
            return Ok(());
        }

        // 4. Nourrir le régulateur de débit à ~10 Hz.
        if dernier_feedback.elapsed() >= Duration::from_millis(100) {
            let perte_pct = if fenetre_tentatives > 0 {
                fenetre_echecs as f32 / fenetre_tentatives as f32 * 100.0
            } else {
                0.0
            };
            pacer.on_feedback(
                perte_pct,
                dernier_rtt_ms.round() as u32,
                dernier_feedback.elapsed(),
            );
            fenetre_tentatives = 0;
            fenetre_echecs = 0;
            dernier_feedback = Instant::now();
        }

        // 5. Affichage périodique — utile pour observer la remontée du
        //    plancher vers le plafond en l'absence de congestion.
        if dernier_affichage.elapsed() >= Duration::from_secs(1) {
            let delta = envoyes_octets - octets_precedent;
            let ecoule = dernier_affichage.elapsed().as_secs_f64();
            println!(
                "  {:.1} Mbps envoyés | cible pacer {:.1} Mbps | {images_sautees} images sautées cumulées | RTT {dernier_rtt_ms:.1} ms",
                delta as f64 * 8.0 / ecoule / 1e6,
                pacer.target_bps() as f64 / 1e6,
            );
            std::io::stdout().flush().ok();
            octets_precedent = envoyes_octets;
            dernier_affichage = Instant::now();
        }
    }

    let ecoule = t0.elapsed().as_secs_f64();
    echantillons_rtt.sort_by(|a, b| a.partial_cmp(b).unwrap());
    echantillons_encode_us.sort_unstable();

    println!("\n--- Résumé de la chaîne complète (Q4) ---");
    println!("Durée              : {ecoule:.1} s");
    println!("Images encodées    : {images_encodees}");
    println!("Images sautées     : {images_sautees} (régulation du débit)");
    if !echantillons_encode_us.is_empty() {
        println!(
            "Encodage médian/p99: {:.2} ms / {:.2} ms  ({} échantillons)",
            percentile_u64(&echantillons_encode_us, 50) as f64 / 1000.0,
            percentile_u64(&echantillons_encode_us, 99) as f64 / 1000.0,
            echantillons_encode_us.len()
        );
    }
    println!(
        "Débit soutenu      : {:.1} Mbps ({} Mo envoyés)",
        envoyes_octets as f64 * 8.0 / ecoule / 1e6,
        envoyes_octets / 1_000_000
    );
    println!(
        "Cible finale pacer : {:.1} Mbps (plancher {}, plafond {})",
        pacer.target_bps() as f64 / 1e6,
        p.floor_mbps,
        p.bitrate_mbps
    );
    println!("Retours reçus      : {retours}");
    println!("Échecs d'envoi     : {echecs_send} / {tentatives_send}");
    if echantillons_rtt.is_empty() {
        println!("RTT                : non mesuré (aucun retour reçu)");
    } else {
        println!(
            "RTT médian / p99   : {:.2} ms / {:.2} ms  ({} échantillons)",
            percentile_f64(&echantillons_rtt, 50),
            percentile_f64(&echantillons_rtt, 99),
            echantillons_rtt.len()
        );
    }
    println!("Rappel : lien en boucle locale sur cette machine — ce débit et ce RTT");
    println!("mesurent le chiffrement et le transport en mémoire, jamais un réseau.");
    Ok(())
}

/// Un tour de service réseau : vide un événement de `link`, compte les
/// retours et met à jour l'échantillon de RTT (l'horodatage que le
/// spectateur renvoie tel quel). Factorisée parce qu'elle est appelée à
/// plusieurs points de la boucle principale — voir `GRANULARITE_SERVICE_RESEAU`
/// pour pourquoi la fréquence d'appel compte ici.
///
/// Rend `Some(raison)` si le lien est tombé ; l'appelant doit alors arrêter.
fn servir_reseau(
    link: &mut PeerLink,
    retours: &mut u64,
    dernier_rtt_ms: &mut f64,
    echantillons_rtt: &mut Vec<f64>,
) -> anyhow::Result<Option<String>> {
    match link.poll()? {
        LinkEvent::Data(d) => {
            *retours += 1;
            if let Ok(brut) = <[u8; 8]>::try_from(d.as_slice()) {
                let echo = u64::from_le_bytes(brut);
                let maintenant_us = epoch_us();
                if maintenant_us >= echo {
                    let rtt_ms = (maintenant_us - echo) as f64 / 1000.0;
                    *dernier_rtt_ms = rtt_ms;
                    echantillons_rtt.push(rtt_ms);
                }
            }
            Ok(None)
        }
        LinkEvent::Failed(raison) => Ok(Some(raison)),
        _ => Ok(None),
    }
}

/// Issue d'une tentative d'envoi d'un morceau déjà encodé.
enum ResultatEnvoi {
    /// Parti — au premier coup ou après relance.
    Envoye,
    /// Le tampon d'émission est resté plein au-delà de `BUDGET_RETRY_ENVOI`.
    Abandonne,
    /// Le lien est tombé pendant l'attente ; raison déjà rédigée pour
    /// l'utilisateur, sans adresse.
    LienTombe(String),
}

/// Envoie un morceau déjà découpé, en relançant tant que le tampon
/// d'émission est plein, jusqu'à `BUDGET_RETRY_ENVOI`. Sert le lien entre
/// chaque tentative (`servir_reseau`) : c'est ce service qui vide le tampon
/// en traitant les accusés de réception de `str0m`.
///
/// Un morceau déjà encodé DOIT partir : NVENC vient de chaîner l'image dont
/// il fait partie sur la dernière qu'il a réellement encodée (GOP infini,
/// frameIntervalP = 1), et sous ce régime aucune image de référence ne
/// revient jamais. L'abandonner silencieusement casserait cette chaîne pour
/// tout le reste du flux — c'est précisément ce qu'a révélé un premier essai
/// de ce banc de test : ffprobe rapportait des « ref POC introuvable » en
/// cascade dès le premier échec d'envoi ignoré. D'où la relance, et l'arrêt
/// propre plutôt qu'une mesure sur un flux qu'on sait corrompu.
#[allow(clippy::too_many_arguments)]
fn envoyer_ou_abandonner(
    link: &mut PeerLink,
    charge: &[u8],
    envoyes_octets: &mut u64,
    echecs_send: &mut u64,
    fenetre_tentatives: &mut u64,
    fenetre_echecs: &mut u64,
    retours: &mut u64,
    dernier_rtt_ms: &mut f64,
    echantillons_rtt: &mut Vec<f64>,
) -> anyhow::Result<ResultatEnvoi> {
    let debut = Instant::now();
    let mut retente = false;
    loop {
        match link.send(charge) {
            Ok(()) => {
                *envoyes_octets += charge.len() as u64;
                if retente {
                    // Congestion réelle, résorbée : signal légitime pour le
                    // Pacer, même si le morceau est finalement parti.
                    *echecs_send += 1;
                    *fenetre_echecs += 1;
                }
                *fenetre_tentatives += 1;
                // `link.send` ne fait que déposer le morceau dans le tampon
                // interne de `str0m` : les octets ne partent réellement sur
                // le socket que pendant `poll()`. Découverte de banc de test :
                // sans ce drainage après CHAQUE morceau, une rafale de
                // plusieurs dizaines de morceaux d'une même image s'empile
                // sans jamais être poussée sur le fil, jusqu'à saturer le
                // tampon — RTT en centaines de ms puis échec, en boucle
                // locale. Un morceau parti selon `str0m` n'est pas encore un
                // morceau émis sur le réseau.
                if let Some(raison) =
                    servir_reseau(link, retours, dernier_rtt_ms, echantillons_rtt)?
                {
                    return Ok(ResultatEnvoi::LienTombe(raison));
                }
                return Ok(ResultatEnvoi::Envoye);
            }
            Err(_) if debut.elapsed() < BUDGET_RETRY_ENVOI => {
                retente = true;
                if let Some(raison) =
                    servir_reseau(link, retours, dernier_rtt_ms, echantillons_rtt)?
                {
                    return Ok(ResultatEnvoi::LienTombe(raison));
                }
                std::thread::sleep(GRANULARITE_SERVICE_RESEAU);
            }
            Err(_) => return Ok(ResultatEnvoi::Abandonne),
        }
    }
}

fn percentile_f64(tries: &[f64], p: usize) -> f64 {
    if tries.is_empty() {
        return 0.0;
    }
    let idx = (tries.len() * p / 100).min(tries.len() - 1);
    tries[idx]
}

fn percentile_u64(tries: &[u64], p: usize) -> u64 {
    if tries.is_empty() {
        return 0;
    }
    let idx = (tries.len() * p / 100).min(tries.len() - 1);
    tries[idx]
}

/// Boucle jusqu'à ce que le canal de données soit utilisable, ou renonce.
///
/// Partagée avec `cmd_view` : les deux bords appliquent exactement le même
/// délai et le même diagnostic. Rend `None` quand la tentative a échoué — le
/// message a déjà été affiché.
///
/// Aucune adresse n'est affichée : les diagnostics parlent de causes, jamais
/// de machines.
pub fn etablir(link: &mut PeerLink) -> anyhow::Result<Option<Duration>> {
    let debut = Instant::now();
    loop {
        match link.poll()? {
            LinkEvent::Failed(raison) => {
                println!("ÉCHEC : {raison}");
                return Ok(None);
            }
            LinkEvent::Connected | LinkEvent::Data(_) | LinkEvent::Idle => {}
        }

        // Le canal de données, pas seulement ICE : c'est lui qui transporte.
        if link.canal_ouvert() {
            return Ok(Some(debut.elapsed()));
        }

        if debut.elapsed() > DELAI_ETABLISSEMENT {
            diagnostiquer(link);
            return Ok(None);
        }

        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Explique l'échec sans accuser le NAT à tort.
///
/// `etablir` attend l'ouverture du canal de données, qui vient bien après ICE :
/// un échec peut donc venir du perçage de NAT, ou de la poignée de main chiffrée
/// qui le suit. Ce sont deux verdicts opposés pour la question centrale du
/// jalon, et c'est ce message qui sera consigné comme réponse. Il doit donc
/// distinguer les deux, et dire quand il n'est pas sûr de lui.
fn diagnostiquer(link: &PeerLink) {
    if link.is_connected() {
        println!(
            "ÉCHEC : le canal de données ne s'est pas ouvert en {} s.",
            DELAI_ETABLISSEMENT.as_secs()
        );
        println!("ATTENTION : la traversée de NAT n'est PAS en cause. Les deux machines");
        println!("se sont bel et bien trouvées — c'est la poignée de main chiffrée");
        println!("(DTLS/SCTP) qui n'a pas abouti. À ne pas compter comme un échec Q5.");
    } else {
        let (emis, recus, erreurs) = link.trafic();
        println!(
            "ÉCHEC : aucune connexion directe en {} s.",
            DELAI_ETABLISSEMENT.as_secs()
        );
        println!();
        println!("  Datagrammes émis   : {emis}");
        println!("  Datagrammes reçus  : {recus}");
        println!("  Erreurs de socket  : {erreurs}");
        let (prive, public) = link.destinations();
        println!("  dont vers reseau local : {prive}");
        println!("  dont vers internet     : {public}");
        println!();

        // Ces trois nombres distinguent des causes que « NAT strict » confondait.
        if emis == 0 {
            println!("Aucun paquet n'a été émis : l'agent ICE n'a pas de destination.");
            println!("La réponse collée ne contenait donc aucune adresse exploitable.");
            println!("C'est un défaut de notre côté, pas un problème de réseau.");
        } else if recus == 0 {
            println!("Nous avons émis sans jamais rien recevoir en retour.");
            println!("Trois causes, et rien d'ici ne permet de trancher :");
            println!("  - le correspondant n'avait plus son programme ouvert ;");
            println!("  - il ne l'a pas lancé au même moment que nous ;");
            println!("  - ses paquets sortent mais les nôtres n'arrivent pas jusqu'à lui.");
            println!();
            println!("Si vous avez tous les deux obtenu « réseau compatible » avec");
            println!("`sky-probe netcheck`, la première cause est de loin la plus probable :");
            println!("le programme du spectateur doit rester ouvert pendant que l'émetteur");
            println!("colle la réponse.");
        } else {
            println!("Des paquets ont circulé DANS LES DEUX SENS, sans que la négociation");
            println!("aboutisse. Le réseau fait son travail : la traversée de NAT n'est pas");
            println!("en cause. Le défaut est dans notre code ou dans la négociation ICE.");
        }
    }

    let erreurs = link.erreurs_socket();
    if erreurs > 0 {
        println!();
        println!("Réserve : {erreurs} erreur(s) sur le port UDP local pendant la tentative.");
        println!("Une cause locale (pare-feu, interface qui change) n'est pas exclue :");
        println!("le diagnostic ci-dessus est à prendre avec précaution.");
    }
}
