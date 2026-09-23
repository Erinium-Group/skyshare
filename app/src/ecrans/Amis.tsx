import { useState } from "react";
import { useActions, type Executer } from "../actions";
import { pont } from "../pont";
import type { AmiVue, Instantane } from "../types";

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
  const { executer, enVol, message } = useActions();
  const occupe = partageEnCours(instantane);

  /** Ajoute l'ami, et ne vide le champ QUE si le cœur a accepté. */
  const ajouter = async () => {
    let accepte = false;
    await executer(async () => {
      const retour = await pont.ajouterAmi(code);
      accepte = true;
      return retour;
    });
    if (accepte) setCode("");
  };

  return (
    <section className="flex max-w-2xl flex-col gap-6">
      <h1 className="font-titre text-4xl">Amis</h1>
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void ajouter();
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
        <button type="submit" disabled={enVol} className="rounded-md bg-accent px-4 py-2 text-fond disabled:opacity-50">
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
                  disabled={enVol}
                  className="rounded-md bg-surface-haute px-3 py-1 disabled:opacity-40"
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
          <LigneAmi key={ami.friendshipId} ami={ami} occupe={occupe} enVol={enVol} executer={executer} />
        ))}
      </ul>
      {instantane.amis.length === 0 && (
        <p className="text-texte-3">Aucun ami pour l'instant : ton code ami est dans Mon compte.</p>
      )}
    </section>
  );
}

function LigneAmi({
  ami,
  occupe,
  enVol,
  executer,
}: {
  ami: AmiVue;
  occupe: boolean;
  enVol: boolean;
  executer: Executer;
}) {
  const [menu, setMenu] = useState(false);
  const sansAppareil = ami.appareils === 0;
  // M3 : l'explication ne peut PAS vivre dans un `title` — il est posé sur un
  // bouton désactivé, donc ni focusable ni survolable, et invisible sans
  // souris. Elle est donc écrite dans la ligne, et rattachée au bouton par
  // `aria-describedby` pour qu'un lecteur d'écran l'annonce avec lui.
  const idExplication = `sans-appareil-${ami.friendshipId}`;

  /** Toute action du menu le referme : il a fait son office (M3). */
  const depuisLeMenu = (action: () => Promise<string | void>) => {
    setMenu(false);
    void executer(action);
  };

  return (
    <li className="flex items-center gap-3 rounded-md bg-surface px-3 py-2">
      <span className="flex-1">{ami.nom}</span>
      <span className="text-texte-3">
        {ami.appareils} appareil{ami.appareils > 1 ? "s" : ""}
      </span>
      {sansAppareil && (
        <span id={idExplication} className="text-texte-3">
          {ami.nom} n'a aucun appareil enregistré
        </span>
      )}
      <button
        type="button"
        disabled={sansAppareil || occupe || enVol}
        aria-describedby={sansAppareil ? idExplication : undefined}
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
        // Échap referme, et un clic hors du menu aussi : `onBlur` capturant,
        // qui ne se déclenche que si le focus quitte le groupe entier.
        <span
          className="flex gap-2"
          onKeyDown={(e) => {
            if (e.key === "Escape") setMenu(false);
          }}
          onBlur={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget)) setMenu(false);
          }}
        >
          <button
            type="button"
            disabled={enVol}
            className="rounded-md bg-surface-haute px-3 py-1 disabled:opacity-40"
            onClick={() => depuisLeMenu(() => pont.retirerAmi(ami.friendshipId))}
          >
            Retirer
          </button>
          <button
            type="button"
            disabled={enVol}
            className="rounded-md bg-surface-haute px-3 py-1 text-alerte disabled:opacity-40"
            onClick={() => depuisLeMenu(() => pont.bloquerAmi(ami.friendshipId))}
          >
            Bloquer
          </button>
        </span>
      )}
    </li>
  );
}
