// Lens state (DESIGN §3.8 rule 2). Written by the listeners in main.tsx and the actions below;
// components only read it.
import { create } from "zustand";
import { call } from "../shared/bridge";
import type {
  CursorEvent,
  FreezeEvent,
  Glass,
  HudState,
  InitState,
  Kind,
  LensOnEvent,
  Rect,
  ScanItem,
  Session,
  SessionEvent,
  Settings,
  Style,
  ToastEvent,
} from "../shared/ipc";
import * as geo from "./geometry";

export interface LensSession {
  id: string;
  kind: Kind;
  style: Style;
  /** Never null: falls back to a 24×18 box at the cursor. */
  anchor: Rect;
  glass: Glass | null;
  /** Answer msgIndex 1 only; follow-ups live in the panel. */
  text: string;
  streaming: boolean;
  error: string | null;
  items: ScanItem[] | null;
  stale: boolean;
  /** The panel is open for this session (it covers the callout/caption). */
  expanded: boolean;
  /** Scan item the panel was last moved to. */
  item: string | null;
  /** Lock-on callout, placed once at start. */
  callout: geo.Placement | null;
  /** Rail panel box (Rail only). */
  rail: Rect | null;
  /** Dismissed: fading out for 150 ms. */
  closing: boolean;
}

interface LensState {
  isPrimary: boolean;
  workArea: Rect | null;
  settings: Settings | null;
  hud: HudState | null;
  visible: boolean;
  session: LensSession | null;
  cursor: CursorEvent;
  toast: ToastEvent | null;
  toastSeq: number;
  /** Lens-On currently playing; `lensOnSeq` keeps counting after it ends (HUD light-up key). */
  lensOn: LensOnEvent | null;
  lensOnSeq: number;
  freeze: FreezeEvent;
}

export const useLens = create<LensState>()(() => ({
  isPrimary: false,
  workArea: null,
  settings: null,
  hud: null,
  visible: true,
  session: null,
  cursor: null,
  toast: null,
  toastSeq: 0,
  lensOn: null,
  lensOnSeq: 0,
  freeze: null,
}));

const get = useLens.getState;
const set = useLens.setState;

export const viewSize = (): geo.Size => ({ w: window.innerWidth, h: window.innerHeight });
export const workArea = (): Rect => get().workArea ?? { x: 0, y: 0, ...viewSize() };

export function applyInit(init: InitState) {
  const cur = get();
  set({
    isPrimary: init.isPrimary,
    workArea: init.workArea,
    // Events may race ahead of the frontend_ready reply; those are newer than the snapshot.
    settings: cur.settings ?? init.settings,
    hud: cur.hud ?? init.hud,
    visible: init.glassesOn,
  });
  const a = init.active;
  if (a && a.lens === init.label && !cur.session) restore(a, init.panel?.id === a.id);
}

function showPanel(id: string, p: geo.PanelBox, itemKey: string | null = null) {
  call("show_panel", { id, box: p.box, growUp: p.growUp, itemKey }).catch(console.error);
}

function patch(id: string, p: Partial<LensSession>) {
  set((s) => (s.session?.id === id ? { session: { ...s.session, ...p } } : {}));
}

type Start = Extract<SessionEvent, { type: "start" }>;

/** Show the thinking state at once; Card, Rail and scan sessions open the panel right away. */
function start(ev: Start, panelShown = false) {
  const work = workArea();
  const view = viewSize();
  const anchor = ev.anchor ?? geo.cursorAnchor(ev.cursor);
  let callout: geo.Placement | null = null;
  let rail: Rect | null = null;
  let panel: geo.PanelBox | null = null;
  if (ev.kind === "scan") panel = geo.placeCard(geo.cursorAnchor(ev.cursor), work, view);
  else if (ev.style === "card") panel = geo.placeCard(anchor, work, view);
  else if (ev.style === "rail") panel = { box: (rail = geo.railBox(work)), growUp: false };
  else if (ev.style === "lockon") callout = geo.placeCallout(anchor, work, view);
  set({
    session: {
      id: ev.id,
      kind: ev.kind,
      style: ev.style,
      anchor,
      glass: ev.glass,
      text: "",
      streaming: true,
      error: null,
      items: null,
      stale: false,
      expanded: panel !== null || panelShown,
      item: null,
      callout,
      rail,
      closing: false,
    },
  });
  if (panel && !panelShown) showPanel(ev.id, panel);
}

/** Re-attach to the active session after a webview reload. */
function restore(a: Session, panelShown: boolean) {
  const { id, kind, style, lens, anchor, cursor, glass } = a;
  start({ type: "start", id, kind, style, lens, anchor, cursor, glass, preview: "" }, panelShown);
  patch(id, { text: a.messages[1]?.text ?? "", streaming: a.streaming, error: a.error, items: a.items });
}

export function onSession(ev: SessionEvent) {
  if (ev.type === "start") return start(ev);
  const s = get().session;
  if (!s || s.id !== ev.id) return;
  switch (ev.type) {
    case "delta":
      if (ev.msgIndex === 1) patch(s.id, { text: s.text + ev.text });
      break;
    case "done":
      if (ev.msgIndex === 1) patch(s.id, { streaming: false });
      break;
    case "error":
      if (ev.msgIndex === 1) patch(s.id, { streaming: false, error: ev.code });
      break;
    case "scan":
      patch(s.id, { items: ev.items, streaming: false });
      break;
    case "stale":
      patch(s.id, { stale: true });
      break;
    case "dismiss":
      patch(s.id, { closing: true });
      setTimeout(() => set((st) => (st.session?.id === s.id ? { session: null } : {})), 150);
      break;
  }
}

/** Hover dwell landed on a `data-hover` target: open the panel there (DESIGN §2.2 rule 8). */
export function expand(target: string) {
  const s = get().session;
  if (!s || s.closing) return;
  const work = workArea();
  if (target.startsWith("item:")) {
    const key = target.slice(5);
    const rect = s.items?.find((i) => i.key === key)?.rect;
    if (!rect || s.item === key) return;
    showPanel(s.id, geo.panelBesideDot(geo.dotCenter(rect), work), key);
    patch(s.id, { item: key, expanded: true });
  } else if (!s.expanded && (target === "callout" || target === "caption")) {
    showPanel(s.id, s.callout ? geo.expandCallout(s.callout, work) : geo.captionPanel(work, viewSize()));
    patch(s.id, { expanded: true });
  }
}

export function onToast(t: ToastEvent) {
  const seq = get().toastSeq + 1;
  set({ toast: t, toastSeq: seq });
  setTimeout(() => {
    if (get().toastSeq === seq) set({ toast: null });
  }, 1800);
}

export function onLensOn(ev: LensOnEvent) {
  const seq = get().lensOnSeq + 1;
  set({ lensOn: ev, lensOnSeq: seq });
  setTimeout(() => {
    if (get().lensOnSeq === seq) set({ lensOn: null });
  }, 700);
}
