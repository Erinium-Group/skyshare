import { useEffect, useState } from "react";

/**
 * L'heure courante, rafraîchie toutes les `periodeMs` — mais SEULEMENT tant que
 * `actif` est vrai.
 *
 * Le cœur ne publie un instantané que sur CHANGEMENT : entre deux
 * synchronisations, rien ne repousse le rendu. Un « Temps restant » ou une
 * « Durée » calculés au seul moment de l'instantané resteraient donc figés. Ce
 * crochet est la seule horloge de l'interface ; il ne demande rien au cœur.
 *
 * RONDE DE CORRECTION 1, M2 : `actif` n'est pas un confort. Sans lui, les deux
 * composants faisaient battre une minuterie chacun, EN PERMANENCE — deux rendus
 * par seconde pour toute la durée de vie de l'application, alors que
 * l'écrasante majorité de ce temps se passe hors de tout partage. Une
 * application qui « vit comme Discord » (spec D4) reste ouverte des journées
 * entières : c'est une dépense continue pour rien.
 *
 * Le `clearInterval` du démontage n'est pas une politesse : sans lui, chaque
 * partage laisse derrière lui une minuterie qui appelle `setMaintenant` sur un
 * composant démonté, et le compte grossit à chaque aller-retour entre écrans.
 * `actif` repassant à faux emprunte exactement le même chemin de nettoyage.
 */
export function useMaintenant(actif: boolean, periodeMs = 1000): number {
  const [maintenant, setMaintenant] = useState(() => Date.now());
  useEffect(() => {
    if (!actif) return;
    // Une remise à l'heure À L'ALLUMAGE : la valeur retenue peut dater du
    // montage, bien avant que le partage commence, et le premier affichage
    // serait faux pendant une seconde.
    setMaintenant(Date.now());
    const minuterie = setInterval(() => setMaintenant(Date.now()), periodeMs);
    return () => clearInterval(minuterie);
  }, [actif, periodeMs]);
  return maintenant;
}
