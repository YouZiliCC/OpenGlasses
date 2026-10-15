// Scan results (DESIGN §2.6 stage A): label + brief per item; click toggles the detail.
import { useEffect, useRef, useState } from "react";
import type { PanelEvent, ScanItem } from "../shared/ipc";
import { Markdown } from "../shared/Markdown";

export function ScanList({ items, panel }: { items: ScanItem[]; panel: PanelEvent | null }) {
  const [open, setOpen] = useState(() => new Set(panel?.itemKey ? [panel.itemKey] : []));
  const list = useRef<HTMLOListElement>(null);

  // Hovering a scan anchor re-requests the panel with its itemKey: expand just that item.
  // Adjusted during render so the height reported for this request already includes it.
  const [seen, setSeen] = useState(panel);
  if (panel !== seen) {
    setSeen(panel);
    if (panel?.itemKey) setOpen(new Set([panel.itemKey]));
  }
  useEffect(() => {
    const key = panel?.itemKey;
    if (key) list.current?.querySelector(`[data-key="${CSS.escape(key)}"]`)?.scrollIntoView({ block: "nearest" });
  }, [panel]);

  if (!items.length) return <div className="dim">NO POINTS FOUND</div>;
  const toggle = (k: string) =>
    setOpen((o) => {
      const n = new Set(o);
      if (!n.delete(k)) n.add(k);
      return n;
    });
  return (
    <ol className="scan" ref={list}>
      {items.map((it, i) => {
        const on = open.has(it.key);
        return (
          <li key={it.key} data-key={it.key}>
            <div className="scan-head" onClick={() => toggle(it.key)}>
              <span className="dim">
                {on ? "▾" : "▸"} {String(i + 1).padStart(2, "0")}
              </span>
              <span>
                <span className="glow">{it.label}</span> <span className="dim">{it.brief}</span>
              </span>
            </div>
            {on && (
              <div className="scan-detail">
                <Markdown text={it.detail} streaming={false} />
              </div>
            )}
          </li>
        );
      })}
    </ol>
  );
}
