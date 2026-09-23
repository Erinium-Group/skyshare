import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { pont } from "../pont";
import { instantaneDeTest } from "../test/fabriques";
import { Listes } from "./Listes";

vi.mock("../pont", () => ({
  pont: {
    creerListe: vi.fn(),
    modifierListe: vi.fn(),
    supprimerListe: vi.fn(),
    definirMembres: vi.fn(),
  },
}));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(pont.creerListe).mockResolvedValue("Liste créée.");
  vi.mocked(pont.modifierListe).mockResolvedValue("Liste enregistrée.");
  vi.mocked(pont.supprimerListe).mockResolvedValue("Liste supprimée.");
  vi.mocked(pont.definirMembres).mockResolvedValue("Membres enregistrés.");
});

const AMIS = [
  { id: 2, friendshipId: 12, nom: "Bob", appareils: 1 },
  { id: 3, friendshipId: 13, nom: "Carole", appareils: 0 },
];
const JEU = { id: 7, nom: "Jeu", couleur: "#C4664A", emoji: "🎮", membres: [2] };

function edition() {
  return within(screen.getByRole("form", { name: "Édition de la liste" }));
}

describe("Listes", () => {
  it("dit que les listes ne filtrent pas encore les partages", () => {
    // Spec §3 : la promesse doit être tenue par l'écran, pas seulement par la
    // documentation. Neutralisation : retirer la phrase — ce test rougit.
    render(<Listes instantane={instantaneDeTest()} />);
    expect(screen.getByText(/ne filtrent pas encore les partages/)).toBeInTheDocument();
  });

  it("les cases cochées sont les membres, et l'enregistrement envoie des identifiants d'utilisateur", async () => {
    // Spec §5 : sans `membres` dans la synchronisation, aucune case ne serait
    // cochée. Neutralisations, une à la fois : (1) initialiser les cases à vide
    // (`new Set()`) — Bob n'est plus coché ; (2) envoyer `ami.friendshipId` au
    // lieu d'`ami.id` — l'appel devient [12, 13].
    render(<Listes instantane={instantaneDeTest({ amis: AMIS, listes: [JEU] })} />);
    await userEvent.click(screen.getByRole("button", { name: /Jeu/ }));
    expect(edition().getByRole("checkbox", { name: "Bob" })).toBeChecked();
    expect(edition().getByRole("checkbox", { name: "Carole" })).not.toBeChecked();
    await userEvent.click(edition().getByRole("checkbox", { name: "Carole" }));
    await userEvent.click(edition().getByRole("button", { name: "Enregistrer" }));
    expect(pont.modifierListe).toHaveBeenCalledWith(7, "Jeu", "#c4664a", "🎮");
    expect(pont.definirMembres).toHaveBeenCalledWith(7, [2, 3]);
    expect(await screen.findByRole("status")).toHaveTextContent("Membres enregistrés.");
  });

  it("Enregistrer reste inactif tant que le nom est vide", async () => {
    // Le site refuse un nom vide (`nomValide`, 1 à 40 unités UTF-16) et le cœur
    // avant lui : le dire AVANT le clic plutôt que d'envoyer une requête dont on
    // connaît le refus. Neutralisation : retirer `nomVide ||` du `disabled` —
    // le bouton devient actif et `creerListe` part avec un nom vide.
    render(<Listes instantane={instantaneDeTest()} />);
    await userEvent.click(screen.getByRole("button", { name: "Nouvelle liste" }));
    expect(edition().getByRole("button", { name: "Enregistrer" })).toBeDisabled();
    await userEvent.type(edition().getByLabelText("Nom"), "Jeu");
    expect(edition().getByRole("button", { name: "Enregistrer" })).toBeEnabled();
  });

  it("créer une liste envoie les trois champs, l'émoji vide devenant « aucun »", async () => {
    // `emoji: ""` n'est PAS `null` pour le site : la chaîne vide est acceptée et
    // stockée. Envoyer `null` est la seule façon de dire « aucun émoji ».
    // Neutralisation : envoyer `emoji` tel quel — l'appel devient "".
    render(<Listes instantane={instantaneDeTest()} />);
    await userEvent.click(screen.getByRole("button", { name: "Nouvelle liste" }));
    await userEvent.type(edition().getByLabelText("Nom"), "Jeu");
    await userEvent.click(edition().getByRole("button", { name: "Enregistrer" }));
    expect(pont.creerListe).toHaveBeenCalledWith("Jeu", "#c4664a", null);
    expect(await screen.findByRole("status")).toHaveTextContent("Liste créée.");
  });

  it("une liste qui n'existe pas encore n'offre pas de membres", () => {
    // Le site attache les membres à une liste EXISTANTE (`PUT
    // /lists/{id}/members`) : il n'y a pas d'identifiant à envoyer avant la
    // création. Neutralisation : afficher les cases aussi pour « nouvelle » —
    // elles apparaissent sans pouvoir être enregistrées.
    render(<Listes instantane={instantaneDeTest({ amis: AMIS })} />);
    expect(screen.queryByRole("form", { name: "Édition de la liste" })).toBeNull();
    expect(screen.getByRole("button", { name: "Nouvelle liste" })).toBeInTheDocument();
  });

  it("supprimer une liste demande une confirmation nommée", async () => {
    // ARBITRAGE DU CONTRÔLEUR : un geste qu'on ne rattrape pas ne part jamais du
    // premier clic. Neutralisation : appeler `supprimer` directement depuis le
    // bouton « Supprimer » — la première assertion rougit.
    render(<Listes instantane={instantaneDeTest({ amis: AMIS, listes: [JEU] })} />);
    await userEvent.click(screen.getByRole("button", { name: /Jeu/ }));
    await userEvent.click(edition().getByRole("button", { name: "Supprimer" }));
    expect(pont.supprimerListe).not.toHaveBeenCalled();
    await userEvent.click(
      edition().getByRole("button", { name: "Supprimer définitivement « Jeu »" }),
    );
    expect(pont.supprimerListe).toHaveBeenCalledWith(7);
  });

  it("annuler la confirmation ne supprime rien", async () => {
    // Contrôle positif du test précédent : sans lui, un bouton qui ne supprime
    // JAMAIS le satisferait aussi.
    render(<Listes instantane={instantaneDeTest({ amis: AMIS, listes: [JEU] })} />);
    await userEvent.click(screen.getByRole("button", { name: /Jeu/ }));
    await userEvent.click(edition().getByRole("button", { name: "Supprimer" }));
    await userEvent.click(edition().getByRole("button", { name: /^Annuler/ }));
    expect(pont.supprimerListe).not.toHaveBeenCalled();
    expect(edition().getByRole("button", { name: "Supprimer" })).toBeInTheDocument();
  });

  it("un refus du cœur s'affiche tel quel et l'éditeur reste ouvert", async () => {
    // Le message vient du cœur, DÉJÀ borné par `detail_borne` : l'interface n'en
    // fabrique aucun. Et une saisie refusée n'est pas jetée. Neutralisation :
    // appeler `props.fermer()` sans regarder l'issue — l'éditeur disparaît.
    vi.mocked(pont.creerListe).mockRejectedValue("Une liste porte déjà ce nom.");
    render(<Listes instantane={instantaneDeTest()} />);
    await userEvent.click(screen.getByRole("button", { name: "Nouvelle liste" }));
    await userEvent.type(edition().getByLabelText("Nom"), "Jeu");
    await userEvent.click(edition().getByRole("button", { name: "Enregistrer" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Une liste porte déjà ce nom.");
    expect(edition().getByLabelText("Nom")).toHaveValue("Jeu");
  });

  it("les membres ne partent pas si la modification a été refusée", async () => {
    // Les deux routes sont distinctes : enchaîner `definirMembres` sur une
    // modification refusée écrirait des membres sur une liste qu'on n'a pas pu
    // renommer. Neutralisation : sortir `definirMembres` de la continuation de
    // `modifierListe` (deux `await` indépendants) — il part quand même.
    vi.mocked(pont.modifierListe).mockRejectedValue("Cette liste n'existe plus.");
    render(<Listes instantane={instantaneDeTest({ amis: AMIS, listes: [JEU] })} />);
    await userEvent.click(screen.getByRole("button", { name: /Jeu/ }));
    await userEvent.click(edition().getByRole("button", { name: "Enregistrer" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Cette liste n'existe plus.");
    expect(pont.definirMembres).not.toHaveBeenCalled();
  });

  it("le nombre de membres s'accorde en nombre dans la liste de gauche", () => {
    render(
      <Listes
        instantane={instantaneDeTest({
          listes: [JEU, { id: 8, nom: "Boulot", couleur: null, emoji: null, membres: [2, 3] }],
        })}
      />,
    );
    const mes = within(screen.getByRole("list", { name: "Mes listes" }));
    expect(mes.getByText("1 membre")).toBeInTheDocument();
    expect(mes.getByText("2 membres")).toBeInTheDocument();
  });
});
