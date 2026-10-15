// Placement math (DESIGN §2.3). The single implementation: every box the lens draws or sends with
// show_panel comes from here. All values are CSS px of this lens (= its monitor).
import { CALLOUT_W, GAP, PANEL_W, RAIL_ENTRY_Y, RAIL_W, type Point, type Rect } from "../shared/ipc";

export interface Size {
  w: number;
  h: number;
}
export type Side = "right" | "left" | "below" | "above" | "caption";
/** A box chosen once at `start`, and the side of the anchor it ended up on. */
export interface Placement {
  side: Side;
  box: Rect;
}
/** show_panel arguments. */
export interface PanelBox {
  box: Rect;
  growUp: boolean;
}

/** Lock-on callout reserve: TL;DR (≤2 lines) + 3 body lines. */
export const CALLOUT_H = 136;
/** Caption band: ≤2 lines of 15 px text + padding. */
const CAPTION_H = 64;
const PANEL_MAX_H = 560;
const EDGE = 8;

const right = (r: Rect) => r.x + r.w;
const bottom = (r: Rect) => r.y + r.h;
/** Clamp to [lo, hi]; lo wins when the range is empty. */
const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), Math.max(lo, hi));

export const pathD = (pts: Point[]) => "M" + pts.map((p) => `${p.x} ${p.y}`).join("L");

/** Stand-in anchor when there is no selection rect: 24×18 centred on the cursor. */
export function cursorAnchor(p: Point): Rect {
  return { x: p.x - 12, y: p.y - 9, w: 24, h: 18 };
}

/** Caption band: centred, 60% of the screen wide, bottom edge 18% above the screen bottom. */
export function captionBand(work: Rect, view: Size): Rect {
  const w = view.w * 0.6;
  const b = Math.min(view.h * 0.82, bottom(work) - EDGE);
  return { x: work.x + (work.w - w) / 2, y: b - CAPTION_H, w, h: CAPTION_H };
}

/** First of right, left, below, above (GAP from the anchor) where a w×h box fits in `work`. */
function besideAnchor(a: Rect, w: number, h: number, work: Rect): Placement | null {
  const okX = (x: number) => x >= work.x && x + w <= right(work);
  const okY = (y: number) => y >= work.y && y + h <= bottom(work);
  const cx = clamp(a.x, work.x, right(work) - w);
  const cy = clamp(a.y, work.y, bottom(work) - h);
  const r = right(a) + GAP;
  const l = a.x - GAP - w;
  const b = bottom(a) + GAP;
  const t = a.y - GAP - h;
  if (okX(r)) return { side: "right", box: { x: r, y: cy, w, h } };
  if (okX(l)) return { side: "left", box: { x: l, y: cy, w, h } };
  if (okY(b)) return { side: "below", box: { x: cx, y: b, w, h } };
  if (okY(t)) return { side: "above", box: { x: cx, y: t, w, h } };
  return null;
}

/** PANEL_W column over `r`: grows down from r's top to the work-area bottom, or up from r's bottom. */
function column(r: Rect, work: Rect, dir: "down" | "up" | "auto"): PanelBox {
  const x = clamp(r.x, work.x, right(work) - PANEL_W);
  const top = clamp(r.y, work.y, bottom(work));
  const bot = clamp(bottom(r), work.y, bottom(work));
  const room = bottom(work) - top;
  const up = dir === "up" || (dir === "auto" && room < Math.min(PANEL_MAX_H, work.h));
  return up
    ? { box: { x, y: work.y, w: PANEL_W, h: bot - work.y }, growUp: true }
    : { box: { x, y: top, w: PANEL_W, h: room }, growUp: false };
}

/** Lock-on callout box (CALLOUT_W × CALLOUT_H); falls back to the caption position. */
export function placeCallout(a: Rect, work: Rect, view: Size): Placement {
  const band = captionBand(work, view);
  return (
    besideAnchor(a, CALLOUT_W, CALLOUT_H, work) ?? {
      side: "caption",
      box: { x: band.x + (band.w - CALLOUT_W) / 2, y: bottom(band) - CALLOUT_H, w: CALLOUT_W, h: CALLOUT_H },
    }
  );
}

/** Panel above the caption band, growing upward (covers the caption). */
export function captionPanel(work: Rect, view: Size): PanelBox {
  const band = captionBand(work, view);
  return column({ x: band.x + (band.w - PANEL_W) / 2, y: band.y, w: PANEL_W, h: band.h }, work, "up");
}

/** Card / scan-list panel at `start`: reserve PANEL_W × min(560, work height) beside the anchor. */
export function placeCard(a: Rect, work: Rect, view: Size): PanelBox {
  const p = besideAnchor(a, PANEL_W, Math.min(PANEL_MAX_H, work.h), work);
  if (!p) return captionPanel(work, view);
  return column(p.box, work, p.side === "above" ? "up" : "down");
}

/** Hover-expanded Lock-on: the panel opens over the callout, from its top-left corner. */
export function expandCallout(p: Placement, work: Rect): PanelBox {
  const c = p.box;
  const x = p.side === "left" ? right(c) - PANEL_W : p.side === "caption" ? c.x + (c.w - PANEL_W) / 2 : c.x;
  const dir = p.side === "below" ? "down" : p.side === "above" || p.side === "caption" ? "up" : "auto";
  return column({ x, y: c.y, w: PANEL_W, h: c.h }, work, dir);
}

/** Rail: docked on the right edge of the work area, 70% of its height, vertically centred. */
export function railBox(work: Rect): Rect {
  const h = Math.round(work.h * 0.7);
  return { x: right(work) - RAIL_W, y: work.y + Math.round((work.h - h) / 2), w: RAIL_W, h };
}

/** Scan anchor dot: middle of the item's right edge. */
export function dotCenter(r: Rect): Point {
  return { x: right(r), y: r.y + r.h / 2 };
}

/** Panel next to a scan dot (right if it fits, else left). */
export function panelBesideDot(d: Point, work: Rect): PanelBox {
  const x = d.x + 16 + PANEL_W <= right(work) ? d.x + 16 : d.x - 16 - PANEL_W;
  return column({ x, y: d.y - 24, w: PANEL_W, h: 48 }, work, "auto");
}

/** Lock-on leader: anchor edge → elbow → callout entry (first text line). */
export function calloutLeader(a: Rect, p: Placement): Point[] {
  const c = p.box;
  const P = 4; // bracket padding
  const E = 16; // final straight segment into the callout
  const acy = a.y + a.h / 2;
  if (p.side === "right") return [{ x: right(a) + P, y: acy }, { x: c.x - E, y: c.y + 10 }, { x: c.x, y: c.y + 10 }];
  if (p.side === "left") return [{ x: a.x - P, y: acy }, { x: right(c) + E, y: c.y + 10 }, { x: right(c), y: c.y + 10 }];
  // below / above / caption: enter from the edge that faces the anchor.
  const down = c.y >= acy;
  const ex = c.x + 10;
  const ey = down ? c.y : bottom(c);
  return [
    { x: a.x + a.w / 2, y: down ? bottom(a) + P : a.y - P },
    { x: ex, y: down ? ey - E : ey + E },
    { x: ex, y: ey },
  ];
}

/** Rail leader: anchor → the rail's "current session" row. */
export function railLeader(a: Rect, rail: Rect): Point[] {
  const end = { x: rail.x, y: rail.y + RAIL_ENTRY_Y };
  return [{ x: right(a) + 4, y: a.y + a.h / 2 }, { x: end.x - 24, y: end.y }, end];
}
