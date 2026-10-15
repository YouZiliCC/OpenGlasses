// IPC contract between the Rust core and the three webviews (lens-*, panel, settings).
// This file is authoritative: src-tauri/src/types.rs mirrors it field by field.
// Rules (docs/DESIGN.md §3.8): optional values are `T | null`, never `?:`;
// all coordinates are CSS px of the lens/monitor that owns the session.

export type Rect = { x: number; y: number; w: number; h: number };
export type Point = { x: number; y: number };

export type Style = "lockon" | "rail" | "caption" | "card";
export type Kind = "text" | "image" | "scan";
export type HotkeyAction = "explain" | "capture" | "scan" | "toggle";
export type ProviderKind = "openai" | "anthropic";
export type BboxFormat = "pixel" | "norm1000";
export type GlassMode = "snapshot" | "acrylic";
export type PanelMode = "card" | "rail";

export interface Provider {
  id: string;
  name: string;
  kind: ProviderKind;
  baseUrl: string;
  model: string;
  visionModel: string | null;
  /** JSON Merge Patch (RFC 7396) applied to every request body. */
  extraBody: Record<string, unknown> | null;
  bboxFormat: BboxFormat;
}

export interface HudCorners {
  tl: boolean;
  tr: boolean;
  bl: boolean;
  br: boolean;
}

export interface Settings {
  /** Always the 8 fixed slots, in display order. */
  providers: Provider[];
  activeProviderId: string;
  /** Accelerator text, e.g. "Alt+Q", "Ctrl+Alt+G". */
  hotkeys: Record<HotkeyAction, string>;
  style: Style;
  hudCorners: HudCorners;
  lensOn: boolean;
  glass: GlassMode;
  /** "auto" (follow the source text) or a language name/BCP-47 tag such as "zh-CN". */
  answerLanguage: string;
  includeWindowTitle: boolean;
  proxy: string | null;
  maxTokens: number;
  excludeFromCapture: boolean;
}

export type HudMode = "STANDBY" | "GRABBING" | "SELECT" | "THINKING" | "STREAM" | "SCAN";

export interface GpuStat {
  name: string;
  util: number;
  vramUsedGb: number;
  vramTotalGb: number;
}

export interface HudState {
  mode: HudMode;
  /** Foreground process name, e.g. "Code.exe". */
  app: string | null;
  cpu: number;
  ramUsedGb: number;
  ramTotalGb: number;
  gpu: GpuStat | null;
  provider: string;
  model: string;
  ttftMs: number | null;
  tps: number | null;
  tokensToday: number;
}

/** Snapshot-glass background: a downscaled, tinted capture of the whole monitor. */
export interface Glass {
  url: string;
  /** CSS size of the monitor the snapshot covers (= the lens viewport). */
  w: number;
  h: number;
}

export interface ScanItem {
  key: string;
  label: string;
  brief: string;
  detail: string;
  /** Lens CSS px; null when the model returned no usable box (stage A only). */
  rect: Rect | null;
}

export interface Message {
  role: "user" | "assistant";
  text: string;
}

export interface Session {
  id: string;
  kind: Kind;
  style: Style;
  /** Label of the lens window that owns (draws) this session. */
  lens: string;
  anchor: Rect | null;
  cursor: Point;
  glass: Glass | null;
  /** messages[0] = user (source text / caption), messages[1] = first answer. */
  messages: Message[];
  streaming: boolean;
  /** Short code of the last error, e.g. "AUTH 401". */
  error: string | null;
  items: ScanItem[] | null;
  pinned: boolean;
}

export type SessionEvent =
  | {
      type: "start";
      id: string;
      kind: Kind;
      style: Style;
      lens: string;
      anchor: Rect | null;
      cursor: Point;
      glass: Glass | null;
      preview: string;
    }
  | { type: "user"; id: string; msgIndex: number; text: string }
  | { type: "delta"; id: string; msgIndex: number; text: string }
  | { type: "done"; id: string; msgIndex: number }
  | { type: "error"; id: string; msgIndex: number; code: string; message: string }
  | { type: "scan"; id: string; items: ScanItem[] }
  /** The source window moved/lost focus: hide brackets and leader lines, keep the text. */
  | { type: "stale"; id: string }
  | { type: "dismiss"; id: string };

export interface PanelEvent {
  id: string;
  mode: PanelMode;
  /** Maximum box the panel may occupy, in CSS px of the session's lens/monitor. */
  box: Rect;
  /** true: the panel's bottom edge sticks to box bottom and it grows upward. */
  growUp: boolean;
  /** Scan item to expand, if any. */
  itemKey: string | null;
}

export interface HistoryEntry {
  id: string;
  kind: Kind;
  tldr: string;
  text: string;
  createdAt: number;
}

export interface InitState {
  label: string;
  isPrimary: boolean;
  /** Lens only: the monitor work area in this lens's CSS px. */
  workArea: Rect | null;
  settings: Settings;
  hud: HudState | null;
  active: Session | null;
  panel: PanelEvent | null;
  glassesOn: boolean;
}

export type CursorEvent = { x: number; y: number; inPanel: boolean } | null;
export interface ToastEvent {
  text: string;
  x: number;
  y: number;
}
export interface LensOnEvent {
  /** Most blurred first. null = play the no-screenshot variant. */
  frames: string[] | null;
}
export type FreezeEvent = { url: string; imgW: number; imgH: number } | null;
export interface VisibilityEvent {
  on: boolean;
}

/** Event names → payload types. Every listener must use the current window's `listen`. */
export interface EventMap {
  "og://cursor": CursorEvent;
  "og://hud": HudState;
  "og://toast": ToastEvent;
  "og://lens-on": LensOnEvent;
  "og://freeze": FreezeEvent;
  "og://session": SessionEvent;
  "og://panel": PanelEvent;
  "og://settings": Settings;
  "og://visibility": VisibilityEvent;
}

/** Command name → [args, return type]. Args are camelCase (Tauri converts to snake_case). */
export interface CommandMap {
  frontend_ready: [Record<string, never>, InitState];
  get_settings: [Record<string, never>, Settings];
  save_settings: [{ settings: Settings }, { hotkeyErrors: Partial<Record<HotkeyAction, string>> }];
  set_api_key: [{ id: string; key: string }, void];
  has_api_key: [{ id: string }, boolean];
  delete_api_key: [{ id: string }, void];
  test_provider: [{ provider: Provider }, string];
  selection_done: [{ rect: Rect | null; viewW: number; viewH: number }, void];
  show_panel: [{ id: string; box: Rect; growUp: boolean; itemKey: string | null }, void];
  panel_set_height: [{ id: string; cssH: number }, void];
  ask_followup: [{ id: string; text: string }, void];
  dismiss_session: [{ id: string }, void];
  pin_session: [{ id: string; pinned: boolean }, void];
  open_settings: [Record<string, never>, void];
  get_history: [Record<string, never>, HistoryEntry[]];
}

/** Layout constants shared by lens geometry and the panel. */
export const PANEL_W = 420;
export const RAIL_W = 380;
export const RAIL_ENTRY_Y = 28;
export const CALLOUT_W = 360;
export const GAP = 56;
