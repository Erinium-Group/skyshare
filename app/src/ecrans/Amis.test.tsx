import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { pont } from "../pont";
import { instantaneDeTest } from "../test/fabriques";
import { Amis } from "./Amis";

vi.mock("../pont", () => ({
  pont: {
    ajouterAmi: vi.fn(),
    accepterAmi: vi.fn(),
    retirerAmi: vi.fn(),
    bloquerAmi: vi.fn(),
    regarder: vi.fn(),
  },
}));

beforeEach(() => vi.clearAllMocks());

const ALICE = { id: 1, friendshipId: 11, nom: "Alice", appareils: 0 };
const BOB = { id: 2, friendshipId: 12, nom: "Bob", appareils: 1 };

function ligne(nom: string) {
  return within(screen.getByText(nom).closest("li")!);
}

describe("Amis", () => {
  it("Regarder est inactif pour un ami sans appareil, actif avec un appareil", () => {
    // Spec §4 et §8. Neutralisation : retirer `sansAppareil ||` du `disabled`.
    render(<Amis instantane={instantaneDeTest({ amis: [ALICE, BOB] })} />);
    expect(ligne("Alice").getByRole("button", { name: "Regarder" })).toBeDisabled();
    expect(ligne("Bob").getByRole("button", { name: "Regarder" })).toBeEnabled();
  });

  it("Regarder est inactif pendant un partage", () => {
    // Neutralisation : retirer `|| occupe` du `disabled`.
    render(
      <Amis
        instantane={instantaneDeTest({
          amis: [BOB],
          partage: { etat: "disponible", debutMs: 0, fenetreS: 1800, ecran: 0 },
        })}
      />,
    );
    expect(ligne("Bob").getByRole("button", { name: "Regarder" })).toBeDisabled();
  });

  it("un partage terminé ne bloque plus Regarder", () => {
    // Contrôle positif du test précédent : sans lui, un `disabled` permanent
    // le satisferait. `termine` n'est pas un partage en cours — c'est l'écran
    // de fin, que l'utilisateur n'a pas encore refermé.
    render(
      <Amis
        instantane={instantaneDeTest({ amis: [BOB], partage: { etat: "termine", fin: { cause: "arrete" } } })}
      />,
    );
    expect(ligne("Bob").getByRole("button", { name: "Regarder" })).toBeEnabled();
  });

  it("le nombre d'appareils s'accorde en nombre", () => {
    render(
      <Amis instantane={instantaneDeTest({ amis: [ALICE, BOB, { id: 3, friendshipId: 13, nom: "Carole", appareils: 2 }] })} />,
    );
    expect(ligne("Alice").getByText("0 appareil")).toBeInTheDocument();
    expect(ligne("Bob").getByText("1 appareil")).toBeInTheDocument();
    expect(ligne("Carole").getByText("2 appareils")).toBeInTheDocument();
  });

  it("accepter et retirer envoient l'identifiant d'AMITIÉ, regarder celui de l'utilisateur", async () => {
    // Les routes friends/{id} lisent un friendshipId (T3). Neutralisation :
    // passer `ami.id` à `retirerAmi` — appelé avec 2 au lieu de 12.
    vi.mocked(pont.accepterAmi).mockResolvedValue("Demande acceptée.");
    vi.mocked(pont.retirerAmi).mockResolvedValue("Ami retiré.");
    vi.mocked(pont.regarder).mockResolvedValue(undefined);
    render(<Amis instantane={instantaneDeTest({ amis: [BOB], demandes: [{ friendshipId: 55, nom: "Carole" }] })} />);
    await userEvent.click(screen.getByRole("button", { name: "Accepter" }));
    expect(pont.accepterAmi).toHaveBeenCalledWith(55);
    await userEvent.click(ligne("Bob").getByRole("button", { name: "Regarder" }));
    expect(pont.regarder).toHaveBeenCalledWith(2);
    await userEvent.click(screen.getByRole("button", { name: "Plus d'actions pour Bob" }));
    await userEvent.click(screen.getByRole("button", { name: "Retirer" }));
    expect(pont.retirerAmi).toHaveBeenCalledWith(12);
    expect(await screen.findByRole("status")).toHaveTextContent("Ami retiré.");
  });

  it("bloquer envoie aussi l'identifiant d'amitié", async () => {
    vi.mocked(pont.bloquerAmi).mockResolvedValue("Ami bloqué.");
    render(<Amis instantane={instantaneDeTest({ amis: [BOB] })} />);
    await userEvent.click(screen.getByRole("button", { name: "Plus d'actions pour Bob" }));
    await userEvent.click(screen.getByRole("button", { name: "Bloquer" }));
    expect(pont.bloquerAmi).toHaveBeenCalledWith(12);
    expect(await screen.findByRole("status")).toHaveTextContent("Ami bloqué.");
  });

  it("un refus du cœur s'affiche tel quel", async () => {
    vi.mocked(pont.ajouterAmi).mockRejectedValue("Code ami introuvable.");
    render(<Amis instantane={instantaneDeTest()} />);
    await userEvent.type(screen.getByLabelText("Code ami"), "SKY-ABCD-EFGH");
    await userEvent.click(screen.getByRole("button", { name: "Ajouter" }));
    expect(pont.ajouterAmi).toHaveBeenCalledWith("SKY-ABCD-EFGH");
    expect(await screen.findByRole("status")).toHaveTextContent("Code ami introuvable.");
  });
});
