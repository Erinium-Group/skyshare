//! La fenêtre native, son appareil Direct3D 11 et sa chaîne d'échange DXGI.
//!
//! Le rendu passe par une texture intermédiaire (`Cible`) recopiée dans le
//! tampon arrière à chaque présentation. Deux raisons : en modèle « flip », le
//! contenu du tampon arrière est indéfini après `Present`, donc on ne pourrait
//! jamais le relire pour le tester ; et la tâche suivante y déposera l'image
//! décodée sans toucher à la présentation.

use std::cell::RefCell;

use anyhow::{anyhow, Context};
use sky_decode::SourceImage;
use windows::core::{w, Interface, HSTRING};
use windows::Win32::Foundation::{
    GetLastError, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1RenderTarget, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE,
};
use windows::Win32::Graphics::Direct3D::D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader, ID3D11RenderTargetView,
    ID3D11ShaderResourceView, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DEFAULT, D3D11_VIEWPORT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    IDXGIDevice, IDXGIFactory2, IDXGISurface, IDXGISwapChain1, DXGI_PRESENT,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_F11;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, GetWindowLongPtrW, GetWindowPlacement, IsWindowVisible, LoadCursorW,
    PeekMessageW, RegisterClassExW, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos,
    ShowWindow, TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
    GWLP_USERDATA, GWL_STYLE, HWND_TOP, IDC_ARROW, MSG, PM_REMOVE, SIZE_MINIMIZED,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
    SW_HIDE, SW_SHOW, WINDOWPLACEMENT, WINDOW_EX_STYLE, WM_CLOSE, WM_KEYDOWN, WM_NCCREATE,
    WM_NCDESTROY, WM_SIZE, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
};

use crate::etat::{dessiner, EtatVisionnage};
use crate::interop::{ImageAAfficher, Pont};
use crate::nuanceur::{rectangle_centre, Programme};
use crate::nv12::PontNv12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvenementFenetre {
    /// L'utilisateur a fermé la fenêtre. Elle reste ouverte : c'est à l'appelant
    /// de la laisser tomber.
    FermetureDemandee,
    /// F11. Consommé par le spectateur (`sky-partage`), qui appelle
    /// [`Fenetre::basculer_plein_ecran`].
    PleinEcranBascule,
}

/// Ce que la procédure de fenêtre dépose pour `pompe_messages`.
#[derive(Default)]
struct Boite {
    evenements: Vec<EvenementFenetre>,
    /// Dernière taille de zone cliente demandée, à appliquer à la chaîne.
    taille: Option<(u32, u32)>,
}

/// La texture où l'on dessine, et les deux moyens d'y dessiner : Direct2D pour
/// les états sans image, Direct3D pour l'image elle-même.
struct Cible {
    texture: ID3D11Texture2D,
    rendu: ID2D1RenderTarget,
    vue_rendu: ID3D11RenderTargetView,
}

pub struct Fenetre {
    hwnd: HWND,
    /// Partagée avec la procédure de fenêtre par `GWLP_USERDATA`. Libérée dans
    /// `Drop`, après la destruction de la fenêtre.
    boite: *mut RefCell<Boite>,
    appareil: ID3D11Device,
    contexte: ID3D11DeviceContext,
    chaine: IDXGISwapChain1,
    fabrique_d2d: ID2D1Factory,
    format_texte: IDWriteTextFormat,
    cible: Cible,
    /// Nuanceurs et échantillonneur, compilés une fois à l'ouverture.
    programme: Programme,
    /// Les textures de plans enregistrées auprès de CUDA. Créées à la première
    /// image et refaites quand sa taille change : l'enregistrement coûte, le
    /// téléversement non.
    pont: Option<Pont>,
    /// La texture NV12 lisible par le nuanceur, où l'image Media Foundation est
    /// copiée. Créée à la première image NV12 et refaite quand sa taille change,
    /// comme `pont`.
    pont_nv12: Option<PontNv12>,
    largeur: u32,
    hauteur: u32,
    /// Pour repeindre après un redimensionnement. `None` dès qu'une image a été
    /// affichée : il n'y a alors plus d'état à repeindre, la prochaine image
    /// remplira la nouvelle taille.
    dernier_etat: Option<EtatVisionnage>,
    /// Le placement d'avant le plein écran, pour y revenir. `Some` : la fenêtre
    /// est en plein écran.
    avant_plein_ecran: Option<WINDOWPLACEMENT>,
    /// Tests seulement : l'étape du plein écran à faire échouer.
    #[cfg(test)]
    echec_simule: Option<EtapePleinEcran>,
}

/// Les deux appels du plein écran qui changent la géométrie, et qu'un test
/// peut faire échouer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EtapePleinEcran {
    Aller,
    Retour,
}

impl Fenetre {
    /// Ouvre une fenêtre visible.
    pub fn ouvrir(titre: &str, largeur: u32, hauteur: u32) -> anyhow::Result<Self> {
        Self::creer(titre, largeur, hauteur, true)
    }

    /// Crée la fenêtre sans jamais l'afficher. C'est la seule voie des tests : ils
    /// ne doivent rien montrer sur la machine de qui les lance.
    pub fn ouvrir_masquee(titre: &str, largeur: u32, hauteur: u32) -> anyhow::Result<Self> {
        Self::creer(titre, largeur, hauteur, false)
    }

    /// L'appareil Direct3D 11 de la fenêtre. Le décodeur et l'interopérabilité
    /// doivent travailler sur celui-ci : une copie de périphérique à périphérique
    /// n'existe pas entre deux appareils.
    pub fn appareil(&self) -> &ID3D11Device {
        &self.appareil
    }

    /// La taille de la zone cliente, en pixels. Ce n'est pas toujours celle
    /// demandée à l'ouverture : Windows impose une largeur minimale de fenêtre,
    /// que la barre de titre et ses boutons commandent.
    pub fn taille(&self) -> (u32, u32) {
        (self.largeur, self.hauteur)
    }

    /// Peint un des états sans image, puis présente.
    pub fn afficher_etat(&mut self, etat: EtatVisionnage) -> anyhow::Result<()> {
        self.dernier_etat = Some(etat);
        dessiner(
            &self.cible.rendu,
            &self.format_texte,
            etat,
            self.largeur,
            self.hauteur,
        )?;
        self.presenter()
    }

    /// Affiche une image décodée, puis présente.
    ///
    /// Le chemin est intégralement GPU, et il y en a deux selon la source :
    /// - 4:4:4 (NVDEC) : la surface CUDA est copiée de périphérique à
    ///   périphérique dans trois textures (voir `interop.rs`) ;
    /// - NV12 (Media Foundation) : la tranche de texture du décodeur est copiée
    ///   par Direct3D 11 dans une texture lisible (voir `nv12.rs`).
    ///
    /// Un nuanceur par source recombine les plans et les convertit en RVB, par
    /// la même matrice (spec D6). Aucun octet de pixel ne passe par la mémoire
    /// centrale — ni `ImageDecodee::copier_vers_memoire_centrale` ni
    /// `ImageMf::copier_luminance` ne sont appelées d'ici (décision D5).
    ///
    /// À appeler dès qu'une image est reçue : `cuvidDecodePicture` peut bloquer le
    /// fil appelant quand les quatre surfaces de sortie de NVDEC sont épuisées
    /// (`cuviddec.h:1036`), et une `ImageDecodee` vivante en immobilise une.
    pub fn afficher(&mut self, image: &dyn ImageAAfficher) -> anyhow::Result<()> {
        let taille = (image.largeur(), image.hauteur());
        match image.source() {
            SourceImage::Cuda444(surface) => {
                let a_jour = self
                    .pont
                    .as_ref()
                    .is_some_and(|pont| (pont.largeur(), pont.hauteur()) == taille);
                if !a_jour {
                    // Relâcher l'ancien avant d'allouer le nouveau : chacun
                    // immobilise trois textures et autant d'enregistrements CUDA.
                    self.pont = None;
                    self.pont = Some(Pont::nouveau(&self.appareil, taille.0, taille.1)?);
                }
                let pont = self
                    .pont
                    .as_mut()
                    .ok_or_else(|| anyhow!("pont d'interopérabilité absent"))?;
                pont.televerser(taille, surface)?;
                let vues = pont.vues();
                self.dessiner_image(taille, &self.programme.pixels, &vues);
            }
            SourceImage::Nv12 { texture, tranche } => {
                let a_jour = self
                    .pont_nv12
                    .as_ref()
                    .is_some_and(|pont| (pont.largeur(), pont.hauteur()) == taille);
                if !a_jour {
                    self.pont_nv12 = None;
                    self.pont_nv12 = Some(PontNv12::nouveau(&self.appareil, taille.0, taille.1)?);
                }
                let pont = self
                    .pont_nv12
                    .as_ref()
                    .ok_or_else(|| anyhow!("texture NV12 lisible absente"))?;
                pont.televerser(&self.contexte, texture, tranche)?;
                let vues = pont.vues();
                self.dessiner_image(taille, &self.programme.pixels_nv12, &vues);
            }
        }
        // Une image remplace l'état : il n'y a plus rien à repeindre après un
        // redimensionnement, la prochaine image s'en chargera à la bonne taille.
        self.dernier_etat = None;
        self.presenter()
    }

    /// Dessine dans la cible une image de `taille`, lue par le nuanceur de
    /// pixels `pixels` dans les `vues` (liées à partir de `t0`). La seule
    /// séquence de dessin d'image, pour les deux sources.
    fn dessiner_image(
        &self,
        taille: (u32, u32),
        pixels: &ID3D11PixelShader,
        vues: &[Option<ID3D11ShaderResourceView>],
    ) {
        let (x, y, largeur, hauteur) = rectangle_centre(taille, (self.largeur, self.hauteur));
        unsafe {
            let cibles = [Some(self.cible.vue_rendu.clone())];
            self.contexte.OMSetRenderTargets(Some(&cibles), None);
            // Le noir des bandes : il couvre toute la cible, puis le viewport
            // restreint le dessin au rectangle qui préserve le rapport d'image.
            self.contexte
                .ClearRenderTargetView(&self.cible.vue_rendu, &[0.0, 0.0, 0.0, 1.0]);
            self.contexte.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: x,
                TopLeftY: y,
                Width: largeur,
                Height: hauteur,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            // Aucun tampon de sommets : les quatre coins sortent de `SV_VertexID`.
            self.contexte.IASetInputLayout(None);
            self.contexte
                .IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
            self.contexte.VSSetShader(&self.programme.sommets, None);
            self.contexte.PSSetShader(pixels, None);
            self.contexte.PSSetShaderResources(0, Some(vues));
            self.contexte
                .PSSetSamplers(0, Some(&[Some(self.programme.echantillonneur.clone())]));
            self.contexte.Draw(4, 0);
            // Détacher autant de vues qu'on en a lié : la prochaine image réécrit
            // ces textures (par CUDA ou par une copie Direct3D), et Direct3D
            // refuse de prêter une ressource encore liée en lecture au nuanceur.
            // Un tableau fixe, découpé : aucune allocation par image. Trois
            // places, le plus grand nombre de vues liées (le 4:4:4).
            const DETACHEES: [Option<ID3D11ShaderResourceView>; 3] = [None, None, None];
            self.contexte
                .PSSetShaderResources(0, Some(&DETACHEES[..vues.len()]));
            self.contexte.OMSetRenderTargets(None, None);
        }
    }

    /// RÉSERVÉ AUX TESTS ET AUX MESURES : le pixel au centre de la cible, en RVB.
    ///
    /// Publique et non `pub(crate)` : le test de couleur vit dans `tests/`, donc
    /// hors du crate, et n'atteindrait pas un membre restreint au crate.
    pub fn pixel_central(&self) -> anyhow::Result<[u8; 3]> {
        let pixels = self.pixels_de_la_cible()?;
        let largeur = self.largeur as usize;
        let indice = ((self.hauteur as usize / 2) * largeur + largeur / 2) * 4;
        let pixel = pixels
            .get(indice..indice + 4)
            .ok_or_else(|| anyhow!("pixel central hors du tampon"))?;
        // La cible est en BGRA : on rend R, V, B dans cet ordre.
        Ok([pixel[2], pixel[1], pixel[0]])
    }

    /// Traite les messages en attente sans bloquer et rend les événements utiles.
    pub fn pompe_messages(&mut self) -> Vec<EvenementFenetre> {
        let mut msg = MSG::default();
        unsafe {
            while PeekMessageW(&mut msg, Some(self.hwnd), 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let (evenements, taille) = {
            let mut boite = unsafe { &*self.boite }.borrow_mut();
            (std::mem::take(&mut boite.evenements), boite.taille.take())
        };
        if let Some((largeur, hauteur)) = taille {
            if (largeur, hauteur) != (self.largeur, self.hauteur)
                && self.redimensionner(largeur, hauteur).is_ok()
            {
                // Un tampon redimensionné est noir jusqu'à la prochaine présentation :
                // on repeint l'état courant. Un échec ici n'est pas perdu, le
                // prochain `afficher_etat` rencontrera le même et le remontera.
                if let Some(etat) = self.dernier_etat {
                    let _ = self.afficher_etat(etat);
                }
            }
        }
        evenements
    }

    fn creer(titre: &str, largeur: u32, hauteur: u32, visible: bool) -> anyhow::Result<Self> {
        if largeur == 0 || hauteur == 0 {
            return Err(anyhow!("taille de fenêtre nulle : {largeur}×{hauteur}"));
        }
        // Le périphérique unique de SkyShare (adaptateur NVIDIA d'abord, API
        // vidéo, protection multi-fil) : c'est sur lui que décodent NVDEC, par
        // l'interopérabilité CUDA, et Media Foundation.
        let (appareil, contexte) = sky_decode::creer_appareil_video()?;
        let boite = Box::into_raw(Box::new(RefCell::new(Boite::default())));
        let hwnd = match creer_hwnd(titre, largeur, hauteur, boite) {
            Ok(hwnd) => hwnd,
            Err(e) => {
                // Aucune fenêtre ne détient plus la boîte.
                drop(unsafe { Box::from_raw(boite) });
                return Err(e);
            }
        };
        // `Fenetre` n'existe pas encore, donc pas de `Drop` : sur échec, on
        // détruit la fenêtre et libère la boîte à la main.
        let fenetre = match Self::assembler(hwnd, boite, appareil, contexte) {
            Ok(fenetre) => fenetre,
            Err(e) => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                    drop(Box::from_raw(boite));
                }
                return Err(e);
            }
        };
        if visible {
            unsafe {
                let _ = ShowWindow(fenetre.hwnd, SW_SHOW);
            }
        }
        Ok(fenetre)
    }

    fn assembler(
        hwnd: HWND,
        boite: *mut RefCell<Boite>,
        appareil: ID3D11Device,
        contexte: ID3D11DeviceContext,
    ) -> anyhow::Result<Self> {
        let mut client = RECT::default();
        unsafe { GetClientRect(hwnd, &mut client) }.context("taille de la zone cliente")?;
        let (largeur, hauteur) = (
            (client.right - client.left).max(0) as u32,
            (client.bottom - client.top).max(0) as u32,
        );
        let (largeur, hauteur) = if largeur == 0 || hauteur == 0 {
            (1, 1)
        } else {
            (largeur, hauteur)
        };

        let dxgi: IDXGIDevice = appareil.cast()?;
        let fabrique: IDXGIFactory2 =
            unsafe { dxgi.GetAdapter()?.GetParent() }.context("fabrique DXGI")?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: largeur,
            Height: hauteur,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            // Deux tampons et FLIP_DISCARD : décision D7, la latence prime sur
            // l'absence de déchirement. On regarde quelqu'un travailler, pas un film.
            BufferCount: 2,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            ..Default::default()
        };
        let chaine = unsafe { fabrique.CreateSwapChainForHwnd(&appareil, hwnd, &desc, None, None) }
            .context("création de la chaîne d'échange")?;

        let fabrique_d2d: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }
                .context("fabrique Direct2D")?;
        let fabrique_texte: IDWriteFactory =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
                .context("fabrique DirectWrite")?;
        let format_texte = unsafe {
            let format = fabrique_texte
                .CreateTextFormat(
                    w!("Segoe UI"),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    28.0,
                    w!("fr-FR"),
                )
                .context("format de texte")?;
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format
        };
        let cible = creer_cible(&appareil, &fabrique_d2d, largeur, hauteur)?;
        // Compilé une fois à l'ouverture, et non à la première image : une erreur
        // de nuanceur doit se voir tout de suite, pas au moment où quelqu'un
        // regarde un écran noir.
        let programme = Programme::compiler(&appareil)?;

        Ok(Self {
            hwnd,
            boite,
            appareil,
            contexte,
            chaine,
            fabrique_d2d,
            format_texte,
            cible,
            programme,
            pont: None,
            pont_nv12: None,
            largeur,
            hauteur,
            dernier_etat: None,
            avant_plein_ecran: None,
            #[cfg(test)]
            echec_simule: None,
        })
    }

    /// La fenêtre occupe-t-elle tout son écran ?
    pub fn en_plein_ecran(&self) -> bool {
        self.avant_plein_ecran.is_some()
    }

    /// Passe en plein écran, ou en revient (F11, décision D4 de la spec : la
    /// fenêtre séparée rend le plein écran possible sans rien arbitrer avec la
    /// vue web).
    ///
    /// La méthode est la classique « fenêtre sans bordure à la taille du
    /// moniteur » : on retire le cadre (`WS_OVERLAPPEDWINDOW`) et on étend la
    /// fenêtre au rectangle du moniteur qui la porte. Pas de plein écran
    /// exclusif DXGI (`SetFullscreenState`) : il change le mode d'affichage et
    /// rend l'alt-tab brutal, pour un gain de latence que le modèle « flip »
    /// obtient déjà en fenêtre sans bordure.
    ///
    /// Le changement de taille arrive par `WM_SIZE`, donc la chaîne d'échange est
    /// redimensionnée au prochain `pompe_messages` — par le même chemin qu'un
    /// redimensionnement à la souris.
    ///
    /// Elle ne MONTRE jamais une fenêtre cachée : aucun drapeau
    /// `SWP_SHOWWINDOW`, et le placement restauré garde l'état caché. C'est ce
    /// qui permet de l'éprouver sur une fenêtre masquée sans rien afficher.
    ///
    /// UN ÉCHEC LAISSE LA FENÊTRE DANS SON ÉTAT PRÉCÉDENT (ronde de correction 1
    /// de la tâche 10). Les deux appels qui changent la géométrie
    /// (`SetWindowPos` à l'aller, `SetWindowPlacement` au retour) viennent après
    /// un changement de style : s'ils échouent, le style est remis et l'état
    /// mémorisé aussi, pour qu'un F11 suivant reparte d'un état cohérent. Sans
    /// cela, un retour raté laissait une fenêtre sans bordure et sans plus aucun
    /// moyen d'en sortir.
    pub fn basculer_plein_ecran(&mut self) -> anyhow::Result<()> {
        unsafe {
            match self.avant_plein_ecran {
                None => {
                    let mut placement = WINDOWPLACEMENT {
                        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                        ..Default::default()
                    };
                    GetWindowPlacement(self.hwnd, &mut placement)
                        .context("placement de la fenêtre")?;
                    let moniteur = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
                    let mut info = MONITORINFO {
                        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                        ..Default::default()
                    };
                    if !GetMonitorInfoW(moniteur, &mut info).as_bool() {
                        return Err(anyhow!("moniteur de la fenêtre introuvable"));
                    }
                    let style = GetWindowLongPtrW(self.hwnd, GWL_STYLE);
                    SetWindowLongPtrW(
                        self.hwnd,
                        GWL_STYLE,
                        style & !(WS_OVERLAPPEDWINDOW.0 as isize),
                    );
                    let ecran = info.rcMonitor;
                    let pose = self.etape(EtapePleinEcran::Aller, || {
                        SetWindowPos(
                            self.hwnd,
                            Some(HWND_TOP),
                            ecran.left,
                            ecran.top,
                            ecran.right - ecran.left,
                            ecran.bottom - ecran.top,
                            SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
                        )
                    });
                    if let Err(e) = pose {
                        // Le cadre revient ; la géométrie n'a pas bougé.
                        SetWindowLongPtrW(self.hwnd, GWL_STYLE, style);
                        self.redessiner_le_cadre();
                        return Err(anyhow::Error::new(e).context("passage en plein écran"));
                    }
                    self.avant_plein_ecran = Some(placement);
                }
                Some(avant) => {
                    let mut placement = avant;
                    let style = GetWindowLongPtrW(self.hwnd, GWL_STYLE);
                    SetWindowLongPtrW(self.hwnd, GWL_STYLE, style | WS_OVERLAPPEDWINDOW.0 as isize);
                    // Le placement relu avant le plein écran porte un état
                    // d'affichage ; pour une fenêtre cachée, on n'en restaure
                    // que la géométrie. `SetWindowPlacement` avec un état
                    // « normal » la MONTRERAIT.
                    if !IsWindowVisible(self.hwnd).as_bool() {
                        placement.showCmd = SW_HIDE.0 as u32;
                    }
                    let pose = self.etape(EtapePleinEcran::Retour, || {
                        SetWindowPlacement(self.hwnd, &placement)
                    });
                    if let Err(e) = pose {
                        // Toujours en plein écran : sans bordure, et le
                        // placement d'avant GARDÉ pour le prochain F11.
                        SetWindowLongPtrW(self.hwnd, GWL_STYLE, style);
                        self.redessiner_le_cadre();
                        return Err(anyhow::Error::new(e).context("retour du plein écran"));
                    }
                    self.avant_plein_ecran = None;
                    self.redessiner_le_cadre();
                }
            }
        }
        Ok(())
    }

    /// Fait prendre en compte un changement de style au cadre de la fenêtre.
    /// Sans géométrie : rien ne bouge, rien ne s'affiche. Un échec n'est pas
    /// rattrapable et ne change rien d'utile — le cadre se redessinera au
    /// prochain changement de taille —, il est donc ignoré.
    fn redessiner_le_cadre(&self) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE
                    | SWP_NOSIZE
                    | SWP_NOZORDER
                    | SWP_NOOWNERZORDER
                    | SWP_NOACTIVATE
                    | SWP_FRAMECHANGED,
            );
        }
    }

    /// Exécute une étape du plein écran — sauf si un test a demandé qu'elle
    /// échoue, auquel cas l'appel n'a PAS lieu et l'échec est rendu à sa place,
    /// comme le ferait un appel Win32 refusé. C'est le seul moyen d'éprouver le
    /// retour arrière : on ne sait pas faire échouer `SetWindowPos` à la demande.
    fn etape(
        &self,
        etape: EtapePleinEcran,
        appel: impl FnOnce() -> windows::core::Result<()>,
    ) -> windows::core::Result<()> {
        #[cfg(test)]
        if self.echec_simule == Some(etape) {
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_FAIL,
            ));
        }
        #[cfg(not(test))]
        let _ = etape;
        appel()
    }

    /// Recopie la cible dans le tampon arrière et présente.
    fn presenter(&self) -> anyhow::Result<()> {
        unsafe {
            let arriere: ID3D11Texture2D = self.chaine.GetBuffer(0).context("tampon arrière")?;
            self.contexte.CopyResource(&arriere, &self.cible.texture);
            // Intervalle de synchronisation 0 : on ne guette pas la synchro
            // verticale, la latence prime (D7).
            self.chaine
                .Present(0, DXGI_PRESENT(0))
                .ok()
                .context("présentation")?;
        }
        Ok(())
    }

    /// Recrée la chaîne et la cible à la nouvelle taille.
    ///
    /// **Un échec ne laisse PAS l'état intact**, contrairement à ce que ce
    /// commentaire affirmait (mineur 20 de la tâche 3). L'ancienne cible n'est
    /// remplacée qu'une fois la nouvelle construite, mais `ResizeBuffers` a déjà
    /// eu lieu : si `creer_cible` échoue, la chaîne est à la NOUVELLE taille et
    /// la cible à l'ancienne, `largeur`/`hauteur` gardant l'ancienne. Le
    /// `CopyResource` de `presenter` relie alors deux textures de tailles
    /// différentes, ce que la documentation de Direct3D 11 interdit (dimensions
    /// identiques exigées) ; la revue finale en déduit qu'il ne copie rien —
    /// jamais provoqué ici. L'image présentée ne changerait alors plus jusqu'au
    /// redimensionnement suivant réussi. Rare (`creer_cible` n'échoue que sur une mémoire vidéo
    /// épuisée ou un appareil perdu) et non corrigé à ce jalon.
    fn redimensionner(&mut self, largeur: u32, hauteur: u32) -> anyhow::Result<()> {
        // Aucune référence au tampon arrière n'est conservée entre deux
        // présentations, donc `ResizeBuffers` n'est pas gêné.
        unsafe {
            self.chaine.ResizeBuffers(
                0,
                largeur,
                hauteur,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_SWAP_CHAIN_FLAG(0),
            )
        }
        .context("redimensionnement de la chaîne")?;
        self.cible = creer_cible(&self.appareil, &self.fabrique_d2d, largeur, hauteur)?;
        self.largeur = largeur;
        self.hauteur = hauteur;
        Ok(())
    }

    /// Somme de contrôle (FNV-1a 64 bits) des pixels de la cible. Sert aux
    /// tests, qui comparent des rendus sans les afficher.
    ///
    /// Ce que ça prouve : ce que Direct2D a réellement dessiné (fond et texte).
    /// Ce que ça ne prouve pas : la recopie vers le tampon arrière ni la
    /// présentation. La cible est la texture intermédiaire, pas le tampon
    /// présenté, dont le contenu est indéfini après `Present` en modèle flip.
    #[cfg(test)]
    pub(crate) fn empreinte_du_tampon(&self) -> anyhow::Result<u64> {
        Ok(self
            .pixels_de_la_cible()?
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |somme, octet| {
                (somme ^ u64::from(*octet)).wrapping_mul(0x0000_0100_0000_01b3)
            }))
    }

    /// RÉSERVÉ AUX TESTS ET AUX MESURES : les pixels de la cible (BGRA, lignes
    /// contiguës), lus côté processeur.
    ///
    /// Publique pour la même raison que [`Fenetre::pixel_central`] : le test de
    /// couleur vit hors du crate. Ce n'est pas une brèche dans la décision D5 —
    /// elle porte sur le chemin normal d'affichage, qui n'appelle jamais ceci.
    pub fn pixels_de_la_cible(&self) -> anyhow::Result<Vec<u8>> {
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_USAGE_STAGING,
        };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { self.cible.texture.GetDesc(&mut desc) };
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        let mut lisible = None;
        unsafe {
            self.appareil
                .CreateTexture2D(&desc, None, Some(&mut lisible))
        }
        .context("texture de lecture")?;
        let lisible = lisible.ok_or_else(|| anyhow!("texture de lecture nulle"))?;
        unsafe {
            self.contexte.CopyResource(&lisible, &self.cible.texture);
            let mut lecture = D3D11_MAPPED_SUBRESOURCE::default();
            self.contexte
                .Map(&lisible, 0, D3D11_MAP_READ, 0, Some(&mut lecture))
                .context("lecture de la texture")?;
            let mut pixels = Vec::with_capacity(desc.Width as usize * desc.Height as usize * 4);
            for ligne in 0..desc.Height as usize {
                let debut = (lecture.pData as *const u8).add(ligne * lecture.RowPitch as usize);
                pixels
                    .extend_from_slice(std::slice::from_raw_parts(debut, desc.Width as usize * 4));
            }
            self.contexte.Unmap(&lisible, 0);
            Ok(pixels)
        }
    }
}

impl Drop for Fenetre {
    fn drop(&mut self) {
        unsafe {
            // La destruction envoie WM_NCDESTROY, qui retire la boîte de la
            // fenêtre : on peut ensuite la libérer sans risque.
            let _ = DestroyWindow(self.hwnd);
            drop(Box::from_raw(self.boite));
        }
    }
}

fn creer_cible(
    appareil: &ID3D11Device,
    fabrique_d2d: &ID2D1Factory,
    largeur: u32,
    hauteur: u32,
) -> anyhow::Result<Cible> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: largeur,
        Height: hauteur,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
        ..Default::default()
    };
    let mut texture = None;
    unsafe { appareil.CreateTexture2D(&desc, None, Some(&mut texture)) }
        .context("création de la cible de rendu")?;
    let texture = texture.ok_or_else(|| anyhow!("cible de rendu nulle"))?;
    let surface: IDXGISurface = texture.cast()?;
    let proprietes = D2D1_RENDER_TARGET_PROPERTIES {
        r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        usage: D2D1_RENDER_TARGET_USAGE_NONE,
        minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
    };
    let rendu = unsafe { fabrique_d2d.CreateDxgiSurfaceRenderTarget(&surface, &proprietes) }
        .context("cible Direct2D sur la texture")?;
    let mut vue_rendu = None;
    unsafe { appareil.CreateRenderTargetView(&texture, None, Some(&mut vue_rendu)) }
        .context("vue de rendu Direct3D sur la texture")?;
    let vue_rendu = vue_rendu.ok_or_else(|| anyhow!("vue de rendu nulle"))?;
    Ok(Cible {
        texture,
        rendu,
        vue_rendu,
    })
}

fn creer_hwnd(
    titre: &str,
    largeur: u32,
    hauteur: u32,
    boite: *mut RefCell<Boite>,
) -> anyhow::Result<HWND> {
    let classe = w!("SkyShareRendu");
    unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let description = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: classe,
            ..Default::default()
        };
        // La classe est enregistrée une fois par processus : les ouvertures
        // suivantes rencontrent « déjà existante », qui n'est pas une erreur.
        if RegisterClassExW(&description) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(anyhow!(
                "enregistrement de la classe de fenêtre : {:?}",
                GetLastError()
            ));
        }
        // On veut une zone cliente de la taille demandée : on agrandit d'autant
        // que les bordures et la barre de titre.
        let mut cadre = RECT {
            left: 0,
            top: 0,
            right: largeur as i32,
            bottom: hauteur as i32,
        };
        AdjustWindowRectEx(&mut cadre, WS_OVERLAPPEDWINDOW, false, WINDOW_EX_STYLE(0))
            .context("dimensionnement de la fenêtre")?;
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            classe,
            &HSTRING::from(titre),
            // Redimensionnable, jamais affichée ici : `ouvrir` s'en charge.
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            cadre.right - cadre.left,
            cadre.bottom - cadre.top,
            None,
            None,
            Some(instance),
            Some(boite as *const _),
        )
    }
    .context("création de la fenêtre")
}

/// Procédure de fenêtre. Elle ne fait que déposer dans la `Boite` ce qu'elle
/// voit passer : c'est `pompe_messages` qui en tire les événements.
unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let creation = lparam.0 as *const CREATESTRUCTW;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*creation).lpCreateParams as isize);
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        let boite = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<Boite>;
        if let Some(boite) = boite.as_ref() {
            match message {
                WM_CLOSE => {
                    boite
                        .borrow_mut()
                        .evenements
                        .push(EvenementFenetre::FermetureDemandee);
                    // On ne détruit pas : la fenêtre est à l'appelant.
                    return LRESULT(0);
                }
                WM_KEYDOWN if wparam.0 == usize::from(VK_F11.0) => {
                    // Bit 30 : la touche était déjà enfoncée (répétition).
                    if lparam.0 & (1 << 30) == 0 {
                        boite
                            .borrow_mut()
                            .evenements
                            .push(EvenementFenetre::PleinEcranBascule);
                    }
                    return LRESULT(0);
                }
                WM_SIZE if wparam.0 != SIZE_MINIMIZED as usize => {
                    let largeur = (lparam.0 & 0xffff) as u32;
                    let hauteur = ((lparam.0 >> 16) & 0xffff) as u32;
                    if largeur > 0 && hauteur > 0 {
                        boite.borrow_mut().taille = Some((largeur, hauteur));
                    }
                }
                WM_NCDESTROY => {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                _ => {}
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        IsWindowVisible, SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
    };

    /// Ces tests échouent bruyamment quand la fenêtre ne s'ouvre pas : le projet
    /// est Windows seulement, et un test de fenêtrage qui se tairait ne prouverait
    /// rien (il laisserait passer une vraie régression).
    fn ouvrir_pour_test(largeur: u32, hauteur: u32) -> Fenetre {
        Fenetre::ouvrir_masquee("test", largeur, hauteur)
            .expect("la fenêtre masquée doit s'ouvrir (Direct3D 11 requis)")
    }

    /// Demande à Windows une zone cliente de cette taille, comme le ferait
    /// l'utilisateur en tirant un bord : la fenêtre reçoit un vrai `WM_SIZE`.
    fn redimensionner_le_client(fenetre: &Fenetre, largeur: i32, hauteur: i32) {
        let mut cadre = RECT {
            left: 0,
            top: 0,
            right: largeur,
            bottom: hauteur,
        };
        unsafe {
            AdjustWindowRectEx(&mut cadre, WS_OVERLAPPEDWINDOW, false, WINDOW_EX_STYLE(0))
                .expect("cadre");
            SetWindowPos(
                fenetre.hwnd,
                None,
                0,
                0,
                cadre.right - cadre.left,
                cadre.bottom - cadre.top,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .expect("SetWindowPos");
        }
    }

    /// Constat de la revue de la tâche 4 : la fenêtre doit travailler sur LE
    /// périphérique de `sky_decode::creer_appareil_video` — API vidéo et
    /// protection multi-fil, que le décodage Media Foundation exige sur le même
    /// périphérique que l'affichage (sans quoi la copie NV12 n'aurait rien à
    /// lire). Le test de `creer_appareil_video` vérifie la fonction ; celui-ci
    /// vérifie que la fenêtre l'appelle.
    ///
    /// Neutralisations mesurées (tâche 6) : une `Fenetre` qui recrée son propre
    /// périphérique par `D3D11CreateDevice` (adaptateur par défaut, sans
    /// protection multi-fil) — (1) BGRA seul : ce test rougit sur les drapeaux
    /// (0x20) ; (2) BGRA et VIDEO_SUPPORT : il rougit sur la protection. Seul ce
    /// test rougit dans les deux cas.
    #[test]
    fn la_fenetre_travaille_sur_le_peripherique_video_protege() {
        use windows::Win32::Graphics::Direct3D10::ID3D10Multithread;
        use windows::Win32::Graphics::Direct3D11::D3D11_CREATE_DEVICE_VIDEO_SUPPORT;

        let fenetre = ouvrir_pour_test(320, 200);
        let drapeaux = unsafe { fenetre.appareil().GetCreationFlags() };
        assert_ne!(
            drapeaux & D3D11_CREATE_DEVICE_VIDEO_SUPPORT.0,
            0,
            "le périphérique de la fenêtre n'a pas D3D11_CREATE_DEVICE_VIDEO_SUPPORT \
             (drapeaux 0x{drapeaux:X}) : ce n'est pas celui de creer_appareil_video"
        );
        let multi: ID3D10Multithread = fenetre
            .appareil()
            .cast()
            .expect("ID3D10Multithread");
        assert!(
            unsafe { multi.GetMultithreadProtected() }.as_bool(),
            "le périphérique de la fenêtre n'est pas protégé contre les accès concurrents"
        );
    }

    #[test]
    fn la_fenetre_masquee_n_est_pas_visible() {
        let fenetre = ouvrir_pour_test(320, 200);
        assert!(!unsafe { IsWindowVisible(fenetre.hwnd) }.as_bool());
    }

    /// Le redimensionnement recrée la chaîne et la cible : c'est le chemin qui
    /// casserait le plus discrètement quand l'image décodée y sera déposée.
    #[test]
    fn le_redimensionnement_recree_la_chaine_et_repeint_l_etat() {
        let mut fenetre = ouvrir_pour_test(320, 200);
        fenetre
            .afficher_etat(EtatVisionnage::EnAttente)
            .expect("rendu initial");
        let avant = fenetre.pixels_de_la_cible().expect("lecture avant");

        redimensionner_le_client(&fenetre, 480, 300);
        let evenements = fenetre.pompe_messages();
        assert!(evenements.is_empty(), "aucun événement attendu");

        assert_eq!(
            (fenetre.largeur, fenetre.hauteur),
            (480, 300),
            "WM_SIZE n'a pas été appliqué"
        );
        let desc = unsafe { fenetre.chaine.GetDesc1() }.expect("description de la chaîne");
        assert_eq!(
            (desc.Width, desc.Height),
            (480, 300),
            "chaîne non redimensionnée"
        );
        let apres = fenetre.pixels_de_la_cible().expect("lecture après");
        assert_eq!(
            apres.len(),
            480 * 300 * 4,
            "la cible n'a pas la nouvelle taille"
        );
        assert_eq!(
            apres[..4],
            avant[..4],
            "l'état courant n'a pas été repeint à la nouvelle taille"
        );

        // Et un rendu suivant, d'un autre état, réussit à la nouvelle taille.
        fenetre
            .afficher_etat(EtatVisionnage::ConnexionPerdue)
            .expect("rendu après redimensionnement");
        let dernier = fenetre.pixels_de_la_cible().expect("lecture finale");
        assert_eq!(dernier.len(), 480 * 300 * 4);
        // Le tampon entier, pas un pixel de fond : ce test vérifie le rendu après
        // redimensionnement, la distinction des fonds a son propre test.
        assert_ne!(dernier, apres, "le nouvel état n'a pas été peint");
    }

    /// F11 aller-retour, sur une fenêtre masquée : la zone cliente prend la
    /// taille du moniteur, la chaîne d'échange suit, puis tout revient — et
    /// rien n'est jamais montré sur la machine de qui lance les tests.
    ///
    /// Neutralisations mesurées : (1) ne pas retirer `WS_OVERLAPPEDWINDOW` — la
    /// zone cliente reste amputée du cadre (2544×1401 contre 2560×1440 sur la
    /// machine de développement) ; (2) ne pas restaurer le placement — la
    /// taille finale reste celle du moniteur.
    ///
    /// NON NEUTRALISÉ, exprès : retirer le forçage `SW_HIDE` du retour. Si la
    /// garde compte, cette neutralisation MONTRE une fenêtre sur l'écran de qui
    /// lance les tests — exactement ce que les assertions de visibilité
    /// ci-dessous sont là pour empêcher. Elles restent comme filet, non prouvées.
    #[test]
    fn le_plein_ecran_couvre_le_moniteur_puis_rend_la_taille_d_avant() {
        let mut fenetre = ouvrir_pour_test(320, 200);
        fenetre
            .afficher_etat(EtatVisionnage::EnAttente)
            .expect("rendu initial");
        let avant = fenetre.taille();

        let moniteur = unsafe {
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            assert!(GetMonitorInfoW(
                MonitorFromWindow(fenetre.hwnd, MONITOR_DEFAULTTONEAREST),
                &mut info
            )
            .as_bool());
            info.rcMonitor
        };
        let ecran = (
            (moniteur.right - moniteur.left) as u32,
            (moniteur.bottom - moniteur.top) as u32,
        );

        fenetre.basculer_plein_ecran().expect("plein écran");
        assert!(fenetre.en_plein_ecran());
        let _ = fenetre.pompe_messages();
        assert_eq!(
            fenetre.taille(),
            ecran,
            "la zone cliente ne couvre pas le moniteur"
        );
        let desc = unsafe { fenetre.chaine.GetDesc1() }.expect("description de la chaîne");
        assert_eq!(
            (desc.Width, desc.Height),
            ecran,
            "la chaîne d'échange n'a pas suivi"
        );
        assert!(
            !unsafe { IsWindowVisible(fenetre.hwnd) }.as_bool(),
            "le plein écran a montré une fenêtre masquée"
        );

        fenetre.basculer_plein_ecran().expect("retour");
        assert!(!fenetre.en_plein_ecran());
        let _ = fenetre.pompe_messages();
        assert_eq!(
            fenetre.taille(),
            avant,
            "la taille d'avant n'est pas rendue"
        );
        assert!(
            !unsafe { IsWindowVisible(fenetre.hwnd) }.as_bool(),
            "le retour du plein écran a montré une fenêtre masquée"
        );
    }

    /// Le style de la fenêtre porte-t-il le cadre ?
    fn a_son_cadre(fenetre: &Fenetre) -> bool {
        let style = unsafe { GetWindowLongPtrW(fenetre.hwnd, GWL_STYLE) };
        let cadre = WS_OVERLAPPEDWINDOW.0 as isize;
        style & cadre == cadre
    }

    /// Ronde de correction 1, I-2 : un basculement raté laisse la fenêtre dans
    /// son état PRÉCÉDENT, à l'aller comme au retour — et un F11 suivant
    /// fonctionne encore. Fenêtre masquée ; l'échec est simulé AU LIEU de
    /// l'appel Win32 (`Fenetre::etape`).
    ///
    /// Neutralisations : (1) ne pas remettre le style quand l'aller échoue —
    /// la fenêtre perd son cadre, ce test rougit ; (2) oublier le placement
    /// mémorisé quand le retour échoue — la fenêtre se croit sortie du plein
    /// écran, ce test rougit.
    #[test]
    fn un_basculement_rate_laisse_la_fenetre_dans_son_etat_precedent() {
        let mut fenetre = ouvrir_pour_test(320, 200);
        let avant = fenetre.taille();

        // Aller raté : fenêtre normale, cadre intact, taille inchangée.
        fenetre.echec_simule = Some(EtapePleinEcran::Aller);
        assert!(fenetre.basculer_plein_ecran().is_err());
        let _ = fenetre.pompe_messages();
        assert!(
            !fenetre.en_plein_ecran(),
            "un aller raté ne compte pas comme plein écran"
        );
        assert!(
            a_son_cadre(&fenetre),
            "un aller raté ne doit pas laisser une fenêtre sans cadre"
        );
        assert_eq!(fenetre.taille(), avant);

        // Aller réussi, puis retour raté : toujours en plein écran, sans cadre.
        fenetre.echec_simule = None;
        fenetre.basculer_plein_ecran().expect("plein écran");
        let _ = fenetre.pompe_messages();
        let ecran = fenetre.taille();
        fenetre.echec_simule = Some(EtapePleinEcran::Retour);
        assert!(fenetre.basculer_plein_ecran().is_err());
        let _ = fenetre.pompe_messages();
        assert!(
            fenetre.en_plein_ecran(),
            "un retour raté laisse la fenêtre en plein écran"
        );
        assert!(
            !a_son_cadre(&fenetre),
            "en plein écran, la fenêtre reste sans cadre"
        );
        assert_eq!(fenetre.taille(), ecran);

        // Et le F11 suivant ramène bien la taille d'avant : rien n'a été perdu.
        fenetre.echec_simule = None;
        fenetre.basculer_plein_ecran().expect("retour");
        let _ = fenetre.pompe_messages();
        assert!(!fenetre.en_plein_ecran());
        assert!(a_son_cadre(&fenetre));
        assert_eq!(fenetre.taille(), avant);
        assert!(!unsafe { IsWindowVisible(fenetre.hwnd) }.as_bool());
    }
}
