// Settings entry: a normal, focusable, opaque window with our own title bar (DESIGN §3.7, §10.7).
import "../shared/base.css";
import "./settings.css";
import { createRoot } from "react-dom/client";
import { call, on } from "../shared/bridge";
import { initTheme, onThemes } from "../shared/themeLoader";
import { SettingsApp, useSaved } from "./Settings";

async function main() {
  // og://settings reaches the theme through the draft (Editor previews draft.theme/animSpeed).
  await Promise.all([on("og://settings", (saved) => useSaved.setState({ saved })), on("og://themes", onThemes)]);
  // Marks this window ready for events (DESIGN §3.8 rule 3); its InitState is not needed here.
  call("frontend_ready").catch(console.error);
  try {
    const saved = await call("get_settings");
    useSaved.setState((st) => (st.saved ? {} : { saved }));
  } catch (e) {
    useSaved.setState({ loadError: String(e) });
  }
  await initTheme(useSaved.getState().saved);
  createRoot(document.getElementById("root")!).render(<SettingsApp />);
}

void main();
