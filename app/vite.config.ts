import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Port fixe : `devUrl` de spike/crates/sky-app/tauri.conf.json le vise.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { outDir: "dist" },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/installation.ts"],
    css: false,
  },
});
