import { useState } from "react";
import { Disposition, type Ecran } from "./Disposition";
import { Amis } from "./ecrans/Amis";
import { Connexion } from "./ecrans/Connexion";
import { Listes } from "./ecrans/Listes";
import { MonCompte } from "./ecrans/MonCompte";
import { useInstantane } from "./useInstantane";

export function App() {
  const etat = useInstantane();
  const [ecran, setEcran] = useState<Ecran>("amis");
  // L'échec AVANT l'attente : les confondre rendrait un rejet indiscernable
  // d'un chargement qui n'aboutit pas (ronde de correction 1, I1).
  if (etat.phase === "echec") {
    return (
      <p role="alert" className="p-8 text-alerte">
        SkyShare n'a pas pu lire son état : {etat.message}
      </p>
    );
  }
  if (etat.phase === "chargement") return <p className="p-8 text-texte-2">Chargement…</p>;
  const instantane = etat.instantane;
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
      {ecran === "amis" && <Amis instantane={instantane} />}
      {ecran === "listes" && <Listes instantane={instantane} />}
      {ecran === "compte" && <MonCompte instantane={instantane} />}
    </Disposition>
  );
}
