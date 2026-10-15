// Toast (DESIGN §2.7): one --og-warn line next to the cursor for 1.6 s, then a fade.
import { useLayoutEffect, useRef } from "react";
import { useLens } from "./store";

const EDGE = 8;

export function Toast() {
  const t = useLens((s) => s.toast);
  const seq = useLens((s) => s.toastSeq);
  const ref = useRef<HTMLDivElement>(null);
  // Keep the whole line on screen.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    if (r.right > innerWidth - EDGE) el.style.left = `${innerWidth - EDGE - r.width}px`;
    if (r.bottom > innerHeight - EDGE) el.style.top = `${innerHeight - EDGE - r.height}px`;
  }, [t, seq]);
  if (!t) return null;
  return (
    <div key={seq} ref={ref} className="toast warn" style={{ left: t.x + 16, top: t.y + 16 }}>
      {t.text}
    </div>
  );
}
