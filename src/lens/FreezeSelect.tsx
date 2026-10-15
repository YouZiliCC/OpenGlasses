// SELECT (DESIGN §3.4): the frozen frame with a drag rectangle. The only interactive part of the
// lens. Rust ends SELECT; we leave this mode only when og://freeze null arrives.
import { useState, type MouseEvent, type PointerEvent } from "react";
import { call } from "../shared/bridge";
import type { FreezeEvent, Point, Rect } from "../shared/ipc";
import { useLens } from "./store";

const norm = (a: Point, b: Point): Rect => ({
  x: Math.min(a.x, b.x),
  y: Math.min(a.y, b.y),
  w: Math.abs(a.x - b.x),
  h: Math.abs(a.y - b.y),
});

function done(rect: Rect | null) {
  call("selection_done", { rect, viewW: innerWidth, viewH: innerHeight }).catch(console.error);
}

function Selector({ f }: { f: NonNullable<FreezeEvent> }) {
  const [from, setFrom] = useState<Point | null>(null);
  const [to, setTo] = useState<Point | null>(null);
  const [sent, setSent] = useState(false);
  const at = (e: PointerEvent) => ({ x: e.clientX, y: e.clientY });

  const down = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || sent) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    setFrom(at(e));
    setTo(at(e));
  };
  const move = (e: PointerEvent) => {
    if (from && !sent) setTo(at(e));
  };
  const up = (e: PointerEvent) => {
    if (e.button !== 0 || !from || sent) return;
    const p = at(e);
    setTo(p);
    setSent(true);
    // Released on another monitor = cancel (never upload a crop the user abandoned).
    const outside = p.x < 0 || p.y < 0 || p.x >= innerWidth || p.y >= innerHeight;
    done(outside ? null : norm(from, p));
  };
  const cancel = (e: MouseEvent) => {
    e.preventDefault();
    if (sent) return;
    setSent(true);
    done(null);
  };

  const r = from && to ? norm(from, to) : null;
  return (
    <div className="freeze" onPointerDown={down} onPointerMove={move} onPointerUp={up} onContextMenu={cancel}>
      <img src={f.url} alt="" draggable={false} />
      {r ? (
        <div className="sel" style={{ left: r.x, top: r.y, width: r.w, height: r.h }}>
          <span className={r.y + r.h > innerHeight - 28 ? "sel-size inside" : "sel-size"}>
            {/* Physical px of the frozen frame. */}
            {Math.round((r.w * f.imgW) / innerWidth)}×{Math.round((r.h * f.imgH) / innerHeight)}
          </span>
        </div>
      ) : (
        <div className="freeze-dim" />
      )}
    </div>
  );
}

export function FreezeSelect() {
  const f = useLens((s) => s.freeze);
  return f ? <Selector key={f.url} f={f} /> : null;
}
