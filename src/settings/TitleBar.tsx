// Custom, themed title bar (DESIGN §10.7): the window has no native decorations. The bar and its
// title drag the window (double-click maximizes); the settings-only capability allows the rest.
import { getCurrentWindow } from "@tauri-apps/api/window";

export function TitleBar() {
  const w = getCurrentWindow();
  return (
    <header className="og-titlebar" data-tauri-drag-region>
      <span className="og-titlebar-title" data-tauri-drag-region>
        OpenGlasses Settings
      </span>
      <button className="og-titlebar-btn" title="Minimize" onClick={() => w.minimize().catch(console.error)}>
        ─
      </button>
      <button className="og-titlebar-btn og-titlebar-close" title="Close" onClick={() => w.close().catch(console.error)}>
        ✕
      </button>
    </header>
  );
}
