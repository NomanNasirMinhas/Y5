import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";
import manifest from "./package.json";

export default defineConfig(({ mode }) => ({
  plugins: [react()],
  define: {
    __APP_VERSION__: JSON.stringify(
      loadEnv(mode, ".", "RELEASE_VERSION").RELEASE_VERSION || manifest.version,
    ),
  },
  server: { port: 1420, strictPort: true },
  clearScreen: false,
}));
