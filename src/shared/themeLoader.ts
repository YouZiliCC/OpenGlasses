// Runtime theme packs, frontend side (DESIGN §10.4). Every window links the chosen theme's CSS
// after the app CSS, sets html[data-theme] and --og-speed, and re-applies on og://settings
// (theme/speed changed) and og://themes (files changed: cssUrl carries the r<rev> path segment).
import { create } from "zustand";
import { call } from "./bridge";
import { SPEED_FACTOR, type Settings, type ThemeInfo, type ThemesEvent } from "./ipc";

export type ThemeWant = Pick<Settings, "theme" | "animSpeed">;

interface ThemeState extends ThemesEvent {
  /** What this window wants: its settings, or the settings draft (live preview). */
  want: ThemeWant;
  /** The theme in effect; null = the wanted id is not installed (base.css only). */
  current: ThemeInfo | null;
}

export const useTheme = create<ThemeState>()(() => ({
  themes: [],
  rev: -1,
  want: { theme: "terminal", animSpeed: "normal" },
  current: null,
}));

let ready = false;
let loading: Promise<unknown> = Promise.resolve();

function apply() {
  if (!ready) return;
  const { themes, want } = useTheme.getState();
  const t = themes.find((x) => x.id === want.theme) ?? null;
  const root = document.documentElement;
  root.style.setProperty("--og-speed", String(SPEED_FACTOR[want.animSpeed]));
  let link = document.getElementById("og-theme") as HTMLLinkElement | null;
  if (t) {
    if (!link) {
      link = document.createElement("link");
      link.id = "og-theme";
      link.rel = "stylesheet";
      // Last in <head>: after the app CSS, so the theme wins at equal specificity.
      document.head.append(link);
    }
    if (link.getAttribute("href") !== t.cssUrl) {
      const el = link;
      loading = new Promise((done) => (el.onload = el.onerror = done));
      el.href = t.cssUrl;
    }
    root.dataset.theme = t.id;
  } else {
    link?.remove();
    delete root.dataset.theme;
  }
  useTheme.setState({ current: t });
}

/** Theme or speed changed: og://settings, or the settings draft (preview before Save). */
export function setTheme(want: ThemeWant) {
  useTheme.setState({ want: { theme: want.theme, animSpeed: want.animSpeed } });
  apply();
}

/** og://themes and the list_themes reply; an older revision never replaces a newer one. */
export function onThemes(ev: ThemesEvent) {
  if (ev.rev < useTheme.getState().rev) return;
  useTheme.setState({ themes: ev.themes, rev: ev.rev });
  apply();
}

/** Startup, before the first render: list the themes, link the chosen one and wait (≤ 1 s)
 * for its CSS, so the window never flashes the base look. */
export async function initTheme(want: ThemeWant) {
  useTheme.setState({ want: { theme: want.theme, animSpeed: want.animSpeed } });
  onThemes(await call("list_themes"));
  ready = true;
  apply();
  await themeSettled();
}

/** Resolves once the theme CSS being applied has loaded (or failed), at most 1 s. */
export function themeSettled(): Promise<void> {
  return Promise.race([loading, new Promise((done) => setTimeout(done, 1000))]).then(() => undefined);
}

/** Boot stage length for the current theme and speed, ms (DESIGN §10.5). */
export function bootDuration(): number {
  const { current, want } = useTheme.getState();
  return (current?.bootMs ?? 1000) * SPEED_FACTOR[want.animSpeed];
}
