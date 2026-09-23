import { useActions } from "../actions";
import { BoutonDestructeur } from "../BoutonDestructeur";
import { pont } from "../pont";
import type { Instantane } from "../types";

/** « ABCD2345 » → « SKY-ABCD-2345 », la forme que le site normalise (`normaliserCode`). */
export function codeAffiche(code: string | null): string {
  return code ? `SKY-${code.slice(0, 4)}-${code.slice(4)}` : "—";
}

export function MonCompte({ instantane }: { instantane: Instantane }) {
  const { executer, enVol, message } = useActions();

  return (
    <section className="flex max-w-2xl flex-col gap-6">
      <h1 className="font-titre text-4xl">Mon compte</h1>
      <p>
        Connecté en tant que <strong>{instantane.nom ?? "…"}</strong>
      </p>

      <section aria-label="Code ami" className="flex items-center gap-3">
        <span className="text-texte-2">Ton code ami</span>
        <code className="rounded-md bg-surface px-3 py-1">{codeAffiche(instantane.code)}</code>
        <button
          type="button"
          className="rounded-md bg-surface-haute px-3 py-1"
          onClick={() =>
            void executer(async () => {
              await navigator.clipboard.writeText(codeAffiche(instantane.code));
              return "Code copié.";
            })
          }
        >
          Copier
        </button>
        {/* Régénérer n'est pas destructeur au sens de l'arbitrage — rien n'est
            perdu, les amis déjà acceptés restent — mais l'ancien code cesse de
            fonctionner : le message du cœur le dit en toutes lettres. */}
        <button
          type="button"
          disabled={enVol}
          className="rounded-md bg-surface-haute px-3 py-1 disabled:opacity-40"
          onClick={() => void executer(() => pont.regenererCode())}
        >
          Régénérer
        </button>
      </section>

      <section aria-label="Appareils" className="flex flex-col gap-2">
        <h2 className="text-texte-2">Appareils</h2>
        <ul className="flex flex-col gap-2">
          {instantane.appareils.map((appareil) => (
            <li
              key={appareil.id}
              className="flex items-center gap-3 rounded-md bg-surface px-3 py-2"
            >
              <span className="flex-1">{appareil.nom}</span>
              {appareil.courant ? (
                // Le révoquer révoquerait la session de CETTE machine : le cœur
                // le refuse (`MESSAGE_APPAREIL_COURANT`), et l'écran ne propose
                // même pas le geste. Les deux, comme ailleurs dans ce projet :
                // un bouton absent ne protège rien si la commande reste
                // appelable, et un refus du cœur seul laisserait croire à un
                // bogue.
                <span className="text-succes">cet appareil</span>
              ) : appareil.revoque ? (
                <span className="text-texte-3">révoqué</span>
              ) : (
                <BoutonDestructeur
                  libelle="Révoquer"
                  confirmer={`Révoquer définitivement ${appareil.nom}`}
                  enVol={enVol}
                  className="rounded-md bg-surface-haute px-3 py-1 text-alerte disabled:opacity-40"
                  agir={() => void executer(() => pont.revoquerAppareil(appareil.id))}
                />
              )}
            </li>
          ))}
        </ul>
        {instantane.appareils.length === 0 && (
          <p className="text-texte-3">Aucun appareil enregistré pour l'instant.</p>
        )}
        {/* Spec §4 : le nom d'un appareil n'est pas modifiable ici. Aucune route
            du site ne le renomme (`devices/route.ts` n'a que POST,
            `devices/[id]/route.ts` que DELETE) : reporté, et signalé. */}
      </section>

      <label className="flex items-center gap-2">
        <input
          type="checkbox"
          checked={instantane.demarrageAutomatique}
          disabled={enVol}
          // L'INVERSE de ce qui est affiché, pas un état local : la case suit
          // l'instantané, et ne bouge qu'une fois que la coquille a bien écrit
          // dans le registre de Windows.
          onChange={() =>
            void executer(() => pont.demarrageAutomatique(!instantane.demarrageAutomatique))
          }
        />
        Lancer SkyShare au démarrage de Windows
      </label>

      <p className="text-texte-3">
        Tant que SkyShare tourne, n'utilise pas <code>sky-probe</code> sur cette machine : il lui
        volerait les demandes de partage.
      </p>

      <button
        type="button"
        disabled={enVol}
        className="self-start rounded-md px-4 py-2 text-alerte disabled:opacity-40"
        onClick={() => void executer(() => pont.deconnexion())}
      >
        Se déconnecter
      </button>

      {message && (
        <p role="status" className="text-texte-2">
          {message}
        </p>
      )}
    </section>
  );
}
