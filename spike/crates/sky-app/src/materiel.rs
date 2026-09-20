//! Ce que l'application apprend de la machine.

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

#[cfg(test)]
mod tests {
    use super::*;

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
