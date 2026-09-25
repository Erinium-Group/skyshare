import { useState } from "react";
import { BarrePartage } from "./composants/BarrePartage";
import { PanneauPartage } from "./composants/PanneauPartage";
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
  // `BarrePartage` va dans `bas` — la barre latérale — et JAMAIS dans les
  // enfants : c'est ce qui garde l'état d'un partage visible quel que soit
  // l'écran choisi (arbitrage 3, « on ne partage jamais sans le savoir »). Le
  // `PanneauPartage`, lui, est le détail de l'écran courant, en tête.
  return (
    <Disposition
      ecran={ecran}
      choisir={setEcran}
      bas={<BarrePartage instantane={instantane} />}
    >
      <PanneauPartage instantane={instantane} />
      {ecran === "amis" && <Amis instantane={instantane} />}
      {ecran === "listes" && <Listes instantane={instantane} />}
      {ecran === "compte" && <MonCompte instantane={instantane} />}
    </Disposition>
  );
}
