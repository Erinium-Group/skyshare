import type { FinVue } from "./types";

/**
 * Chaque fin de partage, EN CLAIR (spec §4).
 *
 * Les quatre premiers textes viennent de la spec §4 du jalon 1, et les quatre
 * du décodage de la spec du jalon 2 (§7), au mot près : ce sont des promesses
 * faites à l'utilisateur, pas des étiquettes d'interface. Ils ont chacun leur
 * test dans `messages.test.ts`. DEUX ONT ÉTÉ CORRIGÉS par la vague finale du
 * jalon 2, parce qu'ils désignaient une cause fausse : `envoi_en_retard`
 * (ex-`trop_lente`) et `sans_decodage_444` — chacun le dit sur son `case`.
 *
 * TROIS TEXTES N'EN VIENNENT PAS, tous ajoutés par la tâche 10 du jalon 2 et
 * signalés comme tels sur leur `case` : `partage_arrete`,
 * `resolution_trop_grande` et `decodage_interrompu`. Ils sont à valider par le
 * propriétaire. La liste complète des textes hors spec, celui du refus de
 * partager sans HEVC 4:4:4 compris (`noyau.rs`), est dans la spec du jalon 2,
 * §7.
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
    case "envoi_en_retard":
      // TEXTE CORRIGÉ PAR LA VAGUE FINALE DU JALON 2 (revue de branche, M4b) :
      // il remplace « La connexion était trop lente pour la vidéo », texte de la
      // spec §4 du jalon 1. La fin vient de la file de paquetisation de la
      // piste média (`Fin::FileDePaquetisationPleine`), qui est LOCALE : `str0m`
      // la vide pendant nos propres `poll`, sans rien attendre du correspondant.
      // Elle dit que l'envoi a pris du retard sur cette machine, rien d'un
      // réseau lent. Cas jamais observé ; texte à valider par le propriétaire.
      return "Le partage s'est arrêté : l'envoi de la vidéo a pris trop de retard sur cette machine.";
    case "session_expiree":
      return "Session expirée — reconnecte-toi";
    case "arrete":
      return "Partage arrêté.";
    case "aucune_demande":
      // REVUE FINALE, M4 : la durée vient du CŒUR (`FENETRE_HOTE`, portée par
      // la variante), jamais d'un littéral recopié ici. Recopiée, elle aurait
      // menti en silence au premier changement de la constante.
      return `Personne n'a demandé à regarder pendant ${minutes(fin.fenetreS)}.`;
    case "partage_arrete":
      // TEXTE NOUVEAU, ABSENT DE LA SPEC (tâche 10). Une fin NORMALE, à
      // distinguer de « Partage arrêté. » : ce n'est pas l'utilisateur qui l'a
      // arrêté, c'est son ami.
      return "Ton ami a arrêté son partage.";
    // --- Spec du jalon 2, §7 : les quatre textes du décodage, AU MOT PRÈS. ---
    // L'interface se branche sur la CAUSE, jamais sur un texte d'erreur : le
    // détail de « pas de carte NVIDIA » vient d'une bibliothèque que Windows
    // peut traduire, et ne traverse jamais la frontière.
    case "sans_carte_nvidia":
      return "Cette machine n'a pas de carte graphique NVIDIA. SkyShare ne peut ni partager son écran ni en recevoir un sur cette machine.";
    case "sans_decodage_444":
      // TEXTE DE LA SPEC §7 CORRIGÉ le 02/10/2026 (revue finale du jalon 2,
      // I2). Il promettait « cette machine peut partager un écran » : rien ne
      // le vérifie à cet endroit — partager exige un ENCODEUR HEVC 4:4:4 —, et
      // le jalon 0 (écart 6) dit, sur base documentaire, que les cartes sans
      // décodage 4:4:4 ne l'encodent pas non plus. Le texte ne dit plus que ce
      // qui est su : cette machine ne peut pas recevoir.
      return "Le décodeur de la carte graphique de cette machine ne prend pas en charge la couleur pleine résolution : SkyShare ne peut pas recevoir d'écran sur cette machine.";
    case "decodeur_refuse":
      // Session refusée ou contexte CUDA perdu : deux causes, un message vrai
      // des deux.
      return "Le décodeur vidéo n'a pas pu démarrer. Fermez les autres applications qui utilisent la carte graphique, puis réessayez.";
    case "image_irreconstituable":
      return "L'image ne peut pas être reconstituée. Demandez à la personne qui partage de relancer son partage.";
    case "resolution_trop_grande":
      // TEXTE NOUVEAU, ABSENT DE LA SPEC (tâche 10). La spec ne prévoyait que
      // trois refus du décodeur ; celui-ci est le quatrième. Il est levé À
      // L'OUVERTURE contre la taille que le spectateur annonce, et depuis la
      // vague finale (I3) aussi au premier paquet contre la taille CODÉE du
      // flux reçu — qui peut dépasser l'écran de quelques lignes (1088 pour
      // 1080). Dans les deux cas le message dit ce qui manque à CETTE carte,
      // sans accuser l'ami.
      return `Le décodeur vidéo de cette carte graphique s'arrête à ${fin.largeurMax}×${fin.hauteurMax} : SkyShare a besoin d'au moins ${fin.largeur}×${fin.hauteur} pour recevoir un écran sur cette machine.`;
    case "decodage_interrompu":
      // TEXTE NOUVEAU, ABSENT DE LA SPEC (ronde de correction 1 de la tâche
      // 10). Le décodeur, qui a DÉJÀ montré au moins une image (vague finale,
      // I3 : avant, c'est une erreur d'ouverture), a refusé le flux trop
      // longtemps malgré les demandes d'image clé. Les
      // textes de la spec §7 décrivent l'OUVERTURE (« n'a pas pu démarrer »,
      // « cette carte ne prend pas en charge… ») et seraient faux ici. Rien ne
      // dit de quel côté vient le défaut : le message n'en désigne aucun.
      return "Le décodage de l'image s'est interrompu en cours de visionnage, malgré les demandes de reprise. Relance le visionnage ; si cela se reproduit, demande à ton ami de relancer son partage.";
    case "autre":
      return fin.message;
  }
}

/**
 * Une durée en secondes dite en minutes : « 30 minutes », « 1 minute ».
 *
 * Elle ne sert qu'à la fenêtre de disponibilité, qui vient du cœur en SECONDES
 * (`FinVue::AucuneDemande { fenetreS }`). L'arrondi est celui de la minute la
 * plus proche : la constante du cœur est un nombre de minutes entier, et une
 * valeur qui ne le serait pas vaut mieux arrondie qu'affichée en décimales.
 */
function minutes(secondes: number): string {
  const n = Math.max(1, Math.round(secondes / 60));
  return `${n} ${n === 1 ? "minute" : "minutes"}`;
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
 * soustraction, et la fenêtre de disponibilité s'écoule pendant que l'instantané
 * précédent est encore à l'écran — « -1 min 55 s » n'a aucun sens pour qui le lit.
 */
export function duree(ms: number): string {
  const secondes = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(secondes / 60)} min ${String(secondes % 60).padStart(2, "0")} s`;
}
