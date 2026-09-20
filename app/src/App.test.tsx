import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { pont } from "./pont";
import { instantaneDeTest } from "./test/fabriques";

vi.mock("./pont", () => ({ pont: { etatCourant: vi.fn(), ecouterEtat: vi.fn(), connexion: vi.fn() } }));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(pont.etatCourant).mockResolvedValue(instantaneDeTest());
  vi.mocked(pont.ecouterEtat).mockResolvedValue(() => {});
});

describe("disposition", () => {
  it("la barre latérale mène aux trois écrans et garde le partage en bas", async () => {
    // Spec D6. Neutralisation : retirer l'entrée « Mon compte » de Disposition.
    render(<App />);
    const navigation = await screen.findByRole("navigation", { name: "Navigation" });
    for (const nom of ["Amis", "Listes", "Mon compte"]) {
      expect(within(navigation).getByRole("button", { name: nom })).toBeInTheDocument();
    }
    expect(within(navigation).getByRole("button", { name: "Partager mon écran" })).toBeInTheDocument();
    await userEvent.click(within(navigation).getByRole("button", { name: "Listes" }));
    expect(screen.getByRole("heading", { name: "Listes" })).toBeInTheDocument();
  });

  it("sans session, seul l'écran de connexion s'affiche", async () => {
    // Neutralisation : afficher la disposition quel que soit `connexion`.
    vi.mocked(pont.etatCourant).mockResolvedValue(instantaneDeTest({ connexion: "deconnecte" }));
    render(<App />);
    expect(await screen.findByRole("button", { name: "Se connecter avec Discord" })).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Navigation" })).toBeNull();
  });

  it("une session expirée mène aussi à l'écran de connexion", async () => {
    // `session_expiree` n'est pas `deconnecte` : sans le `!== "connecte"`, un
    // test sur le seul `deconnecte` laisserait passer la disposition sur une
    // session morte.
    vi.mocked(pont.etatCourant).mockResolvedValue(instantaneDeTest({ connexion: "session_expiree" }));
    render(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Session expirée — reconnecte-toi");
    expect(screen.queryByRole("navigation", { name: "Navigation" })).toBeNull();
  });

  it("le premier instantané du cœur arrive par un appel, pas seulement par un événement", async () => {
    // Un événement `etat` émis avant que l'interface écoute serait perdu :
    // sans `etatCourant`, l'application resterait sur « Chargement… ».
    // Neutralisation : retirer l'appel à `pont.etatCourant` d'`useInstantane`.
    vi.mocked(pont.ecouterEtat).mockImplementation(() => new Promise(() => {}));
    render(<App />);
    expect(await screen.findByRole("navigation", { name: "Navigation" })).toBeInTheDocument();
  });
});
