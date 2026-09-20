import { useEffect, useState } from "react";
import { pont } from "./pont";
import type { Instantane } from "./types";

/**
 * Ce que l'interface sait du cœur à un instant donné. Trois cas, jamais deux :
 * l'échec DOIT être distinct de l'attente, sinon un rejet se confond avec un
 * chargement qui n'aboutit pas — « Chargement… » pour toujours, sans alerte ni
 * issue (ronde de correction 1, I1).
 */
export type EtatDuCoeur =
  | { phase: "chargement" }
  | { phase: "pret"; instantane: Instantane }
  | { phase: "echec"; message: string };

/**
 * Le dernier instantané du cœur : lu une fois au montage (un événement émis
 * avant que l'interface écoute serait perdu), puis tenu à jour par `etat`.
 *
 * LES DEUX PROMESSES ONT UNE REPRISE. `etat_courant` est infaillible
 * aujourd'hui (`commandes.rs` rend `Ok` sans même passer par `sur_un_fil`),
 * mais rien ne le garantit aux tâches 10 et 11, et `listen` peut échouer de
 * son côté. Un rejet avalé ici serait invisible : l'application resterait sur
 * « Chargement… » sans que personne puisse savoir pourquoi.
 *
 * Le PREMIER échec gagne et n'est plus écrasé : un instantané déjà reçu
 * continue de s'afficher si l'abonnement échoue ensuite — mieux vaut un état
 * figé mais lisible qu'un écran d'erreur qui jette ce qu'on savait.
 */
export function useInstantane(): EtatDuCoeur {
  const [etat, setEtat] = useState<EtatDuCoeur>({ phase: "chargement" });
  useEffect(() => {
    let actif = true;
    let arreter: (() => void) | undefined;
    const echouer = (e: unknown) => {
      if (actif) {
        setEtat((precedent) => (precedent.phase === "chargement" ? { phase: "echec", message: String(e) } : precedent));
      }
    };
    const recevoir = (instantane: Instantane) => {
      if (actif) setEtat({ phase: "pret", instantane });
    };
    void pont.etatCourant().then(recevoir, echouer);
    void pont.ecouterEtat(recevoir).then((desabonner) => {
      // Le démontage a pu survenir pendant l'abonnement : se désabonner
      // aussitôt plutôt que de garder un écouteur sur un composant mort.
      if (actif) arreter = desabonner;
      else desabonner();
    }, echouer);
    return () => {
      actif = false;
      arreter?.();
    };
  }, []);
  return etat;
}
