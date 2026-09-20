import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { pont } from "./pont";
import { instantaneDeTest } from "./test/fabriques";
import type { Instantane } from "./types";

vi.mock("./pont", () => ({ pont: { etatCourant: vi.fn(), ecouterEtat: vi.fn(), connexion: vi.fn() } }));

/** L'état que le cœur simulé rend ET émet ; un test le remplace avant `render`. */
let courant: Instantane;

beforeEach(() => {
  vi.clearAllMocks();
  courant = instantaneDeTest();
  vi.mocked(pont.etatCourant).mockImplementation(() => Promise.resolve(courant));
  // RONDE DE CORRECTION 1, M2 : le faux écouteur ÉMET, comme le vrai le fait
  // dès qu'une synchronisation publie. Quand il se contentait de résoudre sa
  // promesse sans jamais appeler son rappel, aucun instantané n'arrivait par
  // aucun chemin : neutraliser `etatCourant` faisait alors rougir les QUATRE
  // tests de ce fichier, et aucun ne prouvait plus rien en propre.
  vi.mocked(pont.ecouterEtat).mockImplementation((rappel) => {
    rappel(courant);
    return Promise.resolve(() => {});
  });
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
    courant = instantaneDeTest({ connexion: "deconnecte" });
    render(<App />);
    expect(await screen.findByRole("button", { name: "Se connecter avec Discord" })).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Navigation" })).toBeNull();
  });

  it("une session expirée mène aussi à l'écran de connexion", async () => {
    // `session_expiree` n'est pas `deconnecte` : sans le `!== "connecte"`, un
    // test sur le seul `deconnecte` laisserait passer la disposition sur une
    // session morte.
    courant = instantaneDeTest({ connexion: "session_expiree" });
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

describe("échec de lecture de l'état", () => {
  it("un refus d'`etat_courant` s'affiche au lieu de « Chargement… » éternel", async () => {
    // I1 : un rejet avalé laisse l'application sur « Chargement… » pour
    // toujours, sans alerte ni issue — échec silencieux. Neutralisation :
    // retirer le gestionnaire de rejet du `.then(recevoir, echouer)` de
    // `pont.etatCourant` dans `useInstantane`.
    vi.mocked(pont.etatCourant).mockRejectedValue("le cœur ne répond pas");
    vi.mocked(pont.ecouterEtat).mockImplementation(() => new Promise(() => {}));
    render(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent("le cœur ne répond pas");
    expect(screen.queryByText("Chargement…")).toBeNull();
  });

  it("un refus de l'abonnement s'affiche aussi", async () => {
    // `listen` peut échouer de son côté ; sans reprise, même écran figé.
    // Neutralisation : retirer le gestionnaire de rejet du `.then(..., echouer)`
    // de `pont.ecouterEtat`.
    vi.mocked(pont.etatCourant).mockImplementation(() => new Promise(() => {}));
    vi.mocked(pont.ecouterEtat).mockRejectedValue("abonnement refusé");
    render(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent("abonnement refusé");
  });

  it("un échec tardif ne jette pas l'instantané déjà affiché", async () => {
    // Contrôle positif des deux tests ci-dessus : sans lui, un écran d'erreur
    // permanent les satisferait. Le premier état reçu doit tenir.
    vi.mocked(pont.ecouterEtat).mockImplementation((rappel) => {
      rappel(courant);
      return Promise.reject("abonnement refusé après coup");
    });
    render(<App />);
    expect(await screen.findByRole("navigation", { name: "Navigation" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
