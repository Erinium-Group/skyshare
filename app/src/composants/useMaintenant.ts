import { useEffect, useState } from "react";

/**
 * L'heure courante, rafraîchie toutes les `periodeMs`.
 *
 * Le cœur ne publie un instantané que sur CHANGEMENT : entre deux
 * synchronisations, rien ne repousse le rendu. Un « Temps restant » ou une
 * « Durée » calculés au seul moment de l'instantané resteraient donc figés. Ce
 * crochet est la seule horloge de l'interface ; il ne demande rien au cœur.
 *
 * Le `clearInterval` du démontage n'est pas une politesse : sans lui, chaque
 * partage laisse derrière lui une minuterie qui appelle `setMaintenant` sur un
 * composant démonté, et le compte grossit à chaque aller-retour entre écrans.
 */
export function useMaintenant(periodeMs = 1000): number {
  const [maintenant, setMaintenant] = useState(() => Date.now());
  useEffect(() => {
    const minuterie = setInterval(() => setMaintenant(Date.now()), periodeMs);
    return () => clearInterval(minuterie);
  }, [periodeMs]);
  return maintenant;
}
