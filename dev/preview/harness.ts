// The fake Rust core behind the preview (DESIGN §10.8). It answers every command the frames send
// (mock.ts posts them here), emits events into them, places the panel frame the way windows.rs
// places the panel window, and plays the scenarios picked in the control bar. shoot.mjs drives
// the same scenarios through `window.preview`.
import type {
  CommandMap,
  EventMap,
  Glass,
  HudState,
  Kind,
  PanelEvent,
  Point,
  Rect,
  Session,
  Settings,
  Style,
  ThemeInfo,
  ThemesEvent,
} from "../../src/shared/ipc";
import { SPEED_FACTOR } from "../../src/shared/ipc";
import * as data from "./data";

type Label = "lens-1" | "panel" | "settings";
const LENS = "lens-1";
const el = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const frames: Record<Label, HTMLIFrameElement> = { [LENS]: el("lens"), panel: el("panel"), settings: el("settings") };
const pages: Record<Label, string> = { [LENS]: "lens.html", panel: "panel.html", settings: "settings.html" };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

// ---- Themes: the repo's themes/ folder plays the built-in theme dir. Edits to theme.css hot
// reload through Vite's own CSS HMR in each frame; a new theme folder reloads this page.

type Manifest = Pick<ThemeInfo, "name" | "author" | "version" | "description" | "bootMs">;
const manifests = import.meta.glob<Manifest>("/themes/*/theme.json", { eager: true, import: "default" });

function listThemes(): ThemesEvent {
  const themes = Object.entries(manifests).map(([path, m]) => {
    const id = path.split("/")[2];
    return { id, ...m, builtin: true, cssUrl: `/themes/${id}/theme.css` };
  });
  return { themes, rev: 1 };
}

// ---- State the Rust core would hold.

const params = new URLSearchParams(location.search);
const settings: Settings = data.defaultSettings();
settings.theme = params.get("theme") ?? settings.theme;
settings.animSpeed = (params.get("speed") as Settings["animSpeed"]) ?? settings.animSpeed;
const keys = new Set(data.KEYS);
let hudState: HudState = { ...data.HUD };
let session: Session | null = null;
let panelReq: PanelEvent | null = null;
let selecting = false;
let seq = 0;

// ---- Frames: the iframes play the windows. Rust sends nothing before a window's frontend_ready.

const ready = new Map<Label, { done: boolean; promise: Promise<void>; resolve: () => void }>();
function unready(l: Label) {
  if (ready.get(l)?.done === false) return;
  let resolve = () => {};
  const promise = new Promise<void>((r) => (resolve = r));
  ready.set(l, { done: false, promise, resolve });
}
(Object.keys(frames) as Label[]).forEach(unready);

function load(l: Label) {
  unready(l);
  frames[l].src = pages[l];
}

function emit<K extends keyof EventMap>(to: Label | Label[], event: K, payload: EventMap[K]) {
  for (const l of [to].flat()) {
    if (ready.get(l)?.done) frames[l].contentWindow?.postMessage({ og: "event", event, payload }, "*");
  }
}
const emitAll = <K extends keyof EventMap>(event: K, payload: EventMap[K]) => emit([LENS, "panel", "settings"], event, payload);

function hud(p: Partial<HudState>) {
  hudState = { ...hudState, ...p };
  emit(LENS, "og://hud", hudState);
}

// ---- Panel window placement (windows.rs request_panel / panel_set_height).

function place(f: HTMLIFrameElement, r: Rect) {
  Object.assign(f.style, { left: `${r.x}px`, top: `${r.y}px`, width: `${r.w}px`, height: `${r.h}px` });
}

function clampToWork(r: Rect): Rect {
  const wa = data.WORK_AREA;
  const w = Math.min(r.w, wa.w);
  const h = Math.min(r.h, wa.h);
  return { x: clamp(r.x, wa.x, wa.x + wa.w - w), y: clamp(r.y, wa.y, wa.y + wa.h - h), w, h };
}

function hidePanel() {
  panelReq = null;
  frames.panel.style.visibility = "hidden";
}

// ---- Sessions (ai/mod.rs).

function dismiss() {
  if (!session) return;
  emit([LENS, "panel"], "og://session", { type: "dismiss", id: session.id });
  session = null;
  setTimeout(() => !panelReq && hidePanel(), 150);
  panelReq = null;
}

function start(kind: Kind, style: Style, anchor: Rect, cursor: Point, glass: Glass | null, preview: string) {
  dismiss();
  hidePanel();
  const id = `s${++seq}`;
  const s: Session = {
    id,
    kind,
    style,
    lens: LENS,
    anchor,
    cursor,
    glass,
    messages: [
      { role: "user", text: preview },
      { role: "assistant", text: "" },
    ],
    streaming: true,
    error: null,
    items: null,
    pinned: false,
  };
  session = s;
  emit([LENS, "panel"], "og://session", { type: "start", id, kind, style, lens: LENS, anchor, cursor, glass, preview });
  hud({ mode: kind === "scan" ? "SCAN" : "THINKING" });
  return s;
}

/** Stream `text` as message `msgIndex` in ~40 ms batches, like the SSE parser flushes it. */
async function stream(s: Session, msgIndex: number, text: string) {
  // A new session (or a dismiss) ends this one.
  const live = () => session === s;
  await sleep(450);
  if (!live()) return;
  const sent = performance.now();
  hud({ mode: "STREAM", ttftMs: 450 });
  for (let i = 0; i < text.length && live(); i += 9) {
    const chunk = text.slice(i, i + 9);
    s.messages[msgIndex].text += chunk;
    emit([LENS, "panel"], "og://session", { type: "delta", id: s.id, msgIndex, text: chunk });
    await sleep(40);
  }
  if (!live()) return;
  s.streaming = false;
  emit([LENS, "panel"], "og://session", { type: "done", id: s.id, msgIndex });
  const tokens = Math.round(text.length / 2);
  hud({ mode: "STANDBY", tps: Math.round(tokens / ((performance.now() - sent) / 1000)), tokensToday: hudState.tokensToday + tokens });
}

function endSelect() {
  selecting = false;
  frames[LENS].style.pointerEvents = "";
  emit(LENS, "og://freeze", null);
}

// ---- Commands. Every entry of CommandMap must be answered (checked by tsc -p dev/preview).

type Handlers = {
  [K in keyof CommandMap]: (a: CommandMap[K][0], from: Label) => CommandMap[K][1] | Promise<CommandMap[K][1]>;
};

const commands: Handlers = {
  frontend_ready: (_, from) => {
    const r = ready.get(from)!;
    r.done = true;
    r.resolve();
    return {
      isPrimary: from === LENS,
      workArea: from === LENS ? data.WORK_AREA : null,
      settings,
      hud: from === LENS ? hudState : null,
      glassesOn: true,
    };
  },
  save_settings: ({ settings: next }) => {
    const replay = next.theme !== settings.theme || next.animSpeed !== settings.animSpeed;
    Object.assign(settings, next);
    syncControls();
    emitAll("og://settings", settings);
    if (replay) replayBoot();
    return { hotkeyErrors: {} };
  },
  set_api_key: ({ id }) => void keys.add(id),
  has_api_key: ({ id }) => keys.has(id),
  delete_api_key: ({ id }) => void keys.delete(id),
  test_provider: async ({ provider }) => {
    await sleep(700);
    if (!keys.has(provider.id)) throw "NO KEY: no API key stored for this provider";
    return "Hello! I'm ready.";
  },
  selection_done: ({ rect }) => {
    if (!selecting) return;
    endSelect();
    if (!rect || rect.w < 8 || rect.h < 8) return void hud({ mode: "STANDBY" });
    const glass = { url: data.DESKTOP_URL, w: data.SCREEN.w, h: data.SCREEN.h };
    const s = start("image", settings.style, rect, { x: rect.x + rect.w, y: rect.y + rect.h }, glass, "[screenshot]");
    void stream(s, 1, data.IMAGE_ANSWER);
  },
  show_panel: ({ id, box, growUp, itemKey }) => {
    if (session?.id !== id) return;
    const mode = session.style === "rail" && session.kind !== "scan" ? "rail" : "card";
    // Pre-placed while hidden so the panel lays out at the target width.
    if (frames.panel.style.visibility !== "visible") place(frames.panel, clampToWork(box));
    panelReq = { id, mode, box, growUp, itemKey };
    emit("panel", "og://panel", panelReq);
  },
  panel_set_height: ({ id, cssH }) => {
    if (panelReq?.id !== id || !Number.isFinite(cssH)) return;
    const b = panelReq.box;
    const h = clamp(cssH, 1, Math.max(1, b.h));
    place(frames.panel, clampToWork({ x: b.x, y: panelReq.growUp ? b.y + b.h - h : b.y, w: b.w, h }));
    frames.panel.style.visibility = "visible";
  },
  ask_followup: ({ id, text }) => {
    const s = session;
    if (s?.id !== id || s.streaming) return;
    const k = s.messages.length;
    s.messages.push({ role: "user", text }, { role: "assistant", text: "" });
    s.streaming = true;
    s.pinned = true;
    emit([LENS, "panel"], "og://session", { type: "user", id, msgIndex: k, text });
    hud({ mode: "THINKING" });
    void stream(s, k + 1, data.FOLLOWUP_ANSWER);
  },
  dismiss_session: ({ id }) => {
    if (session?.id === id) dismiss();
  },
  pin_session: ({ id, pinned }) => {
    if (session?.id === id) session.pinned = pinned;
  },
  open_settings: () => openSettings(),
  get_history: () => data.HISTORY,
  list_themes: listThemes,
  open_themes_folder: () => void status("open_themes_folder: the preview reads the repo's themes/ folder"),
};

/** Plugin commands the windows may call (window controls of the custom title bar, clipboard, links). */
const plugins: Record<string, (a: Record<string, unknown>, from: Label) => unknown> = {
  "plugin:window|minimize": (_, from) => closeWindow(from),
  "plugin:window|close": (_, from) => closeWindow(from),
  "plugin:window|destroy": (_, from) => closeWindow(from),
  "plugin:window|is_maximized": () => false,
  "plugin:clipboard-manager|write_text": (a) => navigator.clipboard.writeText(String(a.text)),
  "plugin:opener|open_url": (a) => void open(String(a.url), "_blank", "noopener"),
};

async function openSettings() {
  frames.settings.style.display = "block";
  if (!frames.settings.getAttribute("src")) load("settings");
  await ready.get("settings")!.promise;
}

function closeWindow(l: Label) {
  if (l !== "settings") return;
  frames.settings.style.display = "none";
  frames.settings.removeAttribute("src");
  unready("settings");
}

addEventListener("message", async (e: MessageEvent) => {
  const m = e.data;
  const from = m?.label as Label;
  if (!frames[from] || e.source !== frames[from].contentWindow) return;
  if (m.og === "hello") return unready(from); // the frame (re)loaded
  if (m.og !== "invoke") return;
  const reply = (r: object) => frames[from].contentWindow?.postMessage({ og: "reply", id: m.id, ...r }, "*");
  try {
    const fn = (commands as Record<string, (a: unknown, from: Label) => unknown>)[m.cmd] ?? plugins[m.cmd];
    if (!fn && m.cmd.startsWith("plugin:")) console.warn(`[preview] ${m.cmd} is a no-op here`);
    else if (!fn) throw `unknown command ${m.cmd}`;
    reply({ ok: true, result: (await fn?.(m.args, from)) ?? null });
  } catch (err) {
    reply({ ok: false, error: String(err) });
  }
});

// ---- Screenshot-derived images: Lens-On frames (capture.rs: 1/32, 1/12, 1/4, most blurred
// first) and the snapshot glass (1/8).

async function downscale(img: ImageBitmap, div: number): Promise<string> {
  const c = new OffscreenCanvas(Math.round(img.width / div), Math.round(img.height / div));
  const g = c.getContext("2d")!;
  g.imageSmoothingQuality = "high";
  g.drawImage(img, 0, 0, c.width, c.height);
  return URL.createObjectURL(await c.convertToBlob({ type: "image/jpeg", quality: 0.8 }));
}
const desktop = await createImageBitmap(await (await fetch(data.DESKTOP_URL)).blob());
const bootFrames = await Promise.all([32, 12, 4].map((d) => downscale(desktop, d)));
const glass: Glass = { url: await downscale(desktop, 8), w: data.SCREEN.w, h: data.SCREEN.h };

// ---- Scenarios.

const SCENARIOS = ["boot", "boot-bare", "lockon", "caption", "card", "rail", "scan", "freeze", "toast", "settings"];

const theme = () => listThemes().themes.find((t) => t.id === settings.theme);
const bootTotal = () => Math.round((theme()?.bootMs ?? 1000) * SPEED_FACTOR[settings.animSpeed]);

/** Wait for the SELECT to end and the answer to finish streaming. */
async function idle() {
  while (selecting || session?.streaming) await sleep(50);
  await sleep(400); // panel height report + intro animations
}

/** Play one scenario; resolves once it reached its steady state (unless `wait` is false). */
async function play(name: string, wait = true) {
  if (selecting) endSelect();
  if (name !== "settings") closeWindow("settings");
  await Promise.all([ready.get(LENS)!.promise, ready.get("panel")!.promise]);
  const sel = data.AT.selection;
  const atSel = { x: sel.x + sel.w, y: sel.y + sel.h - 4 };
  switch (name) {
    case "boot":
    case "boot-bare":
      dismiss();
      hidePanel();
      hud({ mode: "STANDBY" });
      emit(LENS, "og://visibility", { on: true });
      emit(LENS, "og://lens-on", { frames: name === "boot" ? bootFrames : null });
      if (wait) await sleep(bootTotal() + 200);
      return;
    case "lockon":
    case "caption":
    case "card":
    case "rail": {
      hud({ app: "msedge.exe" });
      const s = start("text", name, sel, atSel, glass, data.SOURCE_TEXT);
      const done = stream(s, 1, data.TEXT_ANSWER);
      if (wait) await done.then(idle);
      return;
    }
    case "scan": {
      hud({ app: "Code.exe" });
      // Rust anchors a scan on the foreground window (here the IDE).
      const s = start("scan", settings.style, data.AT.ide, { x: 760, y: 290 }, glass, "[screenshot]");
      const done = sleep(900).then(() => {
        if (session !== s) return;
        s.items = data.SCAN_ITEMS;
        s.streaming = false;
        emit([LENS, "panel"], "og://session", { type: "scan", id: s.id, items: s.items });
        hud({ mode: "STANDBY" });
      });
      if (wait) await done.then(idle);
      return;
    }
    case "freeze":
      dismiss();
      hidePanel();
      hud({ app: "WindowsTerminal.exe", mode: "SELECT" });
      selecting = true;
      frames[LENS].style.pointerEvents = "auto"; // set_lens_interactive(on)
      emit(LENS, "og://freeze", { url: data.DESKTOP_URL, imgW: data.SCREEN.w, imgH: data.SCREEN.h });
      if (wait) await sleep(300);
      status("drag a rectangle on the frozen frame (right-click cancels)");
      return;
    case "toast":
      dismiss();
      emit(LENS, "og://toast", { text: "NO SELECTION", x: atSel.x, y: atSel.y });
      if (wait) await sleep(300);
      return;
    case "settings":
      dismiss();
      await openSettings();
      if (wait) await sleep(600);
      return;
  }
}

// ---- Control bar, fake cursor, fit-to-window.

const pick = { theme: el<HTMLSelectElement>("theme"), speed: el<HTMLSelectElement>("speed"), scenario: el<HTMLSelectElement>("scenario") };

function status(text?: string) {
  const t = theme();
  const info = t
    ? `${t.name} ${t.version} by ${t.author} · boot ${t.bootMs} ms × ${SPEED_FACTOR[settings.animSpeed]} = ${bootTotal()} ms`
    : `theme "${settings.theme}" not found`;
  el("status").textContent = text ? `${info} · ${text}` : info;
}

function syncControls() {
  pick.theme.replaceChildren(
    ...listThemes().themes.map((t) => new Option(`${t.name} (${t.id})`, t.id, false, t.id === settings.theme)),
  );
  pick.speed.value = settings.animSpeed;
  const url = new URL(location.href);
  url.searchParams.set("theme", settings.theme);
  url.searchParams.set("speed", settings.animSpeed);
  url.searchParams.set("scenario", pick.scenario.value);
  history.replaceState(null, "", url);
  status();
}

pick.scenario.replaceChildren(...SCENARIOS.map((s) => new Option(s, s)));
pick.scenario.value = SCENARIOS.includes(params.get("scenario") ?? "") ? params.get("scenario")! : "boot";
syncControls();

pick.theme.onchange = pick.speed.onchange = () => {
  settings.theme = pick.theme.value;
  settings.animSpeed = pick.speed.value as Settings["animSpeed"];
  syncControls();
  emitAll("og://settings", settings);
  replayBoot();
};

/** Like actions::apply_settings: a theme/speed switch replays the boot (unless Lens-On is off). */
function replayBoot() {
  if (settings.lensOn) emit(LENS, "og://lens-on", { frames: bootFrames });
}
pick.scenario.onchange = () => {
  syncControls();
  void play(pick.scenario.value);
};
el("play").onclick = () => void play(pick.scenario.value);

const monitor = el("monitor");
let scale = 1;
function fit() {
  const stage = el("stage");
  scale = Math.min(1, stage.clientWidth / data.SCREEN.w, stage.clientHeight / data.SCREEN.h);
  monitor.style.transform = `scale(${scale})`;
  Object.assign(el("fit").style, { width: `${data.SCREEN.w * scale}px`, height: `${data.SCREEN.h * scale}px` });
}
addEventListener("resize", fit);
fit();

// og://cursor (cursor.rs): drives the lens's hover-to-expand dwell. The parent page gets no
// mouse events while the pointer is over the (interactive) panel frame.
let cursor: Point = { x: 0, y: 0 };
monitor.addEventListener("mousemove", (e) => {
  const r = monitor.getBoundingClientRect();
  cursor = { x: (e.clientX - r.left) / scale, y: (e.clientY - r.top) / scale };
  emit(LENS, "og://cursor", { ...cursor, inPanel: false });
});
monitor.addEventListener("mouseleave", () => emit(LENS, "og://cursor", null));
frames.panel.addEventListener("mouseenter", () => emit(LENS, "og://cursor", { ...cursor, inPanel: true }));

// Telemetry ticks once a second.
setInterval(() => {
  const gpu = hudState.gpu && { ...hudState.gpu, util: Math.round(5 + Math.random() * 12) };
  hud({ cpu: Math.round(14 + Math.random() * 16), gpu });
}, 1000);

load(LENS);
load("panel");
const allReady = Promise.all([ready.get(LENS)!.promise, ready.get("panel")!.promise]);

// For shoot.mjs and the devtools console, e.g. preview.emit("lens-1", "og://toast", {text: "HI", x: 800, y: 400}).
Object.assign(window, {
  preview: { ready: allReady, play, idle, bootTotal, emit, selectRect: data.SELECT_RECT },
});

await allReady;
if (params.get("autoplay") !== "0") void play(pick.scenario.value);
