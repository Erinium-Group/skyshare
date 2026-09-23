import { useRef, useState } from "react";

export type Executer = (action: () => Promise<string | void>) => Promise<void>;

/**
 * Ce que rend `useActions` : l'exécuteur, le fait qu'une action court, et le
 * dernier message rendu par le cœur.
 *
 * RONDE DE CORRECTION 1 DE LA TÂCHE 8, I2. Sans le drapeau `enVol`, rien
 * n'amortissait un clic répété : une touche Entrée maintenue dans le champ code
 * ami produisait une rafale de POST vers la PRODUCTION, chacun suivi d'une
 * resynchronisation. Le projet a délibérément renoncé à toute limitation de
 * débit côté site (elle serait inopérante sur Vercel, où le compteur vit en
 * mémoire et chaque invocation peut tomber sur une autre instance) : le rempart
 * ne peut donc être QU'ICI. Ce n'est pas une décoration d'interface, c'est la
 * seule borne qui existe.
 *
 * TÂCHE 10 : sorti d'`Amis.tsx`, où il était né, parce que les écrans Listes et
 * Mon compte en ont besoin à l'identique. Le recopier deux fois aurait donné
 * trois versions de la SEULE borne du projet, à corriger trois fois.
 */
export interface Actions {
  executer: Executer;
  enVol: boolean;
  message: string | null;
}

/**
 * Toute action passe par ici : le cœur rend soit un message de réussite, soit
 * un refus DÉJÀ borné par `detail_borne` (`noyau.rs`). L'interface l'affiche
 * tel quel et n'en fabrique aucun — elle ne sait rien du site.
 *
 * UNE SEULE action à la fois : tant qu'une court, les suivantes sont refusées
 * ici même, en plus des boutons désactivés. Les deux sont nécessaires — le
 * `disabled` d'un bouton se contourne par la touche Entrée d'un formulaire, et
 * un `disabled` ne protège de rien si le gestionnaire reste appelable.
 */
export function useActions(): Actions {
  const [enVol, setEnVol] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  // Une RÉFÉRENCE, pas l'état : elle est à jour dès l'instruction suivante,
  // alors que `enVol` garde sa valeur pour tout le rendu en cours. Deux
  // soumissions dans le même tour — une touche Entrée maintenue — verraient
  // toutes deux `enVol === false` et partiraient toutes deux.
  const enVolRef = useRef(false);

  const executer: Executer = async (action) => {
    if (enVolRef.current) return;
    enVolRef.current = true;
    setEnVol(true);
    try {
      const retour = await action();
      setMessage(typeof retour === "string" ? retour : null);
    } catch (erreur) {
      setMessage(String(erreur));
    } finally {
      // `finally` : une action qui échoue doit rendre la main comme une autre,
      // sinon un seul refus fige l'écran pour de bon.
      enVolRef.current = false;
      setEnVol(false);
    }
  };

  return { executer, enVol, message };
}
