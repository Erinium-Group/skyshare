import { useActions } from "../actions";
import { decimale, duree, messageDeFin } from "../messages";
import { pont } from "../pont";
import type { Instantane } from "../types";
import { useMaintenant } from "./useMaintenant";

function Mesure({ libelle, valeur }: { libelle: string; valeur: string }) {
  return (
    <div className="flex flex-col">
      <dt className="text-sm text-texte-3">{libelle}</dt>
      <dd className="text-lg">{valeur}</dd>
    </div>
  );
}

/**
 * Le panneau central du partage (spec §4), en tête de l'écran courant.
 *
 * Il ne remplace PAS la barre latérale : il disparaît dès qu'on change d'écran,
 * alors que l'état d'un partage en cours, lui, doit rester visible partout
 * (arbitrage 3). Les deux sont complémentaires, pas redondants.
 */
export function PanneauPartage({ instantane }: { instantane: Instantane }) {
  const actions = useActions();
  const partage = instantane.partage;
  // L'horloge ne bat que pour « Depuis » et « Durée » (ronde de correction 1,
  // M2). En `inactif`, ce composant rend `null` : il faisait pourtant battre
  // une minuterie par seconde pour rien.
  const maintenant = useMaintenant(partage.etat === "diffuse" || partage.etat === "regarde");
  const cadre = "mb-6 flex flex-col gap-3 rounded-md bg-surface p-4";

  /**
   * Arrêter, et le refus du cœur qui peut suivre.
   *
   * RONDE DE CORRECTION 1, I1 : le bouton était rendu sans `actions.message`.
   * Or ce panneau est le SEUL porteur d'un Arrêter dans `demande` et `regarde`
   * — la barre latérale y est sur une autre branche. Un `arreter` refusé y
   * laissait donc un clic sans le moindre effet visible : la variante interface
   * de « la serrure posée mais jamais branchée », et la promesse « les échecs,
   * tous en clair » (spec §4) tenue à moitié.
   *
   * Jamais désactivé : un bouton d'arrêt grisé laisserait un partage en cours
   * sans issue visible. Sa seule borne est la garde d'`useActions` — voir
   * l'invariant écrit dans `BarrePartage`, qui vaut ici mot pour mot : les deux
   * composants tiennent chacun leur `enVolRef`, et ne se coordonnent que parce
   * que leurs deux boutons ne coexistent jamais.
   */
  const arreter = (
    <>
      <button
        type="button"
        className="self-start rounded-md bg-surface-haute px-4 py-2"
        onClick={() => void actions.executer(() => pont.arreter())}
      >
        Arrêter
      </button>
      {actions.message && (
        <p role="alert" className="text-sm text-alerte">
          {actions.message}
        </p>
      )}
    </>
  );

  switch (partage.etat) {
    case "inactif":
      return null;
    case "disponible":
      return (
        <section aria-label="Partage" className={cadre}>
          <h2 className="font-titre text-2xl">Tu es disponible</h2>
          <p className="text-texte-2">
            Tes amis peuvent te demander à regarder. Personne ne regarde encore.
          </p>
        </section>
      );
    case "diffuse":
      return (
        <section aria-label="Partage" className={cadre}>
          {/* `spectateur` peut être nul : le cœur ne connaît pas toujours le
              nom. « null regarde ton écran » serait une fuite de plomberie. */}
          <h2 className="font-titre text-2xl">{partage.spectateur ?? "Un ami"} regarde ton écran</h2>
          <dl className="flex gap-8">
            <Mesure libelle="Depuis" valeur={duree(maintenant - partage.depuisMs)} />
            <Mesure libelle="Débit envoyé" valeur={`${decimale(partage.debitMbps)} Mbps`} />
            <Mesure libelle="Aller-retour" valeur={`${Math.round(partage.rttMs)} ms`} />
          </dl>
        </section>
      );
    case "demande":
      return (
        <section aria-label="Partage" className={cadre}>
          <h2 className="font-titre text-2xl">Demande envoyée à {partage.ami}</h2>
          {/* `ATTENTE_SPECTATEUR` = 60 s (spec D3). Sans l'annoncer, une minute
              d'attente est indiscernable d'un gel. */}
          <p className="text-texte-2">J'attends sa réponse, 60 s au plus…</p>
          {arreter}
        </section>
      );
    case "regarde":
      return (
        <section aria-label="Partage" className={cadre}>
          <h2 className="font-titre text-2xl">
            Connecté en {decimale(partage.connecteEnS)} s · connexion directe, sans relais
          </h2>
          {/* Spec D2, « le panneau le dit en toutes lettres » : sans cette
              phrase, l'absence d'image passe pour une panne, et l'utilisateur
              cherche un défaut qui n'existe pas. La seconde moitié est une
              promesse de vie privée, au même titre que « aucune adresse
              journalisée » : le flux reçu est mesuré puis JETÉ. */}
          <p className="text-texte-2">
            L'image n'est pas encore affichée : elle arrive au jalon 2. SkyShare mesure la connexion,
            puis jette la vidéo reçue — rien n'est écrit sur le disque.
          </p>
          <dl className="flex gap-8">
            <Mesure libelle="Débit reçu" valeur={`${decimale(partage.debitMbps)} Mbps`} />
            {/* `imagesParS` est un entier côté cœur (`images_par_s: u32`) : pas
                de décimale à séparer. */}
            <Mesure libelle="Cadence" valeur={`${partage.imagesParS} images/s`} />
            <Mesure libelle="Gigue" valeur={`${decimale(partage.gigueMs)} ms`} />
            <Mesure libelle="Durée" valeur={duree(maintenant - partage.depuisMs)} />
          </dl>
          {arreter}
        </section>
      );
    case "termine":
      return (
        <section aria-label="Partage" className={cadre}>
          {/* `role="status"` et non `alert` : la fin d'un partage est une
              information, pas une urgence à interrompre la lecture. */}
          <p role="status">{messageDeFin(partage.fin)}</p>
        </section>
      );
  }
}
