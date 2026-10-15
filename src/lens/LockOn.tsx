// Lock-on style (DESIGN §2.3 ①): corner brackets around the anchor, a leader line, and a callout
// (TL;DR + 3 body lines) on snapshot glass. Hover-expanding hides the callout; brackets stay.
import type { CSSProperties } from "react";
import type { Point, Rect } from "../shared/ipc";
import { Markdown } from "../shared/Markdown";
import { body, tldr } from "../shared/tldr";
import { GlassBg } from "./Glass";
import { calloutLeader, pathD, type Placement } from "./geometry";
import type { LensSession } from "./store";

const ARM = 12;
const PAD = 4;
const CONVERGE = 14;

/** Four L-shaped corners around `r`, converging inward in 120 ms. */
function Brackets({ r, warn }: { r: Rect; warn: boolean }) {
  const L = Math.min(ARM, r.w / 2, r.h / 2);
  const [x0, y0, x1, y1] = [r.x - PAD, r.y - PAD, r.x + r.w + PAD, r.y + r.h + PAD];
  const corners: [string, number, number][] = [
    [`M${x0} ${y0 + L}V${y0}H${x0 + L}`, -1, -1],
    [`M${x1 - L} ${y0}H${x1}V${y0 + L}`, 1, -1],
    [`M${x1} ${y1 - L}V${y1}H${x1 - L}`, 1, 1],
    [`M${x0 + L} ${y1}H${x0}V${y1 - L}`, -1, 1],
  ];
  return (
    <g className={warn ? "brackets warn" : "brackets"}>
      {corners.map(([d, dx, dy], i) => (
        <path key={i} d={d} style={{ "--dx": `${dx * CONVERGE}px`, "--dy": `${dy * CONVERGE}px` } as CSSProperties} />
      ))}
    </g>
  );
}

/** Leader line drawn with stroke-dashoffset in 150 ms after `delay`, ending in a ◈ marker. */
export function Leader({ points, delay }: { points: Point[]; delay: number }) {
  const e = points[points.length - 1];
  return (
    <g className="leader" style={{ "--delay": `${delay}ms` } as CSSProperties}>
      <path className="line" d={pathD(points)} pathLength={1} />
      <path className="marker" d={`M${e.x} ${e.y - 4}l4 4l-4 4l-4 -4z`} />
    </g>
  );
}

function Callout({ s, p }: { s: LensSession; p: Placement }) {
  const head = tldr(s.text);
  const rest = body(s.text);
  const box = p.box;
  // When the leader enters from below (callout above the anchor), pin the callout's bottom edge
  // to the reserved box so the text sits on the leader end however short the answer is.
  const fromBelow = p.side !== "right" && p.side !== "left" && box.y < s.anchor.y + s.anchor.h / 2;
  const pos: CSSProperties = fromBelow
    ? { left: box.x, width: box.w, bottom: innerHeight - (box.y + box.h) }
    : { left: box.x, width: box.w, top: box.y };
  return (
    <div className="callout" data-hover="callout" style={pos}>
      <GlassBg glass={s.glass} />
      <div className="tldr">
        {head}
        {s.streaming && !rest && <span className="caret" />}
      </div>
      {rest && (
        <div className={s.streaming ? "clamp3 streaming" : "clamp3"}>
          <Markdown text={rest} streaming={s.streaming} />
        </div>
      )}
      {s.error && <div className="warn">⚠ {s.error}</div>}
    </div>
  );
}

export function LockOn({ s }: { s: LensSession }) {
  if (!s.callout) return null;
  return (
    <>
      {!s.stale && (
        <svg className="svg-layer">
          <Brackets r={s.anchor} warn={s.error !== null} />
          <Leader points={calloutLeader(s.anchor, s.callout)} delay={120} />
        </svg>
      )}
      {!s.expanded && <Callout s={s} p={s.callout} />}
    </>
  );
}
