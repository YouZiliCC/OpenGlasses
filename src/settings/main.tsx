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
  const { settings } = await call("frontend_ready");
  // og://settings may race ahead of the reply; it is newer than the snapshot.
  useSaved.setState((st) => (st.saved ? {} : { saved: settings }));
  await initTheme(useSaved.getState().saved!);
  createRoot(document.getElementById("root")!).render(<SettingsApp />);
}

main().catch(console.error);
