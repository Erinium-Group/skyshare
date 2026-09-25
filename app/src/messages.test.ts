import { describe, expect, it } from "vitest";
import { duree, messageDeFin } from "./messages";

/**
 * Les quatre échecs de la spec §4 ont CHACUN leur test : un seul test qui les
 * cite tous les quatre rougit dès qu'un mot bouge, mais il ne dit pas lequel, et
 * surtout il ne distingue pas « le texte a changé » de « la mauvaise branche a
 * été choisie ». Un test par cause rougit nommément quand `reseau_bloque` rend
 * le texte de `trop_lente`.
 *
 * Ces textes sont une PROMESSE faite à l'utilisateur, pas une étiquette
 * d'interface : les changer change ce qu'on lui dit de son réseau et de sa
 * session. Ils viennent de la spec §4, « Échecs, tous en clair ».
 */
describe("les quatre échecs de la spec §4, au mot près", () => {
  it("l'ami n'est pas en partage — et il est NOMMÉ", () => {
    expect(messageDeFin({ cause: "pas_en_partage", ami: "Bob" })).toBe("Bob n'est pas en partage");
    // Le nom vient de `FinVue`, jamais d'une constante : un message qui dirait
    // « Ton ami » passerait le test précédent si le nom n'était pas interpolé.
    expect(messageDeFin({ cause: "pas_en_partage", ami: "Alice" })).toBe("Alice n'est pas en partage");
  });

  it("aucune connexion directe — un constat, pas une cause supposée", () => {
    // Spec §4 : rien ne mesure LEQUEL des deux réseaux bloque. Le message ne
    // doit donc accuser ni l'un ni l'autre.
    expect(messageDeFin({ cause: "reseau_bloque" })).toBe(
      "Aucune connexion directe n'a pu s'établir entre vos deux réseaux",
    );
  });

  it("connexion trop lente — la limite connue du transport actuel", () => {
    expect(messageDeFin({ cause: "trop_lente" })).toBe("La connexion était trop lente pour la vidéo");
  });

  it("session expirée — reconnecte-toi", () => {
    // Le même texte que l'écran de connexion (`App.test.tsx`) : une session
    // morte se dit d'une seule façon dans toute l'application.
    expect(messageDeFin({ cause: "session_expiree" })).toBe("Session expirée — reconnecte-toi");
  });
});

describe("les autres fins", () => {
  it("« autre » rend le message du cœur, sans rien y ajouter", () => {
    // ARBITRAGE 4 DU CONTRÔLEUR : l'interface ne FABRIQUE aucun message
    // d'erreur. Celui-ci vient du cœur, déjà borné par `detail_borne`
    // (`noyau.rs`) ; le reformuler ici rouvrirait la porte à une donnée brute
    // affichée telle quelle.
    expect(messageDeFin({ cause: "autre", message: "Connexion interrompue : x" })).toBe(
      "Connexion interrompue : x",
    );
  });

  it("un arrêt volontaire et une fenêtre écoulée ne se disent pas pareil", () => {
    expect(messageDeFin({ cause: "arrete" })).toBe("Partage arrêté.");
    expect(messageDeFin({ cause: "aucune_demande" })).toBe(
      "Personne n'a demandé à regarder pendant 30 minutes.",
    );
  });
});

describe("durées", () => {
  it("les secondes sont sur deux chiffres", () => {
    expect(duree(125_000)).toBe("2 min 05 s");
    expect(duree(0)).toBe("0 min 00 s");
    expect(duree(3_600_000)).toBe("60 min 00 s");
  });

  it("une durée négative vaut zéro, jamais « -1 min -5 s »", () => {
    // Le temps restant se calcule par soustraction : la fenêtre de 30 minutes
    // s'écoule pendant que l'instantané précédent est encore affiché.
    // Neutralisation : retirer le `Math.max(0, …)` — la première assertion
    // rougit sur « -1 min 55 s ».
    expect(duree(-5)).toBe("0 min 00 s");
    expect(duree(-115_000)).toBe("0 min 00 s");
  });
});
