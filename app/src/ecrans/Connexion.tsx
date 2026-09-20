import { useState } from "react";
import { pont } from "../pont";
import type { Connexion as EtatConnexion } from "../types";

/** Premier lancement ou session expirée (spec §4). */
export function Connexion({ connexion }: { connexion: EtatConnexion }) {
  const [erreur, setErreur] = useState<string | null>(null);
  // M3 : un CHANGEMENT d'état venu du cœur périme le refus affiché. Sans cela,
  // « Impossible pendant un partage » resterait à l'écran une fois le partage
  // fini, en travers d'un bouton redevenu utilisable. Comparer à l'état
  // précédent, et non tester l'état courant : sinon un refus reçu alors que la
  // session est déjà expirée s'effacerait aussitôt, avant d'être lu.
  const [connexionVue, setConnexionVue] = useState(connexion);
  const enCours = connexion === "en_cours";

  let refus = erreur;
  if (connexionVue !== connexion) {
    setConnexionVue(connexion);
    setErreur(null);
    refus = null;
  }

  // M3 : UNE SEULE alerte à la fois. Deux `role="alert"` simultanés se
  // bousculent chez un lecteur d'écran, et le plus récent — le refus du clic —
  // est celui qui répond à ce que l'utilisateur vient de faire ; « session
  // expirée » ne fait que rappeler pourquoi il est là.
  const alerte = refus ?? (connexion === "session_expiree" ? "Session expirée — reconnecte-toi" : null);

  return (
    <main className="flex h-screen flex-col items-center justify-center gap-6 bg-fond font-corps text-texte">
      <h1 className="font-titre text-5xl">SkyShare</h1>
      {alerte && (
        <p role="alert" className="text-alerte">
          {alerte}
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
    </main>
  );
}
