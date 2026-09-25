import type { FinVue } from "./types";

/**
 * Chaque fin de partage, EN CLAIR (spec §4).
 *
 * Les quatre premiers textes sont ceux de la spec, au mot près : ce sont des
 * promesses faites à l'utilisateur, pas des étiquettes d'interface. Ils ont
 * chacun leur test dans `messages.test.ts`.
 *
 * ARBITRAGE 4 DU CONTRÔLEUR : l'interface ne FABRIQUE aucun message à partir de
 * données brutes. La seule cause qui porte du texte, `autre`, le tient du cœur,
 * où il a déjà été borné par `detail_borne` (`noyau.rs`) — jamais d'adresse,
 * jamais de jeton, jamais de clé.
 *
 * Le `switch` est EXHAUSTIF sans `default` : ajouter une variante à `FinVue`
 * sans l'ajouter ici fait échouer la compilation (`string | undefined` contre
 * `string`), et non apparaître un « undefined » à l'écran.
 */
export function messageDeFin(fin: FinVue): string {
  switch (fin.cause) {
    case "pas_en_partage":
      return `${fin.ami} n'est pas en partage`;
    case "reseau_bloque":
      // Un CONSTAT, pas une cause supposée : rien ne mesure lequel des deux
      // réseaux bloque (spec §4).
      return "Aucune connexion directe n'a pu s'établir entre vos deux réseaux";
    case "trop_lente":
      // Tampon d'émission saturé — l'écart 7 du projet, corrigé au jalon 2.
      return "La connexion était trop lente pour la vidéo";
    case "session_expiree":
      return "Session expirée — reconnecte-toi";
    case "arrete":
      return "Partage arrêté.";
    case "aucune_demande":
      return "Personne n'a demandé à regarder pendant 30 minutes.";
    case "autre":
      return fin.message;
  }
}

/**
 * Un nombre à une décimale, séparateur à la VIRGULE : « 0,6 », « 12,4 ».
 *
 * RONDE DE CORRECTION 1, M1. `toFixed(1)` rend un point — la convention
 * anglaise. C'est la seule sortie chiffrée que l'utilisateur lit, dans une
 * application dont la langue de travail est le français ; la spec et les mesures
 * du jalon 0 écrivent « 0,4 s » et « 12,4 Mbps ».
 *
 * Écrit à la main plutôt que par `toLocaleString("fr-FR")` : la locale d'une
 * `WebView2` suit celle de Windows, et la même mesure s'afficherait « 12.4 » sur
 * une machine en anglais. Le séparateur ne doit dépendre d'aucun réglage.
 */
export function decimale(valeur: number): string {
  return valeur.toFixed(1).replace(".", ",");
}

/**
 * « 2 min 05 s ». Une durée négative vaut zéro : le temps restant se calcule par
 * soustraction, et la fenêtre de 30 minutes s'écoule pendant que l'instantané
 * précédent est encore à l'écran — « -1 min 55 s » n'a aucun sens pour qui le lit.
 */
export function duree(ms: number): string {
  const secondes = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(secondes / 60)} min ${String(secondes % 60).padStart(2, "0")} s`;
}
