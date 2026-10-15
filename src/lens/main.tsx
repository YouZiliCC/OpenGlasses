// Lens entry (DESIGN §3.8 rules 1–3): register every listener on this window, then
// frontend_ready, then the theme, then render. Registration lives outside React, so it happens exactly once.
import "../shared/base.css";
import "./lens.css";
import { createRoot } from "react-dom/client";
import { call, on } from "../shared/bridge";
import { initTheme, onThemes, setTheme } from "../shared/themeLoader";
import { trackDwell } from "./hover";
import { Lens } from "./Lens";
import { applyInit, onLensOn, onSession, onToast, useLens } from "./store";

async function boot() {
  const set = useLens.setState;
  await Promise.all([
    on("og://cursor", (cursor) => {
      set({ cursor });
      trackDwell(cursor);
    }),
    on("og://hud", (hud) => set({ hud })),
    on("og://toast", onToast),
    on("og://lens-on", onLensOn),
    on("og://freeze", (freeze) => set({ freeze })),
    on("og://session", onSession),
    on("og://settings", (settings) => {
      set({ settings });
      setTheme(settings);
    }),
    // Hidden mid-boot: the stage unmounts before its timer ends it, so drop it here.
    on("og://visibility", (v) => set(v.on ? { visible: true } : { visible: false, lensOn: null })),
    on("og://themes", onThemes),
  ]);
  applyInit(await call("frontend_ready"));
  await initTheme(useLens.getState().settings);
  createRoot(document.getElementById("root")!).render(<Lens />);
}

// The WebView2 context menu (Reload, Save as…) must never appear on the overlay; right-click
// during SELECT is a cancel (FreezeSelect).
window.addEventListener("contextmenu", (e) => e.preventDefault());

boot().catch((e) => console.error("lens boot failed", e));
