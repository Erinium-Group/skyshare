import { useEffect, useState } from "react";
import { pont } from "./pont";
import type { Instantane } from "./types";

/**
 * Le dernier instantané du cœur : lu une fois au montage (un événement émis
 * avant que l'interface écoute serait perdu), puis tenu à jour par `etat`.
 */
export function useInstantane(): Instantane | null {
  const [instantane, setInstantane] = useState<Instantane | null>(null);
  useEffect(() => {
    let actif = true;
    let arreter: (() => void) | undefined;
    void pont.etatCourant().then((etat) => {
      if (actif) setInstantane(etat);
    });
    void pont
      .ecouterEtat((etat) => {
        if (actif) setInstantane(etat);
      })
      .then((desabonner) => {
        // Le démontage a pu survenir pendant l'abonnement : se désabonner
        // aussitôt plutôt que de garder un écouteur sur un composant mort.
        if (actif) arreter = desabonner;
        else desabonner();
      });
    return () => {
      actif = false;
      arreter?.();
    };
  }, []);
  return instantane;
}
