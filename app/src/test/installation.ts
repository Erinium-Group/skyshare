import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// Sans globales Vitest, le nettoyage automatique de Testing Library ne
// s'installe pas : chaque test laisserait son rendu au suivant.
afterEach(() => cleanup());
