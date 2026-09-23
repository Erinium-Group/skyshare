import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { pont } from "../pont";
import { instantaneDeTest } from "../test/fabriques";
import { codeAffiche, MonCompte } from "./MonCompte";

vi.mock("../pont", () => ({
  pont: {
    regenererCode: vi.fn(),
    revoquerAppareil: vi.fn(),
    demarrageAutomatique: vi.fn(),
    deconnexion: vi.fn(),
  },
}));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(pont.regenererCode).mockResolvedValue("Nouveau code : SKY-REGE-NAA2. L'ancien ne fonctionne plus.");
  vi.mocked(pont.revoquerAppareil).mockResolvedValue("Appareil révoqué.");
  vi.mocked(pont.demarrageAutomatique).mockResolvedValue(undefined);
  vi.mocked(pont.deconnexion).mockResolvedValue(undefined);
});

const APPAREILS = [
  { id: 4, nom: "BUREAU", courant: true, revoque: false },
  { id: 5, nom: "PORTABLE", courant: false, revoque: false },
  { id: 6, nom: "VIEUX", courant: false, revoque: true },
];

function ligne(nom: string) {
  return within(screen.getByText(nom).closest("li")!);
}

describe("Mon compte", () => {
  it("l'appareil courant n'est pas révocable, les autres le sont", async () => {
    // Révoquer l'appareil de cette machine révoquerait la session qu'il porte :
    // l'utilisateur serait déconnecté sans l'avoir demandé. Le cœur le refuse
    // aussi (`MESSAGE_APPAREIL_COURANT`) — les deux, comme ailleurs : un bouton
    // absent ne protège rien si la commande reste appelable.
    // Neutralisation : afficher `BoutonDestructeur` aussi pour `courant` — la
    // première assertion rougit.
    render(<MonCompte instantane={instantaneDeTest({ appareils: APPAREILS })} />);
    expect(ligne("BUREAU").queryByRole("button", { name: "Révoquer" })).toBeNull();
    expect(ligne("BUREAU").getByText("cet appareil")).toBeInTheDocument();

    await userEvent.click(ligne("PORTABLE").getByRole("button", { name: "Révoquer" }));
    expect(pont.revoquerAppareil).not.toHaveBeenCalled();
    await userEvent.click(
      ligne("PORTABLE").getByRole("button", { name: "Révoquer définitivement PORTABLE" }),
    );
    expect(pont.revoquerAppareil).toHaveBeenCalledWith(5);
  });

  it("un appareil déjà révoqué le dit et n'offre plus rien", () => {
    // Le site répond 204 à la révocation d'un appareil déjà révoqué : le geste
    // serait sans effet et sans message utile. Neutralisation : retirer la
    // branche `revoque` — le bouton réapparaît.
    render(<MonCompte instantane={instantaneDeTest({ appareils: APPAREILS })} />);
    expect(ligne("VIEUX").getByText("révoqué")).toBeInTheDocument();
    expect(ligne("VIEUX").queryByRole("button", { name: "Révoquer" })).toBeNull();
  });

  it("la case de démarrage envoie l'inverse de l'état affiché", async () => {
    // La case suit l'instantané, jamais un état local : elle ne bouge qu'une
    // fois que la coquille a écrit dans le registre de Windows. Neutralisation :
    // envoyer `instantane.demarrageAutomatique` sans le nier — l'appel devient
    // `true`.
    render(<MonCompte instantane={instantaneDeTest({ demarrageAutomatique: true })} />);
    const caseDemarrage = screen.getByRole("checkbox", {
      name: "Lancer SkyShare au démarrage de Windows",
    });
    expect(caseDemarrage).toBeChecked();
    await userEvent.click(caseDemarrage);
    expect(pont.demarrageAutomatique).toHaveBeenCalledWith(false);
  });

  it("la case reflète un démarrage automatique désactivé", () => {
    // Contrôle positif du test précédent : sans lui, une case cochée en dur le
    // satisferait.
    render(<MonCompte instantane={instantaneDeTest({ demarrageAutomatique: false })} />);
    expect(
      screen.getByRole("checkbox", { name: "Lancer SkyShare au démarrage de Windows" }),
    ).not.toBeChecked();
  });

  it("prévient que sky-probe vole les demandes pendant que l'application tourne", () => {
    // Spec §6 : « L'application le dit dans Mon compte ». Neutralisation :
    // retirer le paragraphe — ce test rougit.
    render(<MonCompte instantane={instantaneDeTest()} />);
    expect(screen.getByText(/sky-probe/)).toBeInTheDocument();
  });

  it("le code s'affiche sous la forme que le site accepte", () => {
    // `normaliserCode` du site retire les tirets et met en majuscules : la forme
    // affichée est celle qu'on peut recopier telle quelle dans « Ajouter un
    // ami ». Neutralisation : rendre `code` tel quel — « ABCD2345 ».
    expect(codeAffiche("ABCD2345")).toBe("SKY-ABCD-2345");
    expect(codeAffiche(null)).toBe("—");
  });

  it("régénérer affiche le message du cœur, et rien d'autre", async () => {
    // L'interface n'invente aucun message : celui-ci vient du cœur, déjà borné.
    render(<MonCompte instantane={instantaneDeTest()} />);
    await userEvent.click(screen.getByRole("button", { name: "Régénérer" }));
    expect(pont.regenererCode).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("status")).toHaveTextContent("L'ancien ne fonctionne plus.");
  });

  it("une action en vol désactive Régénérer, qui redevient actif ensuite", async () => {
    // Même borne que dans Amis : le projet a renoncé à toute limitation de débit
    // côté site, le rempart est ici. Neutralisation : retirer `disabled={enVol}`
    // du bouton Régénérer.
    let terminer: (m: string) => void = () => {};
    vi.mocked(pont.regenererCode).mockReturnValue(new Promise((r) => (terminer = r)));
    render(<MonCompte instantane={instantaneDeTest()} />);
    await userEvent.click(screen.getByRole("button", { name: "Régénérer" }));
    expect(screen.getByRole("button", { name: "Régénérer" })).toBeDisabled();
    terminer("Nouveau code : SKY-REGE-NAA2. L'ancien ne fonctionne plus.");
    expect(await screen.findByRole("status")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Régénérer" })).toBeEnabled();
  });

  it("un refus du cœur s'affiche tel quel", async () => {
    // Contrôle positif du `finally` d'`useActions` : un refus rend la main.
    vi.mocked(pont.revoquerAppareil).mockRejectedValue(
      "C'est l'appareil que tu utilises : il ne peut pas se révoquer lui-même.",
    );
    render(<MonCompte instantane={instantaneDeTest({ appareils: APPAREILS })} />);
    await userEvent.click(ligne("PORTABLE").getByRole("button", { name: "Révoquer" }));
    await userEvent.click(
      ligne("PORTABLE").getByRole("button", { name: "Révoquer définitivement PORTABLE" }),
    );
    expect(await screen.findByRole("status")).toHaveTextContent("il ne peut pas se révoquer");
    expect(screen.getByRole("button", { name: "Régénérer" })).toBeEnabled();
  });

  it("Se déconnecter passe par le cœur", async () => {
    render(<MonCompte instantane={instantaneDeTest()} />);
    await userEvent.click(screen.getByRole("button", { name: "Se déconnecter" }));
    expect(pont.deconnexion).toHaveBeenCalledTimes(1);
  });
});
