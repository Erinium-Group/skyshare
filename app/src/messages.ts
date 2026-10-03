import type { FinVue } from "./types";

/**
 * Chaque fin de partage, EN CLAIR (spec §4).
 *
 * Les quatre premiers textes viennent de la spec §4 du jalon 1, et les quatre
 * causes du décodage de la spec du jalon 2 (§7) : ce sont des promesses faites
 * à l'utilisateur, pas des étiquettes d'interface. Ils ont chacun leur test dans
 * `messages.test.ts`. Ils ne sont PLUS tous mot pour mot ceux de ces specs :
 * `envoi_en_retard` (ex-`trop_lente`) a été corrigé par la vague finale du
 * jalon 2 parce qu'il désignait une cause fausse, et deux des quatre textes du
 * décodage ont été réécrits par le jalon « toutes cartes » (voir plus bas) —
 * chacun le dit sur son `case`.
 *
 * TROIS TEXTES N'EN VIENNENT PAS, tous ajoutés par la tâche 10 du jalon 2 et
 * signalés comme tels sur leur `case` : `partage_arrete`,
 * `resolution_trop_grande` et `decodage_interrompu`. Ils sont à valider par le
 * propriétaire. La liste complète des textes hors spec est dans la spec du
 * jalon 2, §7 ; ceux du refus de partager sans format encodable
 * (`MESSAGE_AUCUN_FORMAT`) et de recevoir sans décodeur
 * (`MESSAGE_AUCUN_DECODEUR`) vivent dans `noyau.rs`. `sans_carte_nvidia` et
 * `sans_decodage_444` ont été réécrits par le jalon « toutes cartes »
 * (spec du 02/10/2026, §6), et ne sont donc plus ceux de la spec du jalon 2.
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
    // --- Spec du jalon 2, §7 : les quatre causes du décodage. ---------------
    // Deux textes y sont encore mot pour mot (`decodeur_refuse`,
    // `image_irreconstituable`) ; les deux premiers ci-dessous ont été réécrits.
    // L'interface se branche sur la CAUSE, jamais sur un texte d'erreur : le
    // détail de « pas de carte NVIDIA » vient d'une bibliothèque que Windows
    // peut traduire, et ne traverse jamais la frontière.
    case "sans_carte_nvidia":
      // TEXTE RÉÉCRIT (jalon « toutes cartes », tâche 8), à valider par le
      // propriétaire. Le texte de la spec §7 (« ni partager ni recevoir ») est
      // devenu faux : la réception passe par Media Foundation et n'exige plus
      // NVIDIA. Cette cause ne survient plus qu'à l'ouverture de NVDEC, après
      // une sonde réussie.
      return "Le décodeur NVIDIA de cette machine n'a pas pu être chargé. Relance le visionnage.";
    case "sans_decodage_444":
      // TEXTE RÉÉCRIT (jalon « toutes cartes », tâche 8), à valider par le
      // propriétaire. Il remplace celui de la spec §7, corrigé une première fois
      // le 02/10/2026 (I2) et redevenu faux : « cette machine ne peut pas
      // recevoir » ne l'est plus, un décodage 4:2:0 ou H.264 existe. La cause
      // signifie désormais que le flux reçu, annoncé 4:4:4, n'est pas décodable
      // en 4:4:4 par NVDEC — l'image n'est pas au format annoncé.
      return "L'image reçue n'est pas au format annoncé. Relance le visionnage ; si cela se reproduit, demande à ton ami de relancer son partage.";
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
