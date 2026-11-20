// SPDX-License-Identifier: AGPL-3.0-only
import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const root = import.meta.dirname;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/.smoke/**"] },
  },
  build: {
    target: "chrome105",
    rollupOptions: {
      input: {
        manager: resolve(root, "manager.html"),
        popup: resolve(root, "popup.html"),
        settings: resolve(root, "settings.html"),
      },
    },
  },
});
