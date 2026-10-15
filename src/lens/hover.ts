// Hover-to-expand dwell (DESIGN §2.2 rule 8). The lens is click-through and never gets mouse
// events, so the dwell is detected from og://cursor and hit-tested against [data-hover] elements.
import type { CursorEvent } from "../shared/ipc";
import { expand } from "./store";

const DWELL_MS = 250;
const SLOP_PX = 4;
let origin: { x: number; y: number } | null = null;
let timer = 0;

export function trackDwell(c: CursorEvent) {
  const p = c && !c.inPanel ? { x: c.x, y: c.y } : null;
  // Small jitter keeps the running dwell (measured from where it started, not the last event).
  if (p && origin && Math.hypot(p.x - origin.x, p.y - origin.y) < SLOP_PX) return;
  clearTimeout(timer);
  origin = p;
  if (!p) return;
  timer = window.setTimeout(() => {
    for (const el of document.querySelectorAll<HTMLElement>("[data-hover]")) {
      const r = el.getBoundingClientRect();
      if (p.x >= r.left && p.x <= r.right && p.y >= r.top && p.y <= r.bottom) return expand(el.dataset.hover ?? "");
    }
  }, DWELL_MS);
}
