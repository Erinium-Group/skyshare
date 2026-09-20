import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("disposition", () => {
  it("la barre latérale mène aux trois écrans et garde le partage en bas", async () => {
    // Spec D6. Neutralisation : retirer l'entrée « Mon compte » de
    // Disposition — le test rougit sur cette entrée.
    render(<App />);
    const navigation = screen.getByRole("navigation", { name: "Navigation" });
    for (const nom of ["Amis", "Listes", "Mon compte"]) {
      expect(within(navigation).getByRole("button", { name: nom })).toBeInTheDocument();
    }
    expect(within(navigation).getByRole("button", { name: "Partager mon écran" })).toBeInTheDocument();
    await userEvent.click(within(navigation).getByRole("button", { name: "Listes" }));
    expect(screen.getByRole("heading", { name: "Listes" })).toBeInTheDocument();
  });
});
