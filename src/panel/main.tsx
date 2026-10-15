// Panel entry: register listeners, then frontend_ready, then render (DESIGN §3.8 rules 2–3).
import "../shared/theme.css";
import "./panel.css";
import { createRoot } from "react-dom/client";
import { call, on } from "../shared/bridge";
import { Panel } from "./Panel";
import { applyInit, onPanel, onSession, onSettings } from "./store";

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
    on("og://settings", gate(onSettings)),
  ]);
  try {
    applyInit(await call("frontend_ready"));
  } catch (e) {
    console.error("frontend_ready", e);
  }
  const queued = pending ?? [];
  pending = null;
  queued.forEach((f) => f());
  createRoot(document.getElementById("root")!).render(<Panel />);
}

void main();
