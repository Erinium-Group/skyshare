//! Doublures de test du transport : de quoi éprouver les boucles d'envoi et de
//! réception sans GPU, sans écran et sans réseau.
//!
//! Ce module est un module de crate, compilé sous `#[cfg(test)]`, et non un
//! morceau du module de tests de `hote` : le côté spectateur en a besoin aussi,
//! et ne pourrait pas l'atteindre s'il vivait dans `mod tests`.

use std::cell::Cell;
use std::collections::VecDeque;
use std::rc::Rc;

use sky_decode::{ErreurDecodeur, SourceImage, SurfaceCuda};
use sky_net::{ErreurEnvoi, LinkEvent, MessageControle};
use sky_rendu::{EtatVisionnage, EvenementFenetre, ImageAAfficher};

use crate::hote::LienVideo;
use crate::spectateur::{Afficheur, Decodage, LienSpectateur};

/// Une unité d'accès telle que l'hôte l'a écrite sur la piste média.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ecriture {
    pub unite: Vec<u8>,
    pub horodatage_ms: u64,
}

/// Un lien pair-à-pair sans réseau : il retient ce qu'on lui écrit et rend les
/// événements qu'on lui injecte.
///
/// Il imite `PeerLink::ecrire_image` sur un point : un refus
/// `TropDImagesEnAttente` ne se lève pas tout seul, il faut poller. C'est ce
/// qui permet de vérifier que l'appelant sait faire le geste attendu — poller
/// puis réécrire la MÊME image — plusieurs fois de suite sans conclure à un
/// échec.
///
/// **Sur le nombre de places libérées, la doublure SIMPLIFIE le réel.** Elle en
/// libère exactement une par `poll`. Le vrai `poll` en libère **au moins une**
/// quand il va jusqu'à `Idle` (un `do_payload` par passage de `str0m` dans
/// `handle_timeout` : `Input::Timeout` et chaque datagramme injecté — voir
/// `sky_net::ErreurEnvoi::TropDImagesEnAttente`, lu dans `str0m` 0.23
/// `Media::do_payload` / `Rtc::handle_input`), et **peut-être aucune** quand il
/// rend un événement avant toute injection (déduit de la lecture de
/// `PeerLink::poll`, non mesuré). Ce n'est donc pas « le cas le plus
/// défavorable » au sens strict, et elle n'a pas été choisie comme tel :
/// jusqu'au 03/10/2026 ce commentaire présentait « une seule place » comme
/// mesuré.
///
/// Pourquoi l'écart ne fausse aucun test (vérifié le 03/10/2026, relecture des
/// deux appelants de `saturer`, tous deux dans `hote.rs`) : `envoyer_image`
/// relance jusqu'à acceptation ou `BUDGET_RETRY_ENVOI`, sans jamais compter les
/// places — un `poll` qui en libère zéro, une ou plusieurs ne change que le
/// nombre de tours. `polls() >= 2` dans
/// `une_file_pleine_se_resorbe_en_pollant_et_en_reecrivant_la_meme_image` est
/// une propriété de CE scénario (deux refus imposés par la doublure), pas du
/// réel : elle prouve que l'appelant encaisse deux refus d'affilée, rien sur le
/// rythme de `str0m`.
pub struct LienFactice {
    ecritures: Vec<Ecriture>,
    messages: Vec<MessageControle>,
    evenements: VecDeque<LinkEvent>,
    /// Nombre de places encore à libérer avant qu'une écriture soit acceptée.
    places_a_liberer: usize,
    polls: u64,
    /// Fait refuser tout message de contrôle, comme un canal qui s'est fermé.
    controles_refuses: bool,
}

impl LienFactice {
    pub fn nouveau() -> LienFactice {
        LienFactice {
            ecritures: Vec::new(),
            messages: Vec::new(),
            evenements: VecDeque::new(),
            places_a_liberer: 0,
            polls: 0,
            controles_refuses: false,
        }
    }

    /// Fait refuser tout message de contrôle : le canal est fermé.
    pub fn refuser_les_controles(&mut self) {
        self.controles_refuses = true;
    }

    /// Les unités d'accès acceptées, dans l'ordre. Une écriture refusée n'y
    /// figure pas.
    pub fn ecritures(&self) -> &[Ecriture] {
        &self.ecritures
    }

    /// Les messages de contrôle que l'hôte a envoyés, dans l'ordre.
    pub fn messages_envoyes(&self) -> &[MessageControle] {
        &self.messages
    }

    /// Fait rendre `evenement` par le prochain `poll`. Les événements sortent
    /// dans l'ordre d'injection ; quand la file est vide, `poll` rend `Idle`.
    pub fn injecter(&mut self, evenement: LinkEvent) {
        self.evenements.push_back(evenement);
    }

    /// Refuse les écritures tant que `places` n'ont pas été libérées par autant
    /// de `poll` — la file de paquetisation vue par l'appelant.
    pub fn saturer(&mut self, places: usize) {
        self.places_a_liberer = places;
    }

    pub fn polls(&self) -> u64 {
        self.polls
    }
}

impl LienVideo for LienFactice {
    fn ecrire_image(&mut self, unite: &[u8], horodatage_ms: u64) -> Result<(), ErreurEnvoi> {
        if self.places_a_liberer > 0 {
            return Err(ErreurEnvoi::TropDImagesEnAttente);
        }
        self.ecritures.push(Ecriture { unite: unite.to_vec(), horodatage_ms });
        Ok(())
    }

    fn envoyer_controle(&mut self, message: &MessageControle) -> Result<(), ErreurEnvoi> {
        if self.controles_refuses {
            return Err(ErreurEnvoi::CanalFerme);
        }
        self.messages.push(message.clone());
        Ok(())
    }

    fn poll(&mut self) -> anyhow::Result<LinkEvent> {
        self.polls += 1;
        self.places_a_liberer = self.places_a_liberer.saturating_sub(1);
        Ok(self.evenements.pop_front().unwrap_or(LinkEvent::Idle))
    }
}

/// Côté spectateur, seuls deux gestes du lien servent : écouter et demander.
impl LienSpectateur for LienFactice {
    fn envoyer_controle(&mut self, message: &MessageControle) -> Result<(), ErreurEnvoi> {
        LienVideo::envoyer_controle(self, message)
    }

    fn poll(&mut self) -> anyhow::Result<LinkEvent> {
        LienVideo::poll(self)
    }

    fn vers_internet(&self) -> u64 {
        0
    }
}

/// Une image décodée pour de faux : une géométrie, et une poignée de surface
/// qui ne pointe nulle part.
///
/// Rien ne la lit jamais : `FenetreFactice` note la géométrie et s'arrête là.
/// C'est tout l'intérêt de `ImageAAfficher` — `sky-decode` ne laisse personne
/// fabriquer une `ImageDecodee`, qui exige une session NVDEC vivante, donc sans
/// ce trait la boucle du spectateur ne serait éprouvable que sur une machine à
/// carte NVIDIA, avec un vrai flux HEVC.
pub struct ImageFactice {
    pub largeur: u32,
    pub hauteur: u32,
}

impl ImageAAfficher for ImageFactice {
    fn largeur(&self) -> u32 {
        self.largeur
    }

    fn hauteur(&self) -> u32 {
        self.hauteur
    }

    fn source(&self) -> SourceImage<'_> {
        SourceImage::Cuda444(SurfaceCuda { pointeur: 0, pas: 0, hauteur_surface: 0 })
    }
}

/// Ce qu'un appel à `Decodage::decoder` rendra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// `Ok(None)` : le décodeur a avalé l'unité sans rendre d'image — le cas des
    /// en-têtes VPS/SPS/PPS, qui n'est pas une erreur.
    Avalee,
    /// `Ok(Some(image))`.
    Image,
    /// `Err(..)` : le décodeur refuse l'unité.
    Echec,
}

/// Un décodeur dont chaque réponse est écrite d'avance.
///
/// Il ne décode rien : ce qui s'éprouve ici est la décision du spectateur
/// — afficher, attendre, demander une image clé — et non NVDEC, que
/// `sky-decode` mesure déjà sur son propre banc.
///
/// **Le contrat qu'il imite, et d'où il vient.** Une `Issue::Image` est l'image
/// de l'unité qu'on vient de lui pousser, au même appel. Pendant tout le jalon 2,
/// c'était FAUX pour le vrai décodeur : sans `CUVID_PKT_ENDOFPICTURE`, NVDEC
/// rendait l'image de l'unité précédente, et les tests de la garde d'affichage
/// étaient verts pour une autre raison que leur nom — à la reprise après une
/// perte, le vrai spectateur aurait affiché la dernière image décodée sur des
/// références perdues. Le drapeau est posé depuis la vague de correction finale,
/// et le contrat est désormais PROUVÉ sur le vrai décodeur :
/// `sky-decode/tests/aller_retour.rs`,
/// `chaque_unite_poussee_rend_sa_propre_image_sans_retard`. Qui change ce
/// comportement dans `Decodeur::decoder` doit changer cette doublure avec lui.
pub struct DecodeurFactice {
    issues: VecDeque<Issue>,
    /// Ce que rendent les appels au-delà de `issues`.
    defaut: Issue,
    unites: Vec<Vec<u8>>,
}

impl DecodeurFactice {
    /// Rend `issues` dans l'ordre, puis `defaut` indéfiniment.
    pub fn puis(issues: &[Issue], defaut: Issue) -> DecodeurFactice {
        DecodeurFactice { issues: issues.iter().copied().collect(), defaut, unites: Vec::new() }
    }

    /// Les unités d'accès poussées dans le décodeur, dans l'ordre.
    pub fn unites(&self) -> &[Vec<u8>] {
        &self.unites
    }
}

impl Decodage for DecodeurFactice {
    type Image = ImageFactice;

    fn decoder(
        &mut self,
        unite: &[u8],
        _horodatage_ms: u64,
    ) -> Result<Option<ImageFactice>, ErreurDecodeur> {
        self.unites.push(unite.to_vec());
        match self.issues.pop_front().unwrap_or(self.defaut) {
            Issue::Avalee => Ok(None),
            Issue::Image => Ok(Some(ImageFactice { largeur: 1920, hauteur: 1080 })),
            // Une variante sans donnée, pour que la doublure n'ait pas à
            // fabriquer un diagnostic qu'elle ne saurait pas rendre juste.
            Issue::Echec => Err(ErreurDecodeur::QuatreQuatreQuatreNonPris),
        }
    }
}

/// Ce que le spectateur a demandé à l'écran, dans l'ordre.
///
/// Un seul journal pour les images ET les états : c'est l'ORDRE qui porte la
/// garantie qu'on veut prouver — qu'aucune image n'est montrée avant d'être
/// digne de confiance. Deux listes séparées la perdraient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Geste {
    Etat(EtatVisionnage),
    Image { largeur: u32, hauteur: u32 },
}

/// Une fenêtre qui n'en est pas une : elle note, elle ne dessine pas.
///
/// Indispensable ici — le propriétaire travaille sur la machine des tests, et
/// une `Fenetre` réelle lui ouvrirait une fenêtre sous le nez à chaque
/// `cargo test`.
pub struct FenetreFactice {
    journal: Vec<Geste>,
    /// Ce que le prochain pompage rendra, comme la procédure de fenêtre le
    /// déposerait.
    en_attente: Vec<EvenementFenetre>,
    bascules: usize,
    /// Fait échouer toute bascule du plein écran, comme un `SetWindowPos` refusé.
    plein_ecran_refuse: bool,
    /// Passe à `true` quand la fenêtre est détruite : c'est ce que voit un test
    /// qui veut savoir si la boucle l'a bien laissée tomber.
    fermee: Rc<Cell<bool>>,
}

impl FenetreFactice {
    pub fn nouvelle() -> FenetreFactice {
        FenetreFactice {
            journal: Vec::new(),
            en_attente: Vec::new(),
            bascules: 0,
            plein_ecran_refuse: false,
            fermee: Rc::new(Cell::new(false)),
        }
    }

    /// Fait échouer toute bascule du plein écran.
    pub fn refuser_le_plein_ecran(&mut self) {
        self.plein_ecran_refuse = true;
    }

    /// Comme un clic sur la croix : le prochain pompage rend
    /// `FermetureDemandee`.
    pub fn demander_la_fermeture(&mut self) {
        self.en_attente.push(EvenementFenetre::FermetureDemandee);
    }

    /// Comme F11 : le prochain pompage rend `PleinEcranBascule`.
    pub fn appuyer_sur_f11(&mut self) {
        self.en_attente.push(EvenementFenetre::PleinEcranBascule);
    }

    /// Nombre de bascules du plein écran demandées à la fenêtre.
    pub fn bascules(&self) -> usize {
        self.bascules
    }

    /// Un témoin qui survit à la fenêtre et dit si elle a été détruite.
    pub fn temoin_de_fermeture(&self) -> Rc<Cell<bool>> {
        Rc::clone(&self.fermee)
    }

    pub fn journal(&self) -> &[Geste] {
        &self.journal
    }

    /// Le dernier état montré, ou `None` si aucun ne l'a été.
    pub fn dernier_etat(&self) -> Option<EtatVisionnage> {
        self.journal.iter().rev().find_map(|g| match g {
            Geste::Etat(etat) => Some(*etat),
            Geste::Image { .. } => None,
        })
    }

    /// Le nombre d'images déposées à l'écran.
    pub fn images_affichees(&self) -> usize {
        self.journal.iter().filter(|g| matches!(g, Geste::Image { .. })).count()
    }
}

impl Afficheur for FenetreFactice {
    fn afficher(&mut self, image: &dyn ImageAAfficher) -> anyhow::Result<()> {
        self.journal.push(Geste::Image { largeur: image.largeur(), hauteur: image.hauteur() });
        Ok(())
    }

    fn afficher_etat(&mut self, etat: EtatVisionnage) -> anyhow::Result<()> {
        self.journal.push(Geste::Etat(etat));
        Ok(())
    }

    fn evenements(&mut self) -> Vec<EvenementFenetre> {
        std::mem::take(&mut self.en_attente)
    }

    fn basculer_plein_ecran(&mut self) -> anyhow::Result<()> {
        self.bascules += 1;
        if self.plein_ecran_refuse {
            anyhow::bail!("passage en plein écran refusé");
        }
        Ok(())
    }
}

impl Drop for FenetreFactice {
    fn drop(&mut self) {
        self.fermee.set(true);
    }
}

/// Un encodeur réduit aux deux gestes de reprise : il ne compresse rien, il
/// compte. `NvencEncoder` ne se double pas — il consomme une texture Direct3D —
/// mais `Reprise` est tout ce dont la boucle d'envoi a besoin de lui ici.
pub struct RepriseFactice {
    entetes: Vec<u8>,
    images_cle_forcees: u64,
}

impl RepriseFactice {
    /// `entetes` tient la place des VPS/SPS/PPS d'Annex-B qu'un vrai encodeur
    /// rendrait ; les tests y mettent des NAL reconnaissables.
    pub fn avec_entetes(entetes: Vec<u8>) -> RepriseFactice {
        RepriseFactice { entetes, images_cle_forcees: 0 }
    }

    pub fn images_cle_forcees(&self) -> u64 {
        self.images_cle_forcees
    }
}

impl crate::hote::Reprise for RepriseFactice {
    fn entetes_de_sequence(&self) -> anyhow::Result<Vec<u8>> {
        Ok(self.entetes.clone())
    }

    fn forcer_image_cle(&mut self) {
        self.images_cle_forcees += 1;
    }
}
