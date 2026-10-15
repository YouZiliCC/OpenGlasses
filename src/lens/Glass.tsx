// Snapshot glass (DESIGN §1.1): the tinted monitor thumbnail behind a callout/caption, aligned to
// screen coordinates, with feathered edges instead of a border. Parent must be positioned.
import { useLayoutEffect, useRef } from "react";
import type { Glass } from "../shared/ipc";

export function GlassBg({ glass }: { glass: Glass | null }) {
  const ref = useRef<HTMLDivElement>(null);
  // Re-align after every render: the box follows the text (the caption is centred, so its x moves).
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !glass) return;
    const r = el.getBoundingClientRect();
    el.style.backgroundPosition = `0 0, 0 0, ${-r.left}px ${-r.top}px`;
  });
  if (!glass) return <div className="glass fallback" />;
  return (
    <div
      ref={ref}
      className="glass"
      style={{
        backgroundImage: `var(--scanlines), linear-gradient(var(--glass-tint), var(--glass-tint)), url("${glass.url}")`,
        backgroundSize: `auto, auto, ${glass.w}px ${glass.h}px`,
      }}
    />
  );
}
