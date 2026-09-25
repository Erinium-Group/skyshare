import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../App";
import { pont } from "../pont";
import { instantaneDeTest } from "../test/fabriques";
import type { Instantane } from "../types";
import { BarrePartage } from "./BarrePartage";
import { PanneauPartage } from "./PanneauPartage";

// Le pont est simulé EN ENTIER, et non réduit à `partager`/`arreter` : le test
// d'arbitrage 3 monte `App`, qui appelle `etatCourant` et `ecouterEtat`. Un
// mock partiel rendrait `undefined` pour ceux-là, et l'échec serait un
// « is not a function » au lieu du fait mesuré.
vi.mock("../pont", () => ({
  pont: { partager: vi.fn(), arreter: vi.fn(), etatCourant: vi.fn(), ecouterEtat: vi.fn() },
}));

let courant: Instantane;

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(pont.partager).mockResolvedValue(undefined);
  vi.mocked(pont.arreter).mockResolvedValue(undefined);
  courant = instantaneDeTest();
  vi.mocked(pont.etatCourant).mockImplementation(() => Promise.resolve(courant));
  vi.mocked(pont.ecouterEtat).mockImplementation((rappel) => {
    rappel(courant);
    return Promise.resolve(() => {});
  });
});

const DEUX_ECRANS = [
  { index: 0, nom: "Écran 1 — 1920×1080", principal: false },
  { index: 1, nom: "Écran 2 — 2560×1440", principal: true },
];

describe("barre de partage", () => {
  it("partage l'écran principal par défaut", async () => {
    // Spec §4. Neutralisation : initialiser le choix à 0.
    render(<BarrePartage instantane={instantaneDeTest({ ecrans: DEUX_ECRANS })} />);
    await userEvent.click(screen.getByRole("button", { name: "Partager mon écran" }));
    expect(pont.partager).toHaveBeenCalledWith(1);
  });

  it("l'écran principal reste le défaut quand les écrans arrivent APRÈS le premier rendu", async () => {
    // Le cœur publie `ecrans: []` à la création (`noyau.rs:253`) et ne les
    // relève qu'au premier passage au premier plan (`rafraichir_ecrans`). Un
    // `useState(principal)` fige donc 0 — le rang de repli — et l'utilisateur
    // partagerait le mauvais écran sans que rien ne le dise, le sélecteur
    // affichant pourtant « Écran 1 » comme s'il l'avait choisi.
    // Neutralisation : remplacer `choix ?? principal` par un
    // `useState(principal)` — ce test rougit avec `partager(0)`, celui du
    // dessus reste vert (il ne monte jamais sans écrans).
    const vue = render(<BarrePartage instantane={instantaneDeTest({ ecrans: [] })} />);
    vue.rerender(<BarrePartage instantane={instantaneDeTest({ ecrans: DEUX_ECRANS })} />);
    await userEvent.click(screen.getByRole("button", { name: "Partager mon écran" }));
    expect(pont.partager).toHaveBeenCalledWith(1);
  });

  it("un choix explicite l'emporte sur l'écran principal", async () => {
    // Contrôle positif du test précédent : sans lui, « toujours renvoyer le
    // principal » le satisferait, et le sélecteur ne servirait plus à rien.
    render(<BarrePartage instantane={instantaneDeTest({ ecrans: DEUX_ECRANS })} />);
    await userEvent.selectOptions(screen.getByLabelText("Écran"), "0");
    await userEvent.click(screen.getByRole("button", { name: "Partager mon écran" }));
    expect(pont.partager).toHaveBeenCalledWith(0);
  });

  it("sans carte NVIDIA, Partager est désactivé et le dit", () => {
    // Pas de repli logiciel x264 (question ouverte du projet) : sans carte
    // NVIDIA, la machine ne peut que recevoir. Un bouton actif enverrait une
    // demande de disponibilité en PRODUCTION pour un partage impossible.
    // Neutralisation : retirer `!instantane.nvenc ||` du `disabled`.
    render(<BarrePartage instantane={instantaneDeTest({ nvenc: false })} />);
    expect(screen.getByRole("button", { name: "Partager mon écran" })).toBeDisabled();
    expect(screen.getByText(/aucune carte NVIDIA/)).toBeInTheDocument();
  });

  it("pendant qu'une action court, Partager est désactivé", async () => {
    // `useActions` est la SEULE borne du projet contre les rafales vers la
    // production (le site a délibérément renoncé à toute limitation de débit).
    // Neutralisation : retirer `|| actions.enVol` du `disabled`.
    vi.mocked(pont.partager).mockReturnValue(new Promise(() => {}));
    render(<BarrePartage instantane={instantaneDeTest()} />);
    const bouton = screen.getByRole("button", { name: "Partager mon écran" });
    await userEvent.click(bouton);
    expect(bouton).toBeDisabled();
  });

  it("en partage, la zone dit En partage et propose Arrêter", async () => {
    // Spec §4 et §8, état « en partage ». Neutralisation : ne pas traiter
    // `disponible` comme un partage — le bouton Partager réapparaît.
    render(
      <BarrePartage
        instantane={instantaneDeTest({
          ecrans: DEUX_ECRANS,
          partage: { etat: "disponible", debutMs: Date.now(), fenetreS: 1800, ecran: 1 },
        })}
      />,
    );
    const zone = within(screen.getByRole("region", { name: "Partage en cours" }));
    expect(zone.getByText("En partage")).toBeInTheDocument();
    expect(zone.getByText("Écran 2 — 2560×1440")).toBeInTheDocument();
    expect(zone.getByText(/Temps restant/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Partager mon écran" })).toBeNull();
    await userEvent.click(zone.getByRole("button", { name: "Arrêter" }));
    expect(pont.arreter).toHaveBeenCalledTimes(1);
  });

  it("pendant qu'on diffuse, la zone reste orange et ne parle plus de temps restant", () => {
    // `diffuse` n'a PAS de `fenetreS` : la fenêtre de disponibilité s'arrête
    // quand quelqu'un regarde. Afficher un temps restant ici obligerait à
    // inventer une valeur. Neutralisation : retirer `|| partage.etat ===
    // "diffuse"` — le bouton Partager réapparaît PENDANT une diffusion.
    render(
      <BarrePartage
        instantane={instantaneDeTest({
          ecrans: DEUX_ECRANS,
          partage: { etat: "diffuse", spectateur: "Bob", depuisMs: Date.now(), debitMbps: 12.4, rttMs: 15, ecran: 1 },
        })}
      />,
    );
    const zone = within(screen.getByRole("region", { name: "Partage en cours" }));
    expect(zone.getByText("En partage")).toBeInTheDocument();
    expect(zone.queryByText(/Temps restant/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Partager mon écran" })).toBeNull();
  });

  it("deux clics sur Arrêter n'envoient qu'un seul arrêt", async () => {
    // Arrêter n'est JAMAIS désactivé — un bouton d'arrêt grisé laisserait un
    // partage en cours sans issue visible. Sa seule borne est donc la garde
    // « une seule action à la fois » d'`useActions`, et ce test la mesure
    // seule : aucun `disabled` ne peut répondre à sa place.
    // Neutralisation : appeler `void pont.arreter()` directement au lieu de
    // passer par `actions.executer` — deux appels au lieu d'un.
    vi.mocked(pont.arreter).mockReturnValue(new Promise(() => {}));
    render(
      <BarrePartage
        instantane={instantaneDeTest({
          partage: { etat: "disponible", debutMs: Date.now(), fenetreS: 1800, ecran: 0 },
        })}
      />,
    );
    const bouton = screen.getByRole("button", { name: "Arrêter" });
    expect(bouton).toBeEnabled();
    await userEvent.click(bouton);
    await userEvent.click(bouton);
    expect(pont.arreter).toHaveBeenCalledTimes(1);
  });

  it("un refus du cœur s'affiche, tel qu'il vient", async () => {
    // ARBITRAGE 4 : l'interface n'invente aucun message. Celui-ci est déjà
    // borné par `detail_borne`. Neutralisation : avaler le rejet (retirer
    // l'affichage d'`actions.message`) — le clic paraîtrait sans effet.
    vi.mocked(pont.partager).mockRejectedValue("aucun écran à partager");
    render(<BarrePartage instantane={instantaneDeTest()} />);
    await userEvent.click(screen.getByRole("button", { name: "Partager mon écran" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("aucun écran à partager");
  });
});

describe("panneau de partage", () => {
  it("le spectateur voit la connexion directe, et que l'image n'est pas encore là", () => {
    // Spec D2 : « Le panneau le dit en toutes lettres ». Sans cette phrase, un
    // écran noir passe pour un bug, et l'utilisateur cherche une panne qui
    // n'existe pas. Neutralisation : retirer le paragraphe.
    render(
      <PanneauPartage
        instantane={instantaneDeTest({
          partage: {
            etat: "regarde",
            ami: "Bob",
            connecteEnS: 0.6,
            debitMbps: 12.4,
            imagesParS: 107,
            gigueMs: 5,
            depuisMs: Date.now(),
          },
        })}
      />,
    );
    expect(screen.getByText("Connecté en 0,6 s · connexion directe, sans relais")).toBeInTheDocument();
    expect(screen.getByText(/image n'est pas encore affichée/)).toBeInTheDocument();
    expect(screen.getByText("107 images/s")).toBeInTheDocument();
  });

  it("les mesures du spectateur sont écrites à la française", () => {
    // RONDE DE CORRECTION 1, M1 : virgule décimale, pas point.
    // Neutralisation : retirer le `.replace(".", ",")` de `decimale` — les
    // trois assertions rougissent ici, plus celles de `messages.test.ts`.
    render(
      <PanneauPartage
        instantane={instantaneDeTest({
          partage: {
            etat: "regarde",
            ami: "Bob",
            connecteEnS: 0.6,
            debitMbps: 12.4,
            imagesParS: 107,
            gigueMs: 5,
            depuisMs: Date.now(),
          },
        })}
      />,
    );
    expect(screen.getByText("12,4 Mbps")).toBeInTheDocument();
    expect(screen.getByText("5,0 ms")).toBeInTheDocument();
    // La cadence est un entier côté cœur : aucune décimale à séparer.
    expect(screen.getByText("107 images/s")).toBeInTheDocument();
  });

  it("un refus d'`arreter` s'affiche DANS LE PANNEAU, en état regarde", async () => {
    // RONDE DE CORRECTION 1, I1. Dans `demande` et `regarde`, ce panneau est le
    // SEUL porteur d'un bouton Arrêter : la barre latérale y est sur une autre
    // branche. Sans cet affichage, un refus du cœur laissait le clic sans le
    // moindre effet visible — « la serrure posée mais jamais branchée », version
    // interface. Neutralisation : retirer le bloc `{actions.message && …}` de
    // `arreter` dans `PanneauPartage` — ce test rougit, et LUI SEUL : celui de
    // la barre monte `BarrePartage`, qui a son propre affichage.
    vi.mocked(pont.arreter).mockRejectedValue("le coeur a refusé");
    render(
      <PanneauPartage
        instantane={instantaneDeTest({
          partage: {
            etat: "regarde",
            ami: "Bob",
            connecteEnS: 0.6,
            debitMbps: 12.4,
            imagesParS: 107,
            gigueMs: 5,
            depuisMs: Date.now(),
          },
        })}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Arrêter" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("le coeur a refusé");
  });

  it("un refus d'`arreter` s'affiche aussi en état demande", async () => {
    // Le même bouton, l'autre état. Sans ce second test, déplacer l'affichage
    // dans la seule branche `regarde` resterait vert.
    vi.mocked(pont.arreter).mockRejectedValue("le coeur a refusé");
    render(
      <PanneauPartage
        instantane={instantaneDeTest({ partage: { etat: "demande", ami: "Bob", debutMs: Date.now() } })}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Arrêter" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("le coeur a refusé");
  });

  it("le panneau dit aussi que rien n'est écrit sur le disque", () => {
    // Spec D2 : la vidéo reçue est mesurée puis JETÉE. C'est une promesse de
    // vie privée faite au spectateur ET à l'hôte, au même titre que « aucune
    // adresse journalisée ». Neutralisation : retirer la fin de la phrase.
    render(
      <PanneauPartage
        instantane={instantaneDeTest({
          partage: {
            etat: "regarde",
            ami: "Bob",
            connecteEnS: 0.6,
            debitMbps: 12.4,
            imagesParS: 107,
            gigueMs: 5,
            depuisMs: Date.now(),
          },
        })}
      />,
    );
    expect(screen.getByText(/rien n'est écrit sur le disque/)).toBeInTheDocument();
  });

  it("l'attente du spectateur annonce ses 60 secondes", () => {
    // `ATTENTE_SPECTATEUR` = 60 s (spec D3). Sans le dire, une attente d'une
    // minute est indiscernable d'un gel.
    render(
      <PanneauPartage
        instantane={instantaneDeTest({ partage: { etat: "demande", ami: "Bob", debutMs: Date.now() } })}
      />,
    );
    expect(screen.getByRole("heading", { name: "Demande envoyée à Bob" })).toBeInTheDocument();
    expect(screen.getByText(/60 s au plus/)).toBeInTheDocument();
  });

  it("côté hôte, le panneau nomme qui regarde et montre débit et aller-retour", () => {
    render(
      <PanneauPartage
        instantane={instantaneDeTest({
          partage: { etat: "diffuse", spectateur: "Bob", depuisMs: Date.now(), debitMbps: 12.4, rttMs: 115.4, ecran: 0 },
        })}
      />,
    );
    expect(screen.getByRole("heading", { name: "Bob regarde ton écran" })).toBeInTheDocument();
    expect(screen.getByText("12,4 Mbps")).toBeInTheDocument();
    // L'aller-retour est ARRONDI à l'entier : une milliseconde décimale n'ajoute
    // rien à un chiffre qui varie de dizaines.
    expect(screen.getByText("115 ms")).toBeInTheDocument();
  });

  it("un spectateur sans nom ne fait pas afficher « null regarde ton écran »", () => {
    // `spectateur: string | null` dans `types.ts` : le cœur ne connaît pas
    // toujours le nom. Neutralisation : retirer le `?? "Un ami"`.
    render(
      <PanneauPartage
        instantane={instantaneDeTest({
          partage: { etat: "diffuse", spectateur: null, depuisMs: Date.now(), debitMbps: 1, rttMs: 20, ecran: 0 },
        })}
      />,
    );
    expect(screen.getByRole("heading", { name: "Un ami regarde ton écran" })).toBeInTheDocument();
  });

  it("une fin affiche sa cause en clair", () => {
    render(
      <PanneauPartage
        instantane={instantaneDeTest({ partage: { etat: "termine", fin: { cause: "pas_en_partage", ami: "Bob" } } })}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Bob n'est pas en partage");
  });

  it("une AUTRE fin affiche SA cause, pas celle d'à côté", () => {
    // Le test précédent resterait vert si le panneau affichait toujours le
    // même message. Neutralisation : rendre `messageDeFin({ cause: "arrete" })`
    // en dur — ce test rougit, le précédent aussi : c'est une conséquence
    // réelle (les deux lisent la même ligne), et c'est pourquoi ils ne sont pas
    // interchangeables — seul celui-ci distingue « la cause est lue » de « un
    // message est affiché ».
    render(
      <PanneauPartage
        instantane={instantaneDeTest({ partage: { etat: "termine", fin: { cause: "reseau_bloque" } } })}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "Aucune connexion directe n'a pu s'établir entre vos deux réseaux",
    );
  });

  it("hors partage, le panneau ne prend aucune place", () => {
    const { container } = render(<PanneauPartage instantane={instantaneDeTest()} />);
    expect(container).toBeEmptyDOMElement();
  });
});

describe("on ne partage jamais sans le savoir", () => {
  it("l'état du partage reste visible depuis N'IMPORTE quel écran", async () => {
    // ARBITRAGE 3 DU CONTRÔLEUR, spec §4 : « on ne partage jamais sans le
    // savoir ». Le panneau central disparaît dès qu'on change d'écran ; seule
    // la barre latérale est hors des écrans. Neutralisation : rendre la
    // `BarrePartage` dans les enfants de `Disposition` au lieu de sa prop
    // `bas` — la zone orange disparaît au passage sur Listes.
    courant = instantaneDeTest({
      partage: { etat: "disponible", debutMs: Date.now(), fenetreS: 1800, ecran: 0 },
    });
    render(<App />);
    const navigation = await screen.findByRole("navigation", { name: "Navigation" });
    await userEvent.click(within(navigation).getByRole("button", { name: "Listes" }));
    expect(screen.getByRole("heading", { name: "Listes" })).toBeInTheDocument();
    expect(within(navigation).getByRole("region", { name: "Partage en cours" })).toBeInTheDocument();
  });
});

describe("on ne reçoit pas non plus sans le savoir", () => {
  it("la barre latérale dit qu'une réception est en cours", () => {
    // RONDE DE CORRECTION 1, M4. Une réception consomme du débit et du quota
    // comme une émission. Le panneau la montre déjà sur tous les écrans, mais
    // il vit dans un `<main>` en `overflow-y-auto` : sur une longue liste
    // d'amis, il défile hors de vue. La barre, elle, ne défile pas.
    // Neutralisation : retirer la branche `demande | regarde` de
    // `BarrePartage` — la région disparaît et « Partager mon écran » reprend sa
    // place.
    render(
      <BarrePartage
        instantane={instantaneDeTest({
          partage: {
            etat: "regarde",
            ami: "Bob",
            connecteEnS: 0.6,
            debitMbps: 12.4,
            imagesParS: 107,
            gigueMs: 5,
            depuisMs: Date.now(),
          },
        })}
      />,
    );
    const zone = within(screen.getByRole("region", { name: "Réception en cours" }));
    expect(zone.getByText("Tu regardes")).toBeInTheDocument();
    expect(zone.getByText("Bob")).toBeInTheDocument();
    // Partager reste hors d'atteinte pendant une réception : la machine est
    // occupée. Ce n'est plus un `disabled` mais une branche, donc le bouton
    // n'existe pas du tout.
    expect(screen.queryByRole("button", { name: "Partager mon écran" })).toBeNull();
  });

  it("une demande en attente se voit aussi dans la barre", () => {
    render(
      <BarrePartage
        instantane={instantaneDeTest({ partage: { etat: "demande", ami: "Bob", debutMs: Date.now() } })}
      />,
    );
    const zone = within(screen.getByRole("region", { name: "Réception en cours" }));
    expect(zone.getByText("Demande envoyée")).toBeInTheDocument();
    expect(zone.getByText("Bob")).toBeInTheDocument();
  });

  it("la barre ne propose PAS un second Arrêter pendant une réception", () => {
    // INVARIANT DE M3 : `useActions` tient son `enVolRef` par composant. Deux
    // boutons Arrêter simultanés, dans deux composants, auraient deux gardes
    // indépendantes — et deux `arreter` partiraient. Ce test fige la seule
    // chose qui garantit aujourd'hui qu'ils ne coexistent pas.
    // Neutralisation : ajouter un bouton Arrêter à la branche « Réception en
    // cours » de `BarrePartage`.
    render(
      <BarrePartage
        instantane={instantaneDeTest({
          partage: {
            etat: "regarde",
            ami: "Bob",
            connecteEnS: 0.6,
            debitMbps: 12.4,
            imagesParS: 107,
            gigueMs: 5,
            depuisMs: Date.now(),
          },
        })}
      />,
    );
    expect(screen.queryByRole("button", { name: "Arrêter" })).toBeNull();
  });

  it("l'état « je regarde » survit au changement d'écran", async () => {
    courant = instantaneDeTest({
      partage: {
        etat: "regarde",
        ami: "Bob",
        connecteEnS: 0.6,
        debitMbps: 12.4,
        imagesParS: 107,
        gigueMs: 5,
        depuisMs: Date.now(),
      },
    });
    render(<App />);
    const navigation = await screen.findByRole("navigation", { name: "Navigation" });
    await userEvent.click(within(navigation).getByRole("button", { name: "Listes" }));
    expect(within(navigation).getByRole("region", { name: "Réception en cours" })).toBeInTheDocument();
    // Le panneau est rendu HORS des branches `ecran === …` d'`App.tsx` : il
    // survit lui aussi au changement d'écran. Mesuré, pas supposé — et figé ici
    // pour que personne ne le glisse dans une branche d'écran.
    expect(screen.getByRole("button", { name: "Arrêter" })).toBeInTheDocument();
  });
});

describe("le temps restant s'écoule", () => {
  afterEach(() => vi.useRealTimers());

  it("hors partage, AUCUNE minuterie ne tourne", () => {
    // RONDE DE CORRECTION 1, M2. L'application « vit comme Discord » (spec D4) :
    // elle reste ouverte des journées entières, l'écrasante majorité du temps
    // hors de tout partage. Deux minuteries à 1 Hz y battaient en permanence,
    // dont une dans un composant qui rend `null`.
    // Neutralisation : rendre `useMaintenant` inconditionnel (retirer le
    // `if (!actif) return`) — ce test rougit avec 2 au lieu de 0.
    vi.useFakeTimers();
    const instantane = instantaneDeTest();
    render(
      <>
        <BarrePartage instantane={instantane} />
        <PanneauPartage instantane={instantane} />
      </>,
    );
    expect(vi.getTimerCount()).toBe(0);
  });

  it("côté spectateur, l'horloge bat — mais dans le panneau seulement", () => {
    // Contrôle positif du test ci-dessus : sans lui, « ne jamais démarrer de
    // minuterie » le satisferait, et la durée de réception resterait figée.
    vi.useFakeTimers();
    const instantane = instantaneDeTest({
      partage: {
        etat: "regarde",
        ami: "Bob",
        connecteEnS: 0.6,
        debitMbps: 12.4,
        imagesParS: 107,
        gigueMs: 5,
        depuisMs: Date.now(),
      },
    });
    render(
      <>
        <BarrePartage instantane={instantane} />
        <PanneauPartage instantane={instantane} />
      </>,
    );
    // UNE seule : la barre n'affiche aucune durée dans cet état.
    expect(vi.getTimerCount()).toBe(1);
  });

  it("le compte à rebours descend sans nouvel instantané du cœur", () => {
    // Le cœur ne publie un instantané que sur CHANGEMENT : entre deux
    // synchronisations, personne ne repousse le rendu. Sans minuterie propre,
    // « Temps restant » resterait figé sur la même valeur pendant 30 minutes.
    // Neutralisation : rendre `Date.now()` sans `setInterval` dans
    // `useMaintenant`.
    // Aucune attente réelle : les minuteries sont simulées.
    vi.useFakeTimers();
    const debutMs = Date.now();
    render(
      <BarrePartage
        instantane={instantaneDeTest({ partage: { etat: "disponible", debutMs, fenetreS: 1800, ecran: 0 } })}
      />,
    );
    const zone = within(screen.getByRole("region", { name: "Partage en cours" }));
    expect(zone.getByText("Temps restant : 30 min 00 s")).toBeInTheDocument();
    act(() => void vi.advanceTimersByTime(65_000));
    expect(zone.getByText("Temps restant : 28 min 55 s")).toBeInTheDocument();
  });

  it("la minuterie s'arrête au démontage", () => {
    // Sans `clearInterval`, chaque partage regardé laisse derrière lui une
    // minuterie qui appelle `setMaintenant` sur un composant démonté — une
    // fuite qui grossit à chaque aller-retour entre les écrans.
    // Neutralisation : retirer le `return () => clearInterval(minuterie)`.
    vi.useFakeTimers();
    const vue = render(
      <BarrePartage
        instantane={instantaneDeTest({ partage: { etat: "disponible", debutMs: Date.now(), fenetreS: 1800, ecran: 0 } })}
      />,
    );
    expect(vi.getTimerCount()).toBe(1);
    vue.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});
