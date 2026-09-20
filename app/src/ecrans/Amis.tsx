import { useState } from "react";
import { pont } from "../pont";
import type { AmiVue, Instantane } from "../types";

type Executer = (action: () => Promise<string | void>) => Promise<void>;

/**
 * Un partage ou une attente en cours : on ne regarde personne en même temps.
 *
 * `termine` n'en est PAS un — c'est l'écran de fin, que l'utilisateur n'a pas
 * encore refermé ; rien ne tourne plus, et lui interdire de regarder un ami à
 * ce moment-là n'aurait aucune cause.
 */
function partageEnCours(instantane: Instantane): boolean {
  return instantane.partage.etat !== "inactif" && instantane.partage.etat !== "termine";
}

export function Amis({ instantane }: { instantane: Instantane }) {
  const [code, setCode] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const occupe = partageEnCours(instantane);

  /**
   * Toute action passe par ici : le cœur rend soit un message de réussite,
   * soit un refus DÉJÀ borné par `detail_borne` (`noyau.rs`). L'interface
   * l'affiche tel quel et n'en fabrique aucun — elle ne sait rien du site.
   */
  const executer: Executer = async (action) => {
    try {
      const retour = await action();
      setMessage(typeof retour === "string" ? retour : null);
    } catch (erreur) {
      setMessage(String(erreur));
    }
  };

  return (
    <section className="flex max-w-2xl flex-col gap-6">
      <h1 className="font-titre text-4xl">Amis</h1>
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void executer(() => pont.ajouterAmi(code));
        }}
      >
        <label htmlFor="code-ami" className="sr-only">
          Code ami
        </label>
        <input
          id="code-ami"
          value={code}
          onChange={(e) => setCode(e.target.value)}
          placeholder="SKY-ABCD-EFGH"
          className="flex-1 rounded-md border border-bordure bg-surface px-3 py-2"
        />
        <button type="submit" className="rounded-md bg-accent px-4 py-2 text-fond">
          Ajouter
        </button>
      </form>
      {message && (
        <p role="status" className="text-texte-2">
          {message}
        </p>
      )}
      {instantane.demandes.length > 0 && (
        <section aria-label="Demandes reçues" className="flex flex-col gap-2">
          <h2 className="text-texte-2">Demandes reçues</h2>
          <ul className="flex flex-col gap-2">
            {instantane.demandes.map((demande) => (
              <li
                key={demande.friendshipId}
                className="flex items-center justify-between rounded-md bg-surface px-3 py-2"
              >
                <span>{demande.nom}</span>
                <button
                  type="button"
                  className="rounded-md bg-surface-haute px-3 py-1"
                  onClick={() => void executer(() => pont.accepterAmi(demande.friendshipId))}
                >
                  Accepter
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      <ul aria-label="Mes amis" className="flex flex-col gap-2">
        {instantane.amis.map((ami) => (
          <LigneAmi key={ami.friendshipId} ami={ami} occupe={occupe} executer={executer} />
        ))}
      </ul>
      {instantane.amis.length === 0 && (
        <p className="text-texte-3">Aucun ami pour l'instant : ton code ami est dans Mon compte.</p>
      )}
    </section>
  );
}

function LigneAmi({ ami, occupe, executer }: { ami: AmiVue; occupe: boolean; executer: Executer }) {
  const [menu, setMenu] = useState(false);
  const sansAppareil = ami.appareils === 0;
  return (
    <li className="flex items-center gap-3 rounded-md bg-surface px-3 py-2">
      <span className="flex-1">{ami.nom}</span>
      <span className="text-texte-3">
        {ami.appareils} appareil{ami.appareils > 1 ? "s" : ""}
      </span>
      <button
        type="button"
        disabled={sansAppareil || occupe}
        title={sansAppareil ? `${ami.nom} n'a aucun appareil enregistré` : undefined}
        className="rounded-md bg-surface-haute px-3 py-1 disabled:opacity-40"
        onClick={() => void executer(() => pont.regarder(ami.id))}
      >
        Regarder
      </button>
      <button
        type="button"
        aria-label={`Plus d'actions pour ${ami.nom}`}
        aria-expanded={menu}
        className="rounded-md px-2 py-1 text-texte-2 hover:bg-surface-haute"
        onClick={() => setMenu(!menu)}
      >
        …
      </button>
      {menu && (
        <span className="flex gap-2">
          <button
            type="button"
            className="rounded-md bg-surface-haute px-3 py-1"
            onClick={() => void executer(() => pont.retirerAmi(ami.friendshipId))}
          >
            Retirer
          </button>
          <button
            type="button"
            className="rounded-md bg-surface-haute px-3 py-1 text-alerte"
            onClick={() => void executer(() => pont.bloquerAmi(ami.friendshipId))}
          >
            Bloquer
          </button>
        </span>
      )}
    </li>
  );
}
