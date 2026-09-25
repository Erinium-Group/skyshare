import { useState } from "react";
import { useActions } from "../actions";
import { duree } from "../messages";
import { pont } from "../pont";
import type { Instantane } from "../types";
import { useMaintenant } from "./useMaintenant";

/**
 * Le bas de la barre latérale — le SEUL endroit du partage qui vit hors des
 * écrans (spec D6).
 *
 * ARBITRAGE 3 DU CONTRÔLEUR : « on ne partage jamais sans le savoir ». Le
 * panneau central disparaît dès qu'on passe sur Listes ou Mon compte ; cette
 * zone, elle, reste. Avec l'icône près de l'horloge (tenue par le cœur), c'est
 * ce qui garantit qu'un partage en cours ne peut pas être oublié.
 *
 * Toutes les actions passent par `useActions` : `partager` rend la machine
 * disponible EN PRODUCTION, `arreter` la retire. Ce sont des requêtes réseau, et
 * la garde « une seule action à la fois » est la seule borne du projet contre
 * les rafales — le site a délibérément renoncé à toute limitation de débit
 * (inopérante sur Vercel).
 */
export function BarrePartage({ instantane }: { instantane: Instantane }) {
  // `null` tant que l'utilisateur n'a rien choisi, et NON `useState(principal)` :
  // le cœur publie `ecrans: []` à la création et ne relève les écrans qu'au
  // premier passage au premier plan. Un état initialisé au premier rendu
  // figerait le rang de repli 0, et on partagerait le mauvais écran sans que
  // rien ne le dise.
  const [choix, setChoix] = useState<number | null>(null);
  const actions = useActions();
  const maintenant = useMaintenant();
  const partage = instantane.partage;
  const principal = instantane.ecrans.find((e) => e.principal)?.index ?? 0;
  const ecran = choix ?? principal;

  if (partage.etat === "disponible" || partage.etat === "diffuse") {
    const nomEcran =
      instantane.ecrans.find((e) => e.index === partage.ecran)?.nom ?? `Écran ${partage.ecran + 1}`;
    return (
      <div
        role="region"
        aria-label="Partage en cours"
        className="flex flex-col gap-2 rounded-md bg-accent p-3 text-fond"
      >
        <strong>En partage</strong>
        <span className="text-sm">{nomEcran}</span>
        {/* `diffuse` n'a pas de `fenetreS` : la fenêtre de disponibilité
            s'arrête quand quelqu'un regarde. Afficher un temps restant là
            obligerait à en inventer un. */}
        {partage.etat === "disponible" && (
          <span className="text-sm">
            Temps restant : {duree(partage.debutMs + partage.fenetreS * 1000 - maintenant)}
          </span>
        )}
        {/* JAMAIS désactivé, même pendant une action : un bouton d'arrêt grisé
            laisserait un partage en cours sans issue visible. Sa seule borne
            est la garde d'`useActions`, et c'est suffisant. */}
        <button
          type="button"
          className="rounded-md bg-fond px-3 py-1 text-texte"
          onClick={() => void actions.executer(() => pont.arreter())}
        >
          Arrêter
        </button>
        {actions.message && (
          <p role="alert" className="text-xs">
            {actions.message}
          </p>
        )}
      </div>
    );
  }

  // Une demande envoyée ou un partage regardé occupent déjà la machine : elle
  // ne peut pas se rendre disponible en même temps.
  const occupe = partage.etat === "demande" || partage.etat === "regarde";
  return (
    <div className="flex flex-col gap-2">
      {instantane.ecrans.length > 1 && (
        <label className="flex flex-col gap-1 text-sm text-texte-2">
          Écran
          <select
            value={ecran}
            onChange={(e) => setChoix(Number(e.target.value))}
            className="rounded-md bg-surface-haute px-2 py-1 text-texte"
          >
            {instantane.ecrans.map((e) => (
              <option key={e.index} value={e.index}>
                {e.nom}
                {e.principal ? " (principal)" : ""}
              </option>
            ))}
          </select>
        </label>
      )}
      <button
        type="button"
        disabled={!instantane.nvenc || occupe || actions.enVol}
        className="w-full rounded-md bg-accent px-3 py-2 text-fond disabled:opacity-50"
        onClick={() => void actions.executer(() => pont.partager(ecran))}
      >
        Partager mon écran
      </button>
      {/* Pas de repli logiciel x264 — question ouverte du projet. La machine ne
          peut que recevoir, et on le dit plutôt que de laisser un bouton mort. */}
      {!instantane.nvenc && (
        <p className="text-xs text-texte-3">
          Partage impossible : aucune carte NVIDIA sur cette machine. Tu peux regarder.
        </p>
      )}
      {actions.message && (
        <p role="alert" className="text-xs text-alerte">
          {actions.message}
        </p>
      )}
    </div>
  );
}
