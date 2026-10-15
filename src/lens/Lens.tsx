// Lens root: one full-screen, transparent, click-through layer per monitor (DESIGN §3.1).
// Paint order = DOM order: session < frozen frame < Lens-On < HUD < toast.
import { Anchors } from "./Anchors";
import { Caption } from "./Caption";
import { FreezeSelect } from "./FreezeSelect";
import { Hud } from "./Hud";
import { LensOn } from "./LensOn";
import { LockOn } from "./LockOn";
import { RailLink } from "./RailLink";
import { Toast } from "./Toast";
import { useLens } from "./store";

function SessionView() {
  const s = useLens((st) => st.session);
  if (!s) return null;
  // Card draws nothing here: its thinking state and answer live in the panel.
  let view = null;
  if (s.kind === "scan") view = <Anchors s={s} />;
  else if (s.style === "lockon") view = <LockOn s={s} />;
  else if (s.style === "caption") view = <Caption s={s} />;
  else if (s.style === "rail") view = <RailLink s={s} />;
  // Keyed by session: a new session remounts, so its intro animations play again.
  return (
    <div key={s.id} className={s.closing ? "session closing" : "session"}>
      {view}
    </div>
  );
}

export function Lens() {
  const visible = useLens((s) => s.visible);
  const primary = useLens((s) => s.isPrimary);
  if (!visible) return null;
  return (
    <div className="lens">
      <SessionView />
      <FreezeSelect />
      {primary && <LensOn />}
      {primary && <Hud />}
      <Toast />
    </div>
  );
}
