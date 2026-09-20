import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { pont } from "../pont";
import { Connexion } from "./Connexion";

vi.mock("../pont", () => ({ pont: { connexion: vi.fn() } }));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(pont.connexion).mockResolvedValue(undefined);
});

describe("Connexion", () => {
  it("dit en clair que la session a expiré", () => {
    // Spec §4, quatrième échec. Neutralisation : retirer le paragraphe
    // `session_expiree` — le test rougit.
    render(<Connexion connexion="session_expiree" />);
    expect(screen.getByRole("alert")).toHaveTextContent("Session expirée — reconnecte-toi");
  });

  it("ne crie pas « session expirée » à un premier lancement", () => {
    // Contrôle positif du test précédent : sans lui, une alerte affichée en
    // permanence le satisferait aussi.
    render(<Connexion connexion="deconnecte" />);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("le bouton lance la connexion, et se désactive pendant qu'elle court", async () => {
    const { rerender } = render(<Connexion connexion="deconnecte" />);
    await userEvent.click(screen.getByRole("button", { name: "Se connecter avec Discord" }));
    expect(pont.connexion).toHaveBeenCalledTimes(1);
    rerender(<Connexion connexion="en_cours" />);
    expect(screen.getByRole("button", { name: "Se connecter avec Discord" })).toBeDisabled();
  });

  it("un refus du cœur s'affiche, et le bouton reste cliquable", async () => {
    // Le cœur refuse déjà pendant un partage (`MESSAGE_PENDANT_PARTAGE`) :
    // sans affichage, le clic resterait sans effet visible. Neutralisation :
    // retirer le `.catch` du `onClick` — rien ne s'affiche.
    vi.mocked(pont.connexion).mockRejectedValue("Impossible pendant un partage.");
    render(<Connexion connexion="deconnecte" />);
    await userEvent.click(screen.getByRole("button", { name: "Se connecter avec Discord" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Impossible pendant un partage.");
    expect(screen.getByRole("button", { name: "Se connecter avec Discord" })).toBeEnabled();
  });
});
