import { act, render, screen, within } from "@testing-library/react";
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

  it("une action en vol désactive Ajouter, qui redevient actif ensuite", async () => {
    // I2 : sans borne côté interface, un clic répété part en rafale vers la
    // production — le projet a renoncé à toute limitation de débit côté site.
    // Neutralisation : retirer `disabled={enVol}` du bouton Ajouter.
    let terminer: (m: string) => void = () => {};
    vi.mocked(pont.ajouterAmi).mockReturnValue(new Promise((r) => (terminer = r)));
    render(<Amis instantane={instantaneDeTest()} />);
    await userEvent.type(screen.getByLabelText("Code ami"), "SKY-ABCD-EFGH");
    await userEvent.click(screen.getByRole("button", { name: "Ajouter" }));
    expect(screen.getByRole("button", { name: "Ajouter" })).toBeDisabled();
    terminer("Demande envoyée.");
    expect(await screen.findByRole("status")).toHaveTextContent("Demande envoyée.");
    expect(screen.getByRole("button", { name: "Ajouter" })).toBeEnabled();
  });

  it("un refus rend la main : le bouton ne reste pas figé", async () => {
    // Contrôle positif du test précédent : sans le `finally`, un seul refus
    // condamnerait l'écran. Neutralisation : sortir `setEnVol(false)` du
    // `finally` pour ne le faire que sur succès.
    vi.mocked(pont.ajouterAmi).mockRejectedValue("Code ami introuvable.");
    render(<Amis instantane={instantaneDeTest()} />);
    await userEvent.click(screen.getByRole("button", { name: "Ajouter" }));
    expect(await screen.findByRole("status")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ajouter" })).toBeEnabled();
  });

  it("deux soumissions dans le même tour n'envoient qu'une requête", async () => {
    // I2, le cas réel : une touche Entrée MAINTENUE dans le champ code ami.
    // Le `disabled` du bouton ne protège pas ce chemin — le formulaire se
    // soumet au clavier — et l'état `enVol` garde sa valeur pour tout le rendu
    // en cours. Neutralisation : remplacer `enVolRef.current` par `enVol` dans
    // la garde d'`executer` — `ajouterAmi` est appelé deux fois.
    vi.mocked(pont.ajouterAmi).mockReturnValue(new Promise(() => {}));
    render(<Amis instantane={instantaneDeTest()} />);
    const formulaire = screen.getByLabelText("Code ami").closest("form")!;
    // Les DEUX événements dans le MÊME `act` : React groupe alors la mise à
    // jour d'état, et le second gestionnaire s'exécute avec le rendu — donc le
    // `enVol` — du premier. C'est ce que fait une touche maintenue, et ce que
    // `fireEvent` appelé deux fois ne reproduit PAS : il vide la file entre les
    // deux, ce qui laisse l'état rattraper et masque le défaut.
    const soumettre = () =>
      formulaire.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    act(() => {
      soumettre();
      soumettre();
    });
    expect(pont.ajouterAmi).toHaveBeenCalledTimes(1);
  });

  it("le champ est vidé après un succès, gardé après un refus", async () => {
    // I2 : un champ non vidé invite au second clic. Mais le vider sur un refus
    // obligerait à ressaisir un code correct rejeté pour une autre raison.
    // Neutralisation : vider le champ sans regarder l'issue — le second cas
    // rougit.
    vi.mocked(pont.ajouterAmi).mockResolvedValue("Demande envoyée.");
    render(<Amis instantane={instantaneDeTest()} />);
    const champ = screen.getByLabelText("Code ami");
    await userEvent.type(champ, "SKY-ABCD-EFGH");
    await userEvent.click(screen.getByRole("button", { name: "Ajouter" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Demande envoyée.");
    expect(champ).toHaveValue("");

    vi.mocked(pont.ajouterAmi).mockRejectedValue("Une demande existe déjà avec ce compte.");
    await userEvent.type(champ, "SKY-WXYZ-2345");
    await userEvent.click(screen.getByRole("button", { name: "Ajouter" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Une demande existe déjà");
    expect(champ).toHaveValue("SKY-WXYZ-2345");
  });

  it("le menu « … » se referme après une action", async () => {
    // M3. Neutralisation : retirer le `setMenu(false)` de `depuisLeMenu`.
    vi.mocked(pont.bloquerAmi).mockResolvedValue("Ami bloqué.");
    render(<Amis instantane={instantaneDeTest({ amis: [BOB] })} />);
    await userEvent.click(screen.getByRole("button", { name: "Plus d'actions pour Bob" }));
    await userEvent.click(screen.getByRole("button", { name: "Bloquer" }));
    expect(screen.queryByRole("button", { name: "Bloquer" })).toBeNull();
    expect(screen.getByRole("button", { name: "Plus d'actions pour Bob" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("le menu « … » se referme à l'Échap", async () => {
    // M3. Neutralisation : retirer le `onKeyDown` du groupe.
    render(<Amis instantane={instantaneDeTest({ amis: [BOB] })} />);
    await userEvent.click(screen.getByRole("button", { name: "Plus d'actions pour Bob" }));
    screen.getByRole("button", { name: "Retirer" }).focus();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("button", { name: "Retirer" })).toBeNull();
  });

  it("l'ami sans appareil est expliqué en toutes lettres, pas dans un title", () => {
    // M3 : un `title` sur un bouton désactivé n'est ni focusable ni
    // survolable — l'explication serait invisible sans souris. Neutralisation :
    // remettre l'explication dans un `title` — le texte n'est plus trouvé.
    render(<Amis instantane={instantaneDeTest({ amis: [ALICE] })} />);
    const bouton = ligne("Alice").getByRole("button", { name: "Regarder" });
    const explication = screen.getByText("Alice n'a aucun appareil enregistré");
    expect(explication).toBeInTheDocument();
    expect(bouton).toHaveAttribute("aria-describedby", explication.id);
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
