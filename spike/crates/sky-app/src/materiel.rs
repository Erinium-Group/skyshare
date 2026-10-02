//! Ce que l'application apprend de la machine.

use sky_encode::{Codec, EncodeError, EncoderCaps};

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

/// Tous les codecs que l'encodeur de cette machine sait produire, ou `vec![]`
/// sans carte NVIDIA. Le choix du format transmis n'est plus fait ici : il
/// l'est par la négociation avec le spectateur (`sky_partage::formats_encodables`).
///
/// SANS CARTE NVIDIA, CETTE MACHINE NE PEUT NI PARTAGER NI REGARDER : il n'y a
/// ni NVENC ni NVDEC, et aucun repli logiciel n'existe dans le projet (spec du
/// jalon 2, §7). L'ancienne phrase « une telle machine peut encore regarder »
/// datait d'avant le décodage ; un spectateur sans carte l'apprend désormais à
/// l'ouverture du décodeur, par `FinVue::SansCarteNvidia`.
///
/// LE `catch_unwind` N'EST PLUS LE REMPART CONTRE L'ABSENCE DE PILOTE. Depuis
/// la tâche 7 du jalon 2, `probe_hardware` vérifie `nvcuda.dll` AVANT d'appeler
/// `cudarc` (`sky-encode/src/caps.rs`) : l'absence de pilote est une erreur
/// ordinaire, `Ok(Err(_))` ci-dessous, et ne panique plus.
///
/// Il reste, en défense en profondeur, pour une raison précise : la sonde
/// tourne sur le fil de la synchronisation, AVANT `demarrer` et `boucle`
/// (`lib.rs`), et elle garde des chemins qui paniquent après cette vérification
/// — les `expect` sur la table de fonctions NVENC d'un pilote incomplet. Une
/// panique là tuerait en silence la seule boucle de synchronisation de
/// l'application, qui resterait affichée et figée.
pub fn detecter_nvenc(
    sonde: impl FnOnce() -> Result<EncoderCaps, EncodeError> + std::panic::UnwindSafe,
) -> Vec<Codec> {
    match std::panic::catch_unwind(sonde) {
        Ok(Ok(caps)) => caps.codecs,
        Ok(Err(_)) | Err(_) => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_sonde_qui_panique_rend_aucune_carte() {
        // La défense en profondeur : une sonde qui panique — aujourd'hui un
        // `expect` sur la table NVENC d'un pilote incomplet, plus l'absence de
        // `nvcuda.dll`, qui est vérifiée avant — ne doit pas emporter le fil de
        // la synchronisation. Neutralisation : appeler `sonde()` sans
        // `catch_unwind` — le test panique.
        //
        // Le message de panique est avalé par `catch_unwind` mais reste imprimé
        // par le gestionnaire par défaut : c'est du bruit attendu dans la
        // sortie de `cargo test`, pas un échec.
        assert_eq!(
            detecter_nvenc(|| panic!(
                "la table de fonctions NVENC doit être remplie par NvEncodeAPICreateInstance"
            )),
            Vec::<Codec>::new()
        );
    }

    #[test]
    fn les_codecs_de_la_carte_sont_tous_retenus() {
        let caps = EncoderCaps {
            gpu_name: "RTX".into(),
            codecs: vec![Codec::H264_420, Codec::Hevc420, Codec::Hevc444],
        };
        assert_eq!(
            detecter_nvenc(move || Ok(caps)),
            vec![Codec::H264_420, Codec::Hevc420, Codec::Hevc444]
        );
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
