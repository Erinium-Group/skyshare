//! Ce que l'application apprend de la machine.

use sky_encode::{pick_best, Codec, EncodeError, EncoderCaps};

/// Nom d'appareil quand celui de la machine est refusé par le site (spec §10).
/// MÊME valeur que le repli de `sky_compte` (`nom_par_defaut`, annuaire.rs) :
/// un seul produit, un seul nom générique — arbitrage du contrôleur. Changer
/// l'un sans l'autre ferait apparaître deux noms pour le même cas.
pub const NOM_D_APPAREIL_DE_REPLI: &str = "Appareil SkyShare";

/// Le nom de la machine s'il passe la règle que `sky-compte` applique déjà
/// (celle de `POST /api/sky/devices`), sinon « Appareil SkyShare ».
pub fn nom_d_appareil(nom_machine: Option<&str>) -> String {
    nom_machine
        .filter(|nom| sky_compte::nom_appareil_valide(nom))
        .map(str::to_string)
        .unwrap_or_else(|| NOM_D_APPAREIL_DE_REPLI.to_string())
}

/// Le codec de partage de cette machine, ou `None` sans carte NVIDIA — même
/// choix que `sky-probe hw` (`pick_best(&caps, true)` : netteté du texte).
///
/// `probe_hardware` ouvre `nvcuda.dll` par `cudarc` 0.16.6, qui PANIQUE quand
/// la bibliothèque est absente (au lieu de rendre une erreur) : sans
/// `catch_unwind`, une machine sans pilote NVIDIA fermerait l'application au
/// lancement. Une telle machine peut encore regarder (spec §4).
pub fn detecter_nvenc(
    sonde: impl FnOnce() -> Result<EncoderCaps, EncodeError> + std::panic::UnwindSafe,
) -> Option<Codec> {
    match std::panic::catch_unwind(sonde) {
        Ok(Ok(caps)) => pick_best(&caps, true),
        Ok(Err(_)) | Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_sonde_qui_panique_rend_aucune_carte() {
        // `cudarc` panique quand nvcuda.dll est absente. Neutralisation :
        // appeler `sonde()` sans `catch_unwind` — le test panique.
        //
        // Le message de panique est avalé par `catch_unwind` mais reste imprimé
        // par le gestionnaire par défaut : c'est du bruit attendu dans la
        // sortie de `cargo test`, pas un échec.
        assert_eq!(
            detecter_nvenc(|| panic!("Unable to dynamically load the \"cuda\" shared library")),
            None
        );
    }

    #[test]
    fn une_carte_hevc_444_est_retenue_pour_le_texte() {
        let caps =
            EncoderCaps { gpu_name: "RTX".into(), codecs: vec![Codec::H264_420, Codec::Hevc444] };
        assert_eq!(detecter_nvenc(move || Ok(caps)), Some(Codec::Hevc444));
    }

    #[test]
    fn un_nom_de_machine_que_le_site_refuse_devient_pc() {
        // Spec §10 : validé par la règle de sky-compte (1 à 64 unités UTF-16,
        // sans NUL). Neutralisation : retirer le `filter` — le nom de 65 unités
        // passe, le site le refuserait à l'enregistrement.
        assert_eq!(nom_d_appareil(Some("BUREAU-KILLIAN")), "BUREAU-KILLIAN");
        assert_eq!(nom_d_appareil(Some(&"x".repeat(65))), NOM_D_APPAREIL_DE_REPLI);
        assert_eq!(nom_d_appareil(Some("a\u{0}b")), NOM_D_APPAREIL_DE_REPLI);
        assert_eq!(nom_d_appareil(Some("")), NOM_D_APPAREIL_DE_REPLI);
        assert_eq!(nom_d_appareil(None), NOM_D_APPAREIL_DE_REPLI);
    }
}
