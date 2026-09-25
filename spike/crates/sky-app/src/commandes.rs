//! Les commandes Tauri : des intentions de l'interface, confiées au `Noyau`
//! sur un fil dédié (`spawn_blocking`) — jamais sur le fil de l'interface :
//! `sky-compte` bloque le temps d'une requête (5 s au plus), `connexion` le
//! temps d'une connexion Discord (5 minutes au plus). C'est le seul fichier du
//! cœur qui contient des `async fn`.

use std::sync::Arc;

use tauri::State;

use crate::noyau::Noyau;
use crate::vue::Instantane;

pub(crate) async fn sur_un_fil<T: Send + 'static>(
    noyau: &Arc<Noyau>,
    travail: impl FnOnce(&Arc<Noyau>) -> T + Send + 'static,
) -> Result<T, String> {
    let noyau = Arc::clone(noyau);
    tauri::async_runtime::spawn_blocking(move || travail(&noyau)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn etat_courant(noyau: State<'_, Arc<Noyau>>) -> Result<Instantane, String> {
    Ok(noyau.instantane())
}

#[tauri::command]
pub async fn connexion(noyau: State<'_, Arc<Noyau>>) -> Result<(), String> {
    sur_un_fil(noyau.inner(), |n| n.connexion()).await?
}

#[tauri::command]
pub async fn deconnexion(noyau: State<'_, Arc<Noyau>>) -> Result<(), String> {
    sur_un_fil(noyau.inner(), |n| n.deconnexion()).await?
}

/// `code` : la saisie BRUTE de l'interface. La normalisation appartient au
/// cœur (`sky_compte::normaliser_code_ami`, qui reproduit celle du site), pas à
/// l'interface : deux normalisations divergeraient à la première correction.
#[tauri::command]
pub async fn ajouter_ami(noyau: State<'_, Arc<Noyau>>, code: String) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.ajouter_ami(&code)).await?
}

/// `friendship_id` : identifiant d'AMITIÉ, jamais d'utilisateur — c'est ce que
/// lisent les routes `friends/{id}` du site.
#[tauri::command]
pub async fn accepter_ami(
    noyau: State<'_, Arc<Noyau>>,
    friendship_id: i64,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.accepter_ami(friendship_id)).await?
}

#[tauri::command]
pub async fn retirer_ami(
    noyau: State<'_, Arc<Noyau>>,
    friendship_id: i64,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.retirer_ami(friendship_id)).await?
}

#[tauri::command]
pub async fn bloquer_ami(
    noyau: State<'_, Arc<Noyau>>,
    friendship_id: i64,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.bloquer_ami(friendship_id)).await?
}

// --- Listes de diffusion ---------------------------------------------------
//
// Les arguments voyagent en camelCase depuis l'interface : Tauri 2 résout
// chaque paramètre PAR SA CLÉ, sans repli — une clé absente est une erreur
// d'invocation, pas un `None` silencieux. `friendship_id` s'invoque donc
// `{ friendshipId }`, et les noms d'un seul mot ci-dessous restent identiques
// des deux côtés.

#[tauri::command]
pub async fn creer_liste(
    noyau: State<'_, Arc<Noyau>>,
    nom: String,
    couleur: Option<String>,
    emoji: Option<String>,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.creer_liste(&nom, couleur.as_deref(), emoji.as_deref()))
        .await?
}

#[tauri::command]
pub async fn modifier_liste(
    noyau: State<'_, Arc<Noyau>>,
    id: i64,
    nom: String,
    couleur: Option<String>,
    emoji: Option<String>,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| {
        n.modifier_liste(id, &nom, couleur.as_deref(), emoji.as_deref())
    })
    .await?
}

#[tauri::command]
pub async fn supprimer_liste(noyau: State<'_, Arc<Noyau>>, id: i64) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.supprimer_liste(id)).await?
}

/// `membres` : identifiants d'UTILISATEUR des amis cochés, jamais d'amitié.
#[tauri::command]
pub async fn definir_membres(
    noyau: State<'_, Arc<Noyau>>,
    id: i64,
    membres: Vec<i64>,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.definir_membres(id, &membres)).await?
}

// --- Mon compte ------------------------------------------------------------

#[tauri::command]
pub async fn regenerer_code(noyau: State<'_, Arc<Noyau>>) -> Result<String, String> {
    sur_un_fil(noyau.inner(), |n| n.regenerer_code()).await?
}

#[tauri::command]
pub async fn revoquer_appareil(noyau: State<'_, Arc<Noyau>>, id: i64) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.revoquer_appareil(id)).await?
}

/// Touche le registre de Windows par la coquille : `spawn_blocking` comme les
/// autres, jamais le fil de l'interface.
#[tauri::command]
pub async fn demarrage_automatique(
    noyau: State<'_, Arc<Noyau>>,
    actif: bool,
) -> Result<(), String> {
    sur_un_fil(noyau.inner(), move |n| n.demarrage_automatique(actif)).await?
}

// --- Partage ---------------------------------------------------------------

/// `ecran` : le RANG dans la liste `ecrans` de l'instantané, jamais un
/// identifiant Windows — c'est ce que la capture attend.
#[tauri::command]
pub async fn partager(noyau: State<'_, Arc<Noyau>>, ecran: usize) -> Result<(), String> {
    sur_un_fil(noyau.inner(), move |n| n.partager(ecran)).await?
}

/// `ami` : identifiant d'UTILISATEUR (`AmiVue.id`), jamais d'amitié.
#[tauri::command]
pub async fn regarder(noyau: State<'_, Arc<Noyau>>, ami: i64) -> Result<(), String> {
    sur_un_fil(noyau.inner(), move |n| n.regarder(ami)).await?
}

/// Pas de `spawn_blocking` : lever un drapeau atomique ne bloque pas, et c'est
/// le fil du partage qui fait le travail. Attendre un fil ici retiendrait celui
/// de l'interface le temps que la négociation en cours s'arrête.
#[tauri::command]
pub async fn arreter(noyau: State<'_, Arc<Noyau>>) -> Result<(), String> {
    noyau.arreter();
    Ok(())
}

/// LE CONTRAT DE COMMANDES AVEC `app/src/pont.ts`, dans les TROIS sens.
///
/// EXIGENCE DU CONTRÔLEUR (tâche 11), prolongement du contrat de forme de
/// `vue.rs`. Celui-là compare les instantanés QUE LE CŒUR ÉMET aux types que
/// l'interface déclare. Rien ne couvrait le sens INVERSE — les intentions que
/// l'interface envoie —, et c'est justement là que la tâche 11 ajoute trois
/// commandes, sur un chemin Tauri qui n'a JAMAIS été exécuté : l'essai réel du
/// jalon a été reporté.
///
/// Une commande peut rater de trois façons, dont aucune ne se voit à la
/// compilation ni au `npm build` :
/// - déclarée dans `commandes.rs` mais absente de `generate_handler!` : Tauri
///   répond « command not found » au premier clic ;
/// - appelée par `pont.ts` sous un autre nom ;
/// - invoquée avec une clé d'argument différente : Tauri 2 résout chaque
///   paramètre PAR SA CLÉ, sans repli — une clé absente est une erreur
///   d'invocation, pas un `None` silencieux (voir l'en-tête des listes).
///
/// Les trois ensembles sont donc comparés deux à deux, `commande` et
/// `commande.argument` confondus dans le même jeu. `pont.ts` écrit ses clés en
/// camelCase, que Tauri traduit : la comparaison se fait en snake_case.
///
/// CE QU'IL NE COUVRE PAS : les TYPES des arguments (`number` contre `string`),
/// l'ordre, et tout le reste du chemin — que Tauri transporte, que le noyau
/// fasse ce qu'il annonce. SEUL un essai réel le dira.
#[cfg(test)]
mod contrat_commandes {
    use std::collections::BTreeSet;

    const COMMANDES_RS: &str = include_str!("commandes.rs");
    const LIB_RS: &str = include_str!("lib.rs");
    const PONT_TS: &str = include_str!("../../../../app/src/pont.ts");

    /// Le fichier PRIVÉ de son propre module de tests. Sans cette coupe,
    /// l'extraction retrouve ici les marqueurs qu'elle CHERCHE — ils sont
    /// écrits en toutes lettres dans le code ci-dessous — et se fabrique une
    /// commande fantôme. Mesuré : c'est exactement ce qui est arrivé.
    /// Le marqueur est épelé en deux morceaux pour ne pas se trouver lui-même.
    fn sans_les_tests(source: &str) -> &str {
        source.split(concat!("#[cfg", "(test)]")).next().unwrap_or(source)
    }

    fn en_snake_case(camel: &str) -> String {
        let mut snake = String::with_capacity(camel.len() + 2);
        for c in camel.chars() {
            if c.is_ascii_uppercase() {
                snake.push('_');
                snake.push(c.to_ascii_lowercase());
            } else {
                snake.push(c);
            }
        }
        snake
    }

    /// Les commandes déclarées : chaque `#[tauri::command]` et les paramètres
    /// de la `pub async fn` qui la suit, `noyau` excepté.
    fn declarees(source: &str) -> BTreeSet<String> {
        let mut jeu = BTreeSet::new();
        for apres in source.split("#[tauri::command]").skip(1) {
            let Some(apres) = apres.split_once("pub async fn ") else { continue };
            let Some((nom, reste)) = apres.1.split_once('(') else { continue };
            let Some((parametres, _)) = reste.split_once(')') else { continue };
            let nom = nom.trim();
            jeu.insert(nom.to_string());
            for parametre in parametres.split(',') {
                let Some((champ, _)) = parametre.split_once(':') else { continue };
                let champ = champ.trim();
                // `State<'_, Arc<Noyau>>` porte une virgule : le morceau qui la
                // suit n'a pas de `:` propre et tombe déjà. `noyau` est le
                // porteur du noyau, jamais un argument de l'interface.
                if champ.is_empty() || champ == "noyau" || !champ.starts_with(char::is_alphabetic) {
                    continue;
                }
                jeu.insert(format!("{nom}.{champ}"));
            }
        }
        jeu
    }

    /// Les commandes enregistrées dans `generate_handler!`. Sans arguments :
    /// la macro n'en nomme aucun.
    fn enregistrees(source: &str) -> BTreeSet<String> {
        let Some((_, apres)) = source.split_once("generate_handler![") else {
            panic!("`generate_handler!` est introuvable dans lib.rs");
        };
        let (bloc, _) = apres.split_once(']').expect("`generate_handler!` non refermé");
        bloc
            .split(',')
            .filter_map(|entree| entree.trim().strip_prefix("commandes::"))
            .map(str::to_string)
            .collect()
    }

    /// Les commandes appelées : chaque `invoke<…>("nom"` et, s'il y en a, les
    /// clés de son objet d'arguments.
    fn appelees(source: &str) -> BTreeSet<String> {
        let mut jeu = BTreeSet::new();
        for apres in source.split("invoke<").skip(1) {
            let Some((_, apres)) = apres.split_once('"') else { continue };
            let Some((nom, reste)) = apres.split_once('"') else { continue };
            jeu.insert(nom.to_string());
            // L'objet d'arguments, s'il existe, suit immédiatement la virgule
            // et précède la parenthèse fermante de l'appel.
            let Some((avant_fin, _)) = reste.split_once(')') else { continue };
            let Some((_, objet)) = avant_fin.split_once('{') else { continue };
            let Some((objet, _)) = objet.split_once('}') else { continue };
            for clef in objet.split(',') {
                // `{ nom }` (abrégé) comme `{ nom: valeur }`.
                let clef = clef.split(':').next().unwrap_or("").trim();
                if !clef.is_empty() {
                    jeu.insert(format!("{nom}.{}", en_snake_case(clef)));
                }
            }
        }
        jeu
    }

    #[test]
    fn chaque_commande_est_declaree_enregistree_et_appelee_sous_le_meme_nom() {
        let declarees = declarees(sans_les_tests(COMMANDES_RS));
        let enregistrees = enregistrees(LIB_RS);
        let appelees = appelees(PONT_TS);

        // CONTRÔLE POSITIF : sans lui, trois extractions muettes rendraient
        // trois ensembles vides, donc égaux, et ce test ne dirait plus rien.
        assert!(
            declarees.len() >= 20 && enregistrees.len() >= 15 && appelees.len() >= 20,
            "une extraction ne lit plus son fichier : {} déclarées, {} enregistrées, {} appelées",
            declarees.len(),
            enregistrees.len(),
            appelees.len()
        );

        let noms_declares: BTreeSet<&String> =
            declarees.iter().filter(|e| !e.contains('.')).collect();
        let noms_enregistres: BTreeSet<&String> = enregistrees.iter().collect();
        assert_eq!(
            noms_declares, noms_enregistres,
            "une commande est déclarée sans être dans generate_handler!, ou l'inverse : \
             Tauri répondrait « command not found » au premier clic"
        );
        assert_eq!(
            declarees, appelees,
            "les commandes (ou leurs clés d'arguments) ne correspondent plus entre \
             commandes.rs et app/src/pont.ts"
        );
    }
}
