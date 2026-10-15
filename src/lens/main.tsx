// Lens entry (DESIGN §3.8 rules 1–3): register every listener on this window, then
// frontend_ready, then render. Registration lives outside React, so it happens exactly once.
import "../shared/theme.css";
import "./lens.css";
import { createRoot } from "react-dom/client";
import { call, on } from "../shared/bridge";
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
    on("og://settings", (settings) => set({ settings })),
    on("og://visibility", (v) => set({ visible: v.on })),
  ]);
  applyInit(await call("frontend_ready"));
  createRoot(document.getElementById("root")!).render(<Lens />);
}

boot().catch((e) => console.error("lens boot failed", e));
