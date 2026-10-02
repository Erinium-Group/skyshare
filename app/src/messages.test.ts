import { describe, expect, it } from "vitest";
import { decimale, duree, messageDeFin } from "./messages";

/**
 * Les quatre échecs de la spec §4 ont CHACUN leur test : un seul test qui les
 * cite tous les quatre rougit dès qu'un mot bouge, mais il ne dit pas lequel, et
 * surtout il ne distingue pas « le texte a changé » de « la mauvaise branche a
 * été choisie ». Un test par cause rougit nommément quand `reseau_bloque` rend
 * le texte d'`envoi_en_retard`.
 *
 * Ces textes sont une PROMESSE faite à l'utilisateur, pas une étiquette
 * d'interface : les changer change ce qu'on lui dit de son réseau et de sa
 * session. Ils viennent de la spec §4, « Échecs, tous en clair » — sauf
 * `envoi_en_retard`, corrigé par la vague finale du jalon 2 parce que le texte
 * de la spec désignait une cause réseau fausse.
 */
describe("les quatre échecs de la spec §4", () => {
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

  it("envoi en retard — une cause locale, qui n'accuse pas le réseau", () => {
    // Revue finale du jalon 2, M4b : la file de paquetisation est LOCALE,
    // vidée par nos propres `poll` sans rien attendre du correspondant. Le
    // texte de la spec (« la connexion était trop lente pour la vidéo ») accusait
    // le réseau. Neutralisation : le remettre — les deux assertions rougissent.
    const texte = messageDeFin({ cause: "envoi_en_retard" });
    expect(texte).toBe(
      "Le partage s'est arrêté : l'envoi de la vidéo a pris trop de retard sur cette machine.",
    );
    expect(texte).not.toMatch(/connexion|réseau|lente/);
  });

  it("session expirée — reconnecte-toi", () => {
    // Le même texte que l'écran de connexion (`App.test.tsx`) : une session
    // morte se dit d'une seule façon dans toute l'application.
    expect(messageDeFin({ cause: "session_expiree" })).toBe("Session expirée — reconnecte-toi");
  });
});

/**
 * Les quatre textes du décodage, spec du jalon 2 §7, au mot près — un test par
 * cause, pour la même raison qu'au-dessus. Neutralisation de chacun : changer
 * un mot de son texte, ou faire rendre à sa branche le texte d'une autre ; il
 * rougit, et lui seul.
 */
describe("les quatre messages du décodage, spec du jalon 2 §7, au mot près", () => {
  it("aucune carte NVIDIA : ni partager ni recevoir", () => {
    expect(messageDeFin({ cause: "sans_carte_nvidia" })).toBe(
      "Cette machine n'a pas de carte graphique NVIDIA. SkyShare ne peut ni partager son écran ni en recevoir un sur cette machine.",
    );
  });

  it("carte sans décodage 4:4:4 : recevoir non, et rien n'est promis du partage", () => {
    // Le texte de la spec promettait « peut partager un écran » : faux en
    // général (le partage exige l'ENCODAGE HEVC 4:4:4, qu'aucun code ne vérifie
    // ici). Corrigé le 02/10/2026 ; la seconde assertion garde la promesse
    // retirée hors du texte.
    const texte = messageDeFin({ cause: "sans_decodage_444" });
    expect(texte).toBe(
      "Le décodeur de la carte graphique de cette machine ne prend pas en charge la couleur pleine résolution : SkyShare ne peut pas recevoir d'écran sur cette machine.",
    );
    expect(texte).not.toMatch(/peut partager/);
  });

  it("décodeur refusé : fermer ce qui occupe la carte", () => {
    expect(messageDeFin({ cause: "decodeur_refuse" })).toBe(
      "Le décodeur vidéo n'a pas pu démarrer. Fermez les autres applications qui utilisent la carte graphique, puis réessayez.",
    );
  });

  it("image irreconstituable : demander de relancer le partage", () => {
    expect(messageDeFin({ cause: "image_irreconstituable" })).toBe(
      "L'image ne peut pas être reconstituée. Demandez à la personne qui partage de relancer son partage.",
    );
  });
});

describe("les fins du visionnage hors spec", () => {
  it("résolution trop grande : les chiffres viennent du cœur, et l'ami n'est pas accusé", () => {
    // Texte NOUVEAU (tâche 10), absent de la spec. Les chiffres viennent de
    // la variante — deux jeux différents donnent deux phrases différentes, ce
    // qu'un texte écrit en dur ne ferait pas. Neutralisation : intervertir
    // `largeurMax` et `hauteurMax` dans le gabarit — la première assertion
    // rougit.
    expect(
      messageDeFin({
        cause: "resolution_trop_grande",
        largeur: 2560,
        hauteur: 1440,
        largeurMax: 2048,
        hauteurMax: 1152,
      }),
    ).toBe(
      "Le décodeur vidéo de cette carte graphique s'arrête à 2048×1152 : SkyShare a besoin d'au moins 2560×1440 pour recevoir un écran sur cette machine.",
    );
    expect(
      messageDeFin({
        cause: "resolution_trop_grande",
        largeur: 2560,
        hauteur: 1440,
        largeurMax: 1920,
        hauteurMax: 1088,
      }),
    ).toContain("1920×1088");
  });

  it("un décodage interrompu en cours de flux ne parle ni de démarrage ni de la carte", () => {
    // Ronde de correction 1, I-1. Texte NOUVEAU. Ce qu'il ne doit PAS dire
    // compte autant que ce qu'il dit : l'image a pu s'afficher, et la carte
    // peut être parfaitement capable. Neutralisation : rendre le texte de
    // `decodeur_refuse` ou de `sans_decodage_444` — ce test rougit.
    const texte = messageDeFin({ cause: "decodage_interrompu" });
    expect(texte).toBe(
      "Le décodage de l'image s'est interrompu en cours de visionnage, malgré les demandes de reprise. Relance le visionnage ; si cela se reproduit, demande à ton ami de relancer son partage.",
    );
    expect(texte).not.toMatch(/démarrer|carte graphique/);
  });

  it("l'ami qui arrête son partage ne se dit pas comme un arrêt volontaire", () => {
    // D3 (tâche 10). Neutralisation : rendre « Partage arrêté. » — la
    // comparaison avec la fin `arrete` rougit.
    expect(messageDeFin({ cause: "partage_arrete" })).toBe("Ton ami a arrêté son partage.");
    expect(messageDeFin({ cause: "partage_arrete" })).not.toBe(messageDeFin({ cause: "arrete" }));
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
    expect(messageDeFin({ cause: "aucune_demande", fenetreS: 1800 })).toBe(
      "Personne n'a demandé à regarder pendant 30 minutes.",
    );
    // CE QUI DISCRIMINE (revue finale, M3) : la durée est celle que le CŒUR a
    // portée, pas un littéral de l'interface. Sans cette seconde assertion,
    // « pendant 30 minutes » écrit en dur passerait aussi bien — c'était
    // précisément le défaut, et `FENETRE_HOTE` aurait pu changer en silence.
    expect(messageDeFin({ cause: "aucune_demande", fenetreS: 600 })).toBe(
      "Personne n'a demandé à regarder pendant 10 minutes.",
    );
  });
});

describe("décimales", () => {
  it("le séparateur est la VIRGULE, jamais le point", () => {
    // RONDE DE CORRECTION 1, M1. `toFixed(1)` rend « 0.6 » — la convention
    // anglaise — dans une application dont la langue de travail est le
    // français, et sur la seule sortie chiffrée que l'utilisateur lit.
    // Neutralisation : retirer le `.replace(".", ",")` — les trois assertions
    // rougissent, la première sur « 0.6 ».
    expect(decimale(0.6)).toBe("0,6");
    expect(decimale(12.4)).toBe("12,4");
    // Une décimale TOUJOURS, même sur un entier : « 5 ms » et « 5,0 ms » dans
    // la même ligne de mesures sauteraient d'une largeur à l'autre.
    expect(decimale(5)).toBe("5,0");
  });

  it("l'arrondi reste celui de `toFixed`, à une décimale", () => {
    // Contrôle positif : sans lui, un `String(valeur).replace(…)` passerait le
    // test précédent tout en affichant « 12,449999999999999 Mbps ».
    expect(decimale(12.449999999999999)).toBe("12,4");
    expect(decimale(0.05)).toBe("0,1");
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
