import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  build: {
    rollupOptions: {
      input: {
        main: "./index.html",
        overlay: "./overlay.html",
      },
      output: {
        manualChunks(id: string) {
          if (id.includes("node_modules/react")) return "react";
          if (id.includes("node_modules/@tauri-apps")) return "tauri";
        },
      },
    },
  },
});
