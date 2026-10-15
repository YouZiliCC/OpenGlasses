// Scan anchors (DESIGN §2.6 stage B): a dot on the right edge of each located item, breathing
// 3 times. Items without a rect are listed only in the panel.
import { dotCenter } from "./geometry";
import type { LensSession } from "./store";

export function Anchors({ s }: { s: LensSession }) {
  if (s.stale || !s.items) return null;
  return s.items.map((it) => {
    if (!it.rect) return null;
    const c = dotCenter(it.rect);
    // The element is a 24 px hit box for the hover dwell; the 8 px dot is its ::after.
    return <div key={it.key} className="anchor" data-hover={`item:${it.key}`} style={{ left: c.x, top: c.y }} />;
  });
}
