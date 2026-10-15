// Settings entry: a normal, focusable, opaque window (DESIGN §3.7).
import "../shared/theme.css";
import "./settings.css";
import { createRoot } from "react-dom/client";
import { call, on } from "../shared/bridge";
import { SettingsApp, useSaved } from "./Settings";

async function main() {
  await on("og://settings", (saved) => useSaved.setState({ saved }));
  // Marks this window ready for events (DESIGN §3.8 rule 3); its InitState is not needed here.
  call("frontend_ready").catch(console.error);
  try {
    const saved = await call("get_settings");
    useSaved.setState((st) => (st.saved ? {} : { saved }));
  } catch (e) {
    useSaved.setState({ loadError: String(e) });
  }
  createRoot(document.getElementById("root")!).render(<SettingsApp />);
}

void main();
