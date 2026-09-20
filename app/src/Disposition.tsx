import type { ReactNode } from "react";

export type Ecran = "amis" | "listes" | "compte";

const ENTREES: { ecran: Ecran; libelle: string }[] = [
  { ecran: "amis", libelle: "Amis" },
  { ecran: "listes", libelle: "Listes" },
  { ecran: "compte", libelle: "Mon compte" },
];

/** Barre latérale à gauche, bouton de partage toujours visible en bas (spec D6). */
export function Disposition(props: {
  ecran: Ecran;
  choisir: (ecran: Ecran) => void;
  bas: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex h-screen bg-fond font-corps text-texte">
      <nav aria-label="Navigation" className="flex w-60 flex-col border-r border-bordure bg-surface p-4">
        <p className="mb-6 font-titre text-3xl">SkyShare</p>
        <ul className="flex flex-col gap-1">
          {ENTREES.map((entree) => (
            <li key={entree.ecran}>
              <button
                type="button"
                aria-current={props.ecran === entree.ecran ? "page" : undefined}
                className={`w-full rounded-md px-3 py-2 text-left ${
                  props.ecran === entree.ecran ? "bg-surface-haute text-texte" : "text-texte-2 hover:bg-surface-haute"
                }`}
                onClick={() => props.choisir(entree.ecran)}
              >
                {entree.libelle}
              </button>
            </li>
          ))}
        </ul>
        <div className="mt-auto">{props.bas}</div>
      </nav>
      <main className="flex-1 overflow-y-auto p-8">{props.children}</main>
    </div>
  );
}
