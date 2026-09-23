import { useState } from "react";
import { useActions, type Executer } from "../actions";
import { BoutonDestructeur } from "../BoutonDestructeur";
import { pont } from "../pont";
import type { AmiVue, Instantane, ListeVue } from "../types";

/**
 * Couleur par défaut d'une liste : l'accent de la charte. `<input type="color">`
 * n'accepte QUE du minuscule et rend du minuscule ; le site accepte les deux
 * casses (`couleurValide`).
 */
const COULEUR_PAR_DEFAUT = "#c4664a";

export function Listes({ instantane }: { instantane: Instantane }) {
  const [choisie, setChoisie] = useState<number | "nouvelle" | null>(null);
  const { executer, enVol, message } = useActions();
  // Relue dans l'instantané À CHAQUE RENDU, jamais recopiée dans un état : une
  // synchronisation qui arrive pendant l'édition doit être vue, et une liste
  // supprimée ailleurs ne doit pas rester ouverte.
  const liste =
    typeof choisie === "number" ? (instantane.listes.find((l) => l.id === choisie) ?? null) : null;

  return (
    <section className="flex flex-col gap-4">
      <h1 className="font-titre text-4xl">Listes</h1>
      <p className="text-texte-3">
        Les listes ne filtrent pas encore les partages : tous tes amis peuvent te demander à
        regarder.
      </p>
      <div className="flex gap-6">
        <ul aria-label="Mes listes" className="flex w-64 flex-col gap-1">
          {instantane.listes.map((l) => (
            <li key={l.id}>
              <button
                type="button"
                aria-current={choisie === l.id ? "true" : undefined}
                onClick={() => setChoisie(l.id)}
                className="flex w-full items-center gap-2 rounded-md px-3 py-2 hover:bg-surface-haute"
              >
                <span
                  aria-hidden="true"
                  className="h-3 w-3 rounded-full"
                  style={{ background: l.couleur ?? "transparent" }}
                />
                <span aria-hidden="true">{l.emoji}</span>
                <span className="flex-1 text-left">{l.nom}</span>
                <span className="text-texte-3">
                  {l.membres.length} membre{l.membres.length > 1 ? "s" : ""}
                </span>
              </button>
            </li>
          ))}
          <li>
            <button
              type="button"
              onClick={() => setChoisie("nouvelle")}
              className="px-3 py-2 text-accent"
            >
              Nouvelle liste
            </button>
          </li>
        </ul>
        {(choisie === "nouvelle" || liste !== null) && (
          // `key` : changer de liste REMONTE l'éditeur, donc repart des champs
          // de la liste choisie. Sans elle, React garderait l'état de saisie de
          // la précédente et on enregistrerait son nom sur celle-ci.
          <EditeurListe
            key={liste?.id ?? "nouvelle"}
            liste={liste}
            amis={instantane.amis}
            executer={executer}
            enVol={enVol}
            fermer={() => setChoisie(null)}
          />
        )}
      </div>
      {message && (
        <p role="status" className="text-texte-2">
          {message}
        </p>
      )}
    </section>
  );
}

function EditeurListe(props: {
  liste: ListeVue | null;
  amis: AmiVue[];
  executer: Executer;
  enVol: boolean;
  fermer: () => void;
}) {
  const { liste } = props;
  const [nom, setNom] = useState(liste?.nom ?? "");
  const [couleur, setCouleur] = useState((liste?.couleur ?? COULEUR_PAR_DEFAUT).toLowerCase());
  const [emoji, setEmoji] = useState(liste?.emoji ?? "");
  const [membres, setMembres] = useState<Set<number>>(new Set(liste?.membres ?? []));

  function basculer(id: number) {
    setMembres((avant) => {
      const apres = new Set(avant);
      if (apres.has(id)) apres.delete(id);
      else apres.add(id);
      return apres;
    });
  }

  /**
   * Le site refuse un nom vide (`nomValide` : 1 à 40 unités UTF-16), et le cœur
   * le refuse avant lui. Le bouton le dit AVANT le clic plutôt que de laisser
   * partir une requête dont on connaît déjà le refus. `=== ""` et non `.trim()`
   * : « &nbsp; » fait un caractère, et le site l'accepte — refuser plus que lui
   * serait inventer une règle.
   */
  const nomVide = nom === "";

  async function enregistrer() {
    const emojiEnvoye = emoji === "" ? null : emoji;
    if (liste) {
      await props.executer(async () => {
        await pont.modifierListe(liste.id, nom, couleur, emojiEnvoye);
        // Les membres sont une SECONDE route (`PUT .../members`) : elle ne part
        // qu'après l'acceptation de la première, et c'est son message qui
        // s'affiche — c'est le dernier mot du cœur sur l'enregistrement.
        // Triés : l'ordre d'un `Set` suit les clics, et un envoi reproductible
        // vaut mieux qu'un envoi au hasard.
        return await pont.definirMembres(liste.id, [...membres].sort((a, b) => a - b));
      });
      return;
    }
    let cree = false;
    await props.executer(async () => {
      const retour = await pont.creerListe(nom, couleur, emojiEnvoye);
      cree = true;
      return retour;
    });
    // Comme le champ code ami d'`Amis` : on ne referme QUE si le cœur a accepté,
    // sinon une saisie refusée pour une raison corrigeable serait perdue.
    if (cree) props.fermer();
  }

  function supprimer() {
    if (!liste) return;
    void props.executer(async () => {
      const retour = await pont.supprimerListe(liste.id);
      props.fermer();
      return retour;
    });
  }

  return (
    <form
      aria-label="Édition de la liste"
      className="flex flex-1 flex-col gap-3 rounded-md bg-surface p-4"
      onSubmit={(e) => {
        e.preventDefault();
        void enregistrer();
      }}
    >
      <label className="flex flex-col gap-1">
        Nom
        <input
          value={nom}
          onChange={(e) => setNom(e.target.value)}
          maxLength={40}
          className="rounded-md border border-bordure bg-fond px-3 py-2"
        />
      </label>
      <label className="flex items-center gap-2">
        Couleur
        <input type="color" value={couleur} onChange={(e) => setCouleur(e.target.value)} />
      </label>
      <label className="flex flex-col gap-1">
        Émoji
        <input
          value={emoji}
          onChange={(e) => setEmoji(e.target.value)}
          className="rounded-md border border-bordure bg-fond px-3 py-2"
        />
      </label>
      {liste ? (
        <fieldset className="flex flex-col gap-1">
          <legend className="text-texte-2">Membres</legend>
          {props.amis.map((ami) => (
            <label key={ami.id} className="flex items-center gap-2">
              {/* `ami.id` : identifiant d'UTILISATEUR, c'est ce que lit
                  `PUT /lists/{id}/members`. `friendshipId` y serait refusé. */}
              <input type="checkbox" checked={membres.has(ami.id)} onChange={() => basculer(ami.id)} />
              {ami.nom}
            </label>
          ))}
          {props.amis.length === 0 && (
            <p className="text-texte-3">Aucun ami à mettre dans cette liste pour l'instant.</p>
          )}
        </fieldset>
      ) : (
        <p className="text-texte-3">
          Les membres se choisissent une fois la liste créée : le site les attache à une liste qui
          existe.
        </p>
      )}
      <div className="flex items-center gap-2">
        <button
          type="submit"
          disabled={nomVide || props.enVol}
          className="rounded-md bg-accent px-4 py-2 text-fond disabled:opacity-50"
        >
          Enregistrer
        </button>
        {liste && (
          <BoutonDestructeur
            libelle="Supprimer"
            confirmer={`Supprimer définitivement « ${liste.nom} »`}
            enVol={props.enVol}
            agir={supprimer}
          />
        )}
      </div>
    </form>
  );
}
