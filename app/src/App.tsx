import { useState } from "react";
import { Disposition, type Ecran } from "./Disposition";
import { Amis } from "./ecrans/Amis";
import { Connexion } from "./ecrans/Connexion";
import { useInstantane } from "./useInstantane";

const TITRES: Record<Ecran, string> = { amis: "Amis", listes: "Listes", compte: "Mon compte" };

export function App() {
  const instantane = useInstantane();
  const [ecran, setEcran] = useState<Ecran>("amis");
  if (instantane === null) return <p className="p-8 text-texte-2">Chargement…</p>;
  // `!== "connecte"` et non `=== "deconnecte"` : `en_cours` et
  // `session_expiree` n'ont pas de session utilisable non plus.
  if (instantane.connexion !== "connecte") return <Connexion connexion={instantane.connexion} />;
  return (
    <Disposition
      ecran={ecran}
      choisir={setEcran}
      bas={
        <button type="button" disabled className="w-full rounded-md bg-accent px-3 py-2 text-fond disabled:opacity-50">
          Partager mon écran
        </button>
      }
    >
      {ecran === "amis" ? <Amis instantane={instantane} /> : <h1 className="font-titre text-4xl">{TITRES[ecran]}</h1>}
    </Disposition>
  );
}
