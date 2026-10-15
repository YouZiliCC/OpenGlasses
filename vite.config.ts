import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and no screen clearing so Rust errors stay visible.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
    // `tauri dev` loads this server directly, so tauri.conf.json's CSP never applies here.
    // Mirror its resource limits so an untrusted theme pack can't reach the network in dev
    // either (no script-src/connect-src: Vite HMR and React refresh need them).
    headers: {
      "Content-Security-Policy":
        "img-src 'self' data: blob: http://ogframe.localhost http://ogtheme.localhost; " +
        "font-src 'self' data: http://ogtheme.localhost; " +
        "style-src 'self' 'unsafe-inline' http://ogtheme.localhost",
    },
  },
  build: {
    target: "es2022",
    rollupOptions: {
      // Three windows, three entries (DESIGN §4).
      input: { lens: "lens.html", panel: "panel.html", settings: "settings.html" },
    },
  },
});
