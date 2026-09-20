import { useState } from "react";
import { Disposition, type Ecran } from "./Disposition";

const TITRES: Record<Ecran, string> = { amis: "Amis", listes: "Listes", compte: "Mon compte" };

export function App() {
  const [ecran, setEcran] = useState<Ecran>("amis");
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
      <h1 className="font-titre text-4xl">{TITRES[ecran]}</h1>
    </Disposition>
  );
}
