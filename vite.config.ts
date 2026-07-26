import { defineConfig } from "vite";

// Tauri serves the frontend from this dev server in `tauri dev`, and from
// dist/ in a release build. Fixed port + strictPort so tauri.conf.json can
// point at it deterministically.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 5183,
    strictPort: true,
  },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
  },
});
