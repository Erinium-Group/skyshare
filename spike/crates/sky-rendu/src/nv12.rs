//! Le chemin NV12 : l'image que Media Foundation décode, recopiée dans une
//! texture que le nuanceur peut lire. Aucune interopérabilité CUDA ici — la
//! copie est un `CopySubresourceRegion` de Direct3D 11, de texture à texture, sur
//! le périphérique unique (`sky_decode::creer_appareil_video`), donc sans
//! détour par la mémoire centrale.

use anyhow::{anyhow, Context};
use windows::Win32::Graphics::Direct3D::D3D_SRV_DIMENSION_TEXTURE2D;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView, ID3D11Texture2D,
    D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_SHADER_RESOURCE_VIEW_DESC,
    D3D11_SHADER_RESOURCE_VIEW_DESC_0, D3D11_TEX2D_SRV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_NV12, DXGI_FORMAT_R8G8_UNORM, DXGI_FORMAT_R8_UNORM, DXGI_SAMPLE_DESC,
};

/// La copie de l'image Media Foundation dans une texture que le nuanceur
/// peut lire. La texture du décodeur est une tranche de tableau liée
/// `D3D11_BIND_DECODER` seulement (mesuré, spec §2) : on ne peut pas
/// l'échantillonner. La copie se fait à la taille d'AFFICHAGE, ce qui écarte
/// les lignes de remplissage (1080 contre 1088).
pub(crate) struct PontNv12 {
    texture: ID3D11Texture2D,
    vue_y: ID3D11ShaderResourceView,
    vue_uv: ID3D11ShaderResourceView,
    largeur: u32,
    hauteur: u32,
}

impl PontNv12 {
    pub(crate) fn nouveau(
        appareil: &ID3D11Device,
        largeur: u32,
        hauteur: u32,
    ) -> anyhow::Result<Self> {
        // NV12 sous-échantillonne la chrominance d'un facteur 2 dans les deux
        // sens, et la documentation de `DXGI_FORMAT_NV12` exige une largeur et
        // une hauteur paires (repris de la documentation, non éprouvé ici). Le
        // refus est dit ici plutôt que laissé à un code d'erreur de Direct3D.
        if largeur == 0 || hauteur == 0 || !largeur.is_multiple_of(2) || !hauteur.is_multiple_of(2)
        {
            return Err(anyhow!(
                "taille d'image NV12 nulle ou impaire : {largeur}×{hauteur}"
            ));
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: largeur,
            Height: hauteur,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_NV12,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            ..Default::default()
        };
        let mut texture = None;
        unsafe { appareil.CreateTexture2D(&desc, None, Some(&mut texture)) }
            .context("création de la texture NV12 lisible")?;
        let texture = texture.ok_or_else(|| anyhow!("texture NV12 nulle"))?;
        // Deux vues sur la même texture : `R8_UNORM` voit le plan Y à pleine
        // résolution, `R8G8_UNORM` le plan UV entrelacé à demi-résolution. Le
        // nuanceur les échantillonne aux mêmes coordonnées normalisées, ce qui
        // apparie chaque pixel à sa chrominance.
        let vue_y = creer_vue(appareil, &texture, DXGI_FORMAT_R8_UNORM).context("vue du plan Y")?;
        let vue_uv =
            creer_vue(appareil, &texture, DXGI_FORMAT_R8G8_UNORM).context("vue du plan UV")?;
        Ok(Self {
            texture,
            vue_y,
            vue_uv,
            largeur,
            hauteur,
        })
    }

    pub(crate) fn largeur(&self) -> u32 {
        self.largeur
    }

    pub(crate) fn hauteur(&self) -> u32 {
        self.hauteur
    }

    /// Copie la tranche `tranche` de `texture` (celle du décodeur) dans la
    /// texture lisible, à la taille d'affichage.
    ///
    /// Refuse une source trop petite ou une tranche hors du tableau : Direct3D 11
    /// IGNORE SANS ERREUR une copie dont la boîte déborde de la source, et
    /// l'image précédente reste affichée sans que rien ne le dise (mesuré,
    /// tâche 6 : ce refus et la recréation de la texture retirés ensemble,
    /// `tests/couleur.rs::le_pont_nv12_suit_la_taille_de_l_image` affiche la
    /// couleur de l'image d'avant). Seul, ce refus n'est atteint par aucun test :
    /// `afficher` recrée la texture à chaque changement de taille, il ne sert
    /// que si cette recréation casse. Les deux refus ont chacun leur test
    /// unitaire ci-dessous (ronde de correction 1).
    pub(crate) fn televerser(
        &self,
        contexte: &ID3D11DeviceContext,
        texture: &ID3D11Texture2D,
        tranche: u32,
    ) -> anyhow::Result<()> {
        let mut source = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut source) };
        if self.largeur > source.Width || self.hauteur > source.Height {
            return Err(anyhow!(
                "copie de {}×{} plus grande que la texture source {}×{}",
                self.largeur,
                self.hauteur,
                source.Width,
                source.Height
            ));
        }
        if tranche >= source.ArraySize {
            return Err(anyhow!(
                "tranche {tranche} hors d'un tableau de {}",
                source.ArraySize
            ));
        }
        // Indice de sous-ressource du niveau 0 de la tranche
        // (`D3D11CalcSubresource`) : il vaut `tranche` quand la texture n'a
        // qu'un niveau, ce qui est le cas des textures du décodeur relevées ;
        // le calcul complet ne coûte rien et ne suppose rien.
        let sous_ressource = tranche * source.MipLevels;
        unsafe {
            contexte.CopySubresourceRegion(
                &self.texture,
                0,
                0,
                0,
                0,
                texture,
                sous_ressource,
                Some(&D3D11_BOX {
                    left: 0,
                    top: 0,
                    front: 0,
                    right: self.largeur,
                    bottom: self.hauteur,
                    back: 1,
                }),
            );
        }
        Ok(())
    }

    /// Les deux vues à donner au nuanceur NV12, dans l'ordre Y, UV.
    pub(crate) fn vues(&self) -> [Option<ID3D11ShaderResourceView>; 2] {
        [Some(self.vue_y.clone()), Some(self.vue_uv.clone())]
    }
}

/// Une vue de lecture d'un plan de la texture NV12, désigné par son format.
fn creer_vue(
    appareil: &ID3D11Device,
    texture: &ID3D11Texture2D,
    format: DXGI_FORMAT,
) -> anyhow::Result<ID3D11ShaderResourceView> {
    let desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
        Format: format,
        ViewDimension: D3D_SRV_DIMENSION_TEXTURE2D,
        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
            Texture2D: D3D11_TEX2D_SRV {
                MostDetailedMip: 0,
                MipLevels: 1,
            },
        },
    };
    let mut vue = None;
    unsafe { appareil.CreateShaderResourceView(texture, Some(&desc), Some(&mut vue)) }
        .context("CreateShaderResourceView")?;
    vue.ok_or_else(|| anyhow!("vue NV12 nulle"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_de_test::image_de_test_nv12;
    use crate::interop::ImageAAfficher;
    use sky_decode::SourceImage;

    /// Le périphérique de SkyShare et une image NV12 unie de test, sur lui.
    /// Panique sans périphérique Direct3D 11 matériel : ces tests ne se
    /// sautent pas.
    fn texture_de_test(
        appareil: &ID3D11Device,
        largeur: u32,
        hauteur: u32,
    ) -> crate::image_de_test::ImageNv12DeTest {
        image_de_test_nv12(appareil, largeur, hauteur, [76, 85, 255]).expect("image NV12 de test")
    }

    fn texture_de(image: &crate::image_de_test::ImageNv12DeTest) -> ID3D11Texture2D {
        match image.source() {
            SourceImage::Nv12 { texture, .. } => texture.clone(),
            SourceImage::Cuda444(_) => panic!("une image NV12 de test doit être NV12"),
        }
    }

    /// Une copie plus grande que la source est refusée : Direct3D 11 l'ignorerait
    /// sans erreur, et l'image précédente resterait affichée (mesuré, N2b).
    /// Neutralisation : le refus de taille retiré — ce test rougit seul.
    #[test]
    fn une_source_plus_petite_que_la_copie_est_refusee() {
        let (appareil, contexte) =
            sky_decode::creer_appareil_video().expect("périphérique Direct3D 11 matériel");
        let pont = PontNv12::nouveau(&appareil, 128, 64).expect("texture NV12 lisible");
        let image = texture_de_test(&appareil, 64, 64);
        let erreur = pont
            .televerser(&contexte, &texture_de(&image), 0)
            .expect_err("une copie 128×64 depuis une source 64×64 doit être refusée");
        assert!(
            erreur
                .to_string()
                .contains("plus grande que la texture source"),
            "refus attendu pour la taille, obtenu : {erreur:#}"
        );
    }

    /// Une tranche hors du tableau source est refusée, au lieu d'une copie
    /// d'une sous-ressource qui n'existe pas.
    /// Neutralisation : le refus de tranche retiré — ce test rougit seul.
    #[test]
    fn une_tranche_hors_du_tableau_est_refusee() {
        let (appareil, contexte) =
            sky_decode::creer_appareil_video().expect("périphérique Direct3D 11 matériel");
        let pont = PontNv12::nouveau(&appareil, 64, 64).expect("texture NV12 lisible");
        let image = texture_de_test(&appareil, 64, 64);
        let texture = texture_de(&image);
        pont.televerser(&contexte, &texture, 0)
            .expect("la tranche 0 d'une texture d'une tranche est valide");
        let erreur = pont
            .televerser(&contexte, &texture, 1)
            .expect_err("la tranche 1 d'une texture d'une seule tranche doit être refusée");
        assert!(
            erreur.to_string().contains("hors d'un tableau de 1"),
            "refus attendu pour la tranche, obtenu : {erreur:#}"
        );
    }
}
