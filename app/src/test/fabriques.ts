import type { Instantane } from "../types";

/** Un instantané plausible, connecté, sans ami ni partage. */
export function instantaneDeTest(partiel: Partial<Instantane> = {}): Instantane {
  return {
    connexion: "connecte",
    nom: "Killian",
    code: "ABCD2345",
    amis: [],
    demandes: [],
    listes: [],
    appareils: [],
    partage: { etat: "inactif" },
    nvenc: true,
    ecrans: [{ index: 0, nom: "Écran 1", principal: true }],
    demarrageAutomatique: true,
    ...partiel,
  };
}
