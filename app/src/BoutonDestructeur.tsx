import { useState } from "react";

/**
 * Un geste qu'on ne rattrape pas ne part jamais du premier clic.
 *
 * ARBITRAGE DU CONTRÔLEUR (tâche 10) : supprimer une liste et révoquer un
 * appareil se confirment. La confirmation est posée DANS la page, à l'endroit du
 * geste, et non par `window.confirm` : dans une fenêtre Tauri, `confirm` est une
 * boîte du système que rien n'habille, et il bloque le fil de l'interface.
 *
 * Le libellé de confirmation est CELUI DU GESTE, jamais un « OK » : c'est la
 * seule chose que l'utilisateur lit vraiment avant de cliquer une seconde fois.
 */
export function BoutonDestructeur(props: {
  /** Ce qui ouvre la confirmation, par exemple « Supprimer ». */
  libelle: string;
  /** Ce qui la valide, et qui doit nommer le geste ET sa cible. */
  confirmer: string;
  enVol: boolean;
  agir: () => void;
  className?: string;
}) {
  const [demande, setDemande] = useState(false);

  if (!demande) {
    return (
      <button
        type="button"
        disabled={props.enVol}
        className={props.className ?? "rounded-md px-4 py-2 text-alerte disabled:opacity-40"}
        onClick={() => setDemande(true)}
      >
        {props.libelle}
      </button>
    );
  }

  return (
    <span className="flex items-center gap-2">
      <button
        type="button"
        disabled={props.enVol}
        className="rounded-md bg-alerte px-3 py-1 text-fond disabled:opacity-40"
        onClick={() => {
          setDemande(false);
          props.agir();
        }}
      >
        {props.confirmer}
      </button>
      <button
        type="button"
        aria-label={`Annuler — ${props.confirmer}`}
        className="rounded-md px-3 py-1 text-texte-2 hover:bg-surface-haute"
        onClick={() => setDemande(false)}
      >
        Annuler
      </button>
    </span>
  );
}
