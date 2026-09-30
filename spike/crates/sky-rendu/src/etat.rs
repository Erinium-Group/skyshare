//! Les trois états d'une fenêtre de visionnage qui n'a pas (ou plus) d'image.
//!
//! « Une fenêtre noire muette est un défaut, pas un état » (spec §5) : chaque
//! état dit ce qui se passe, en français, et a sa propre teinte de fond. La
//! teinte n'est pas décorative : elle garantit que le test compare des pixels
//! réellement différents, et pas seulement du texte au même emplacement.

use anyhow::Context;
use windows::core::Interface;
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{ID2D1RenderTarget, D2D1_DRAW_TEXT_OPTIONS_NONE};
use windows::Win32::Graphics::DirectWrite::{IDWriteTextFormat, DWRITE_MEASURING_MODE_NATURAL};

#[cfg(test)]
use crate::Fenetre;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtatVisionnage {
    /// La connexion est établie, aucune image n'est encore arrivée.
    EnAttente,
    /// Le lien avec l'hôte est rompu.
    ConnexionPerdue,
    /// L'hôte a arrêté le partage.
    PartageArrete,
}

/// Ce qu'un état montre : une teinte de fond et une phrase.
struct Apparence {
    fond: D2D1_COLOR_F,
    texte: &'static str,
}

/// Blanc cassé, lisible sur les trois fonds.
const COULEUR_TEXTE: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0.93,
    g: 0.93,
    b: 0.95,
    a: 1.0,
};

const fn fond(r: f32, g: f32, b: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a: 1.0 }
}

fn apparence(etat: EtatVisionnage) -> Apparence {
    match etat {
        // Bleu nuit : rien d'anormal, on attend.
        EtatVisionnage::EnAttente => Apparence {
            fond: fond(0.06, 0.10, 0.22),
            texte: "En attente de l'image…",
        },
        // Rouge sombre : quelque chose s'est cassé.
        EtatVisionnage::ConnexionPerdue => Apparence {
            fond: fond(0.28, 0.07, 0.07),
            texte: "Connexion perdue",
        },
        // Gris neutre : fin normale, pas une panne.
        EtatVisionnage::PartageArrete => Apparence {
            fond: fond(0.16, 0.16, 0.17),
            texte: "Le partage s'est arrêté",
        },
    }
}

/// Peint un état sur `cible` : fond uni, puis la phrase centrée.
pub(crate) fn dessiner(
    cible: &ID2D1RenderTarget,
    format: &IDWriteTextFormat,
    etat: EtatVisionnage,
    largeur: u32,
    hauteur: u32,
) -> anyhow::Result<()> {
    let apparence = apparence(etat);
    let texte: Vec<u16> = apparence.texte.encode_utf16().collect();
    let zone = D2D_RECT_F {
        left: 0.0,
        top: 0.0,
        right: largeur as f32,
        bottom: hauteur as f32,
    };
    unsafe {
        let pinceau = cible
            .CreateSolidColorBrush(&COULEUR_TEXTE, None)
            .context("création du pinceau")?;
        cible.BeginDraw();
        cible.Clear(Some(&apparence.fond));
        cible.DrawText(
            &texte,
            format,
            &zone,
            &pinceau.cast::<windows::Win32::Graphics::Direct2D::ID2D1Brush>()?,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
        cible
            .EndDraw(None, None)
            .context("fin du dessin Direct2D")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Échoue bruyamment si la fenêtre ne s'ouvre pas : le projet est Windows
    /// seulement, et un test qui se tairait laisserait passer une vraie
    /// régression (« une preuve qui passerait aussi bien dans le cas négatif
    /// n'est pas une preuve »).
    fn ouvrir_pour_test() -> Fenetre {
        Fenetre::ouvrir_masquee("test", 320, 200)
            .expect("la fenêtre masquée doit s'ouvrir (Direct3D 11 requis)")
    }

    /// Le pixel (x, y) d'un tampon BGRA de 320 pixels de large.
    fn pixel(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
        let debut = (y * 320 + x) * 4;
        pixels[debut..debut + 4].try_into().unwrap()
    }

    /// Les fonds seuls, indépendamment du texte : on échantillonne deux coins,
    /// loin de la phrase centrée. Sans cette assertion, trois fonds identiques
    /// avec des phrases différentes passeraient le test des empreintes.
    #[test]
    fn les_trois_fonds_sont_distincts() {
        let mut fenetre = ouvrir_pour_test();
        let mut fonds = Vec::new();
        for etat in [
            EtatVisionnage::EnAttente,
            EtatVisionnage::ConnexionPerdue,
            EtatVisionnage::PartageArrete,
        ] {
            fenetre.afficher_etat(etat).expect("rendu");
            let pixels = fenetre.pixels_de_la_cible().expect("lecture du tampon");
            let haut_gauche = pixel(&pixels, 2, 2);
            let bas_droite = pixel(&pixels, 317, 197);
            assert_eq!(haut_gauche, bas_droite, "{etat:?} : fond non uni");
            fonds.push(haut_gauche);
        }
        assert_ne!(
            fonds[0], fonds[1],
            "fonds de l'attente et de la perte se confondent"
        );
        assert_ne!(
            fonds[1], fonds[2],
            "fonds de la perte et de l'arrêt se confondent"
        );
        assert_ne!(
            fonds[0], fonds[2],
            "fonds de l'attente et de l'arrêt se confondent"
        );
    }

    /// Une fenêtre noire muette est un défaut, pas un état (spec §5).
    /// Ce test prouve que les trois états se distinguent réellement : il compare
    /// les pixels rendus, pas le nom de l'état.
    #[test]
    fn les_trois_etats_donnent_trois_rendus_differents() {
        let mut fenetre = ouvrir_pour_test();
        let mut empreintes = Vec::new();
        for etat in [
            EtatVisionnage::EnAttente,
            EtatVisionnage::ConnexionPerdue,
            EtatVisionnage::PartageArrete,
        ] {
            fenetre.afficher_etat(etat).expect("rendu");
            empreintes.push(fenetre.empreinte_du_tampon().expect("lecture du tampon"));
        }
        assert_ne!(
            empreintes[0], empreintes[1],
            "attente et connexion perdue se confondent"
        );
        assert_ne!(
            empreintes[1], empreintes[2],
            "connexion perdue et arrêt se confondent"
        );
        assert_ne!(
            empreintes[0], empreintes[2],
            "attente et arrêt se confondent"
        );
    }

    /// Une teinte seule ferait une fenêtre muette : chaque état doit aussi avoir
    /// écrit sa phrase, c'est-à-dire des pixels d'une autre couleur que le fond.
    #[test]
    fn chaque_etat_ecrit_sa_phrase_sur_son_fond() {
        let mut fenetre = ouvrir_pour_test();
        for etat in [
            EtatVisionnage::EnAttente,
            EtatVisionnage::ConnexionPerdue,
            EtatVisionnage::PartageArrete,
        ] {
            fenetre.afficher_etat(etat).expect("rendu");
            let pixels = fenetre.pixels_de_la_cible().expect("lecture du tampon");
            let fond = &pixels[..4];
            let hors_fond = pixels.chunks_exact(4).filter(|p| *p != fond).count();
            assert!(
                hors_fond > 100,
                "{etat:?} : {hors_fond} pixels hors fond, texte absent"
            );
        }
    }
}
