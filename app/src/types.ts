// Miroir EXACT de spike/crates/sky-app/src/vue.rs. Un nom changé là-bas doit
// l'être ici : le test Rust `le_partage_est_serialise_sous_les_noms_que_lit_l_interface`
// fige la forme émise.

export type Connexion = "deconnecte" | "en_cours" | "connecte" | "session_expiree";

export interface AmiVue {
  id: number;
  friendshipId: number;
  nom: string;
  appareils: number;
}

export interface DemandeVue {
  friendshipId: number;
  nom: string;
}

export interface ListeVue {
  id: number;
  nom: string;
  couleur: string | null;
  emoji: string | null;
  /** Identifiants d'UTILISATEUR (AmiVue.id), jamais d'amitié. */
  membres: number[];
}

export interface AppareilVue {
  id: number;
  nom: string;
  courant: boolean;
  revoque: boolean;
}

export interface EcranVue {
  index: number;
  nom: string;
  principal: boolean;
}

export type FinVue =
  | { cause: "arrete" }
  | { cause: "pas_en_partage"; ami: string }
  | { cause: "reseau_bloque" }
  | { cause: "envoi_en_retard" }
  | { cause: "session_expiree" }
  | { cause: "aucune_demande"; fenetreS: number }
  | { cause: "partage_arrete" }
  | { cause: "sans_carte_nvidia" }
  | { cause: "sans_decodage_444" }
  | { cause: "decodeur_refuse" }
  | {
      cause: "resolution_trop_grande";
      largeur: number;
      hauteur: number;
      largeurMax: number;
      hauteurMax: number;
    }
  | { cause: "image_irreconstituable" }
  | { cause: "decodage_interrompu" }
  | { cause: "autre"; message: string };

export type PartageVue =
  | { etat: "inactif" }
  | { etat: "disponible"; debutMs: number; fenetreS: number; ecran: number }
  | {
      etat: "diffuse";
      spectateur: string | null;
      depuisMs: number;
      debitMbps: number;
      /** `null` quand rien ne le mesure — aujourd'hui toujours, côté hôte. */
      rttMs: number | null;
      ecran: number;
    }
  | { etat: "demande"; ami: string; debutMs: number }
  | {
      etat: "regarde";
      ami: string;
      connecteEnS: number;
      debitMbps: number;
      imagesParS: number;
      gigueMs: number;
      latenceDecodageMs: number;
      imagesAbandonnees: number;
      depuisMs: number;
    }
  | { etat: "termine"; fin: FinVue };

export interface Instantane {
  connexion: Connexion;
  nom: string | null;
  code: string | null;
  amis: AmiVue[];
  demandes: DemandeVue[];
  listes: ListeVue[];
  appareils: AppareilVue[];
  partage: PartageVue;
  nvenc: boolean;
  ecrans: EcranVue[];
  demarrageAutomatique: boolean;
}
