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
    rollupOptions: {
      // Two entry points: the borderless game overlay, and the ordinary
      // window that holds setup, status and options. Paths are relative to the
      // project root, which avoids needing @types/node here just for __dirname.
      input: {
        main: "index.html",
        settings: "settings.html",
      },
    },
  },
});
