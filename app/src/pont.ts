import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Instantane } from "./types";

/**
 * Le SEUL module qui parle au cœur Rust. L'interface n'a ni jeton, ni clé,
 * ni accès réseau (spec §3, frontière D5) : elle lit des instantanés et envoie
 * des intentions. Les noms d'arguments sont en camelCase : Tauri 2 les
 * traduit vers les paramètres snake_case des commandes (`friendshipId` →
 * `friendship_id`).
 */
export const pont = {
  etatCourant: () => invoke<Instantane>("etat_courant"),
  ecouterEtat: (rappel: (instantane: Instantane) => void): Promise<UnlistenFn> =>
    listen<Instantane>("etat", (evenement) => rappel(evenement.payload)),
  connexion: () => invoke<void>("connexion"),
  deconnexion: () => invoke<void>("deconnexion"),
  /** `code` : la saisie BRUTE. La normalisation appartient au cœur. */
  ajouterAmi: (code: string) => invoke<string>("ajouter_ami", { code }),
  accepterAmi: (friendshipId: number) => invoke<string>("accepter_ami", { friendshipId }),
  retirerAmi: (friendshipId: number) => invoke<string>("retirer_ami", { friendshipId }),
  bloquerAmi: (friendshipId: number) => invoke<string>("bloquer_ami", { friendshipId }),
  /** `ami` : identifiant d'UTILISATEUR. Commande ajoutée à la tâche 11. */
  regarder: (ami: number) => invoke<void>("regarder", { ami }),
  creerListe: (nom: string, couleur: string | null, emoji: string | null) =>
    invoke<string>("creer_liste", { nom, couleur, emoji }),
  modifierListe: (id: number, nom: string, couleur: string | null, emoji: string | null) =>
    invoke<string>("modifier_liste", { id, nom, couleur, emoji }),
  supprimerListe: (id: number) => invoke<string>("supprimer_liste", { id }),
  /** `membres` : identifiants d'UTILISATEUR (AmiVue.id), jamais d'amitié. */
  definirMembres: (id: number, membres: number[]) =>
    invoke<string>("definir_membres", { id, membres }),
  regenererCode: () => invoke<string>("regenerer_code"),
  /** `id` : identifiant d'APPAREIL. Jamais celui de cette machine — le cœur le refuse. */
  revoquerAppareil: (id: number) => invoke<string>("revoquer_appareil", { id }),
  demarrageAutomatique: (actif: boolean) => invoke<void>("demarrage_automatique", { actif }),
};
