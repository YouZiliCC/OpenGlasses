// Panel entry: register listeners, then frontend_ready, then the theme, then render
// (DESIGN §3.8 rules 2–3, §10.4).
import "../shared/base.css";
import "./panel.css";
import { createRoot } from "react-dom/client";
import { call, on } from "../shared/bridge";
import type { Settings } from "../shared/ipc";
import { initTheme, onThemes, setTheme } from "../shared/themeLoader";
import { Panel } from "./Panel";
import { applyInit, onPanel, onSession, onSettings, usePanel } from "./store";

// Events that land before the InitState snapshot is applied are replayed after it.
let pending: (() => void)[] | null = [];
const gate =
  <T,>(fn: (p: T) => void) =>
  (p: T) => {
    if (pending) pending.push(() => fn(p));
    else fn(p);
  };

async function main() {
  await Promise.all([
    on("og://session", gate(onSession)),
    on("og://panel", gate(onPanel)),
    on(
      "og://settings",
      gate((s: Settings) => {
        onSettings(s);
        setTheme(s);
      }),
    ),
    on("og://themes", onThemes),
  ]);
  try {
    applyInit(await call("frontend_ready"));
  } catch (e) {
    console.error("frontend_ready", e);
  }
  await initTheme(usePanel.getState().settings);
  const queued = pending ?? [];
  pending = null;
  queued.forEach((f) => f());
  createRoot(document.getElementById("root")!).render(<Panel />);
}

// No page reload in the panel (a reload replays queued deltas onto the restored snapshot):
// keep right-click Copy/Paste on text, drop the page menu and F5 / Ctrl+R.
window.addEventListener("contextmenu", (e) => {
  if (!(e.target instanceof HTMLTextAreaElement) && !getSelection()?.toString()) e.preventDefault();
});
window.addEventListener("keydown", (e) => {
  if (e.key === "F5" || (e.ctrlKey && e.key.toLowerCase() === "r")) e.preventDefault();
});

void main();
