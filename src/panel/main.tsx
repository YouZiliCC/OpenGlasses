// Panel entry: register listeners, then frontend_ready, then the theme, then render
// (DESIGN §3.8 rules 2–3, §10.4).
import "../shared/base.css";
import "./panel.css";
import { createRoot } from "react-dom/client";
import { call, on } from "../shared/bridge";
import { initTheme, onThemes, setTheme } from "../shared/themeLoader";
import { Panel } from "./Panel";
import { applyInit, onPanel, onSession, onSettings, usePanel } from "./store";

async function main() {
  await Promise.all([
    on("og://session", onSession),
    on("og://panel", onPanel),
    on("og://settings", (s) => {
      onSettings(s);
      setTheme(s);
    }),
    on("og://themes", onThemes),
  ]);
  applyInit(await call("frontend_ready"));
  await initTheme(usePanel.getState().settings!);
  createRoot(document.getElementById("root")!).render(<Panel />);
}

// No page reload in the panel (it would lose the session Rust is still showing): keep
// right-click Copy/Paste on text, drop the page menu and F5 / Ctrl+R.
window.addEventListener("contextmenu", (e) => {
  if (!(e.target instanceof HTMLTextAreaElement) && !getSelection()?.toString()) e.preventDefault();
});
window.addEventListener("keydown", (e) => {
  if (e.key === "F5" || (e.ctrlKey && e.key.toLowerCase() === "r")) e.preventDefault();
});

main().catch(console.error);
