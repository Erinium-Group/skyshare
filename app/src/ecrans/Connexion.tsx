import { useState } from "react";
import { pont } from "../pont";
import type { Connexion as EtatConnexion } from "../types";

/** Premier lancement ou session expirée (spec §4). */
export function Connexion({ connexion }: { connexion: EtatConnexion }) {
  const [erreur, setErreur] = useState<string | null>(null);
  const enCours = connexion === "en_cours";
  return (
    <main className="flex h-screen flex-col items-center justify-center gap-6 bg-fond font-corps text-texte">
      <h1 className="font-titre text-5xl">SkyShare</h1>
      {connexion === "session_expiree" && (
        <p role="alert" className="text-alerte">
          Session expirée — reconnecte-toi
        </p>
      )}
      <button
        type="button"
        disabled={enCours}
        className="rounded-md bg-accent px-5 py-3 text-fond disabled:opacity-50"
        onClick={() => {
          setErreur(null);
          // Le cœur refuse par exemple pendant un partage : sans cette
          // reprise, le clic resterait sans effet visible.
          pont.connexion().catch((e: unknown) => setErreur(String(e)));
        }}
      >
        Se connecter avec Discord
      </button>
      {enCours && <p className="text-texte-2">Termine la connexion dans ton navigateur, puis reviens ici.</p>}
      {erreur && (
        <p role="alert" className="text-alerte">
          {erreur}
        </p>
      )}
    </main>
  );
}
