// Lens-On boot stage (DESIGN §10.5): a fixed DOM that the theme CSS animates; lens.css has the
// default (f0 fades out). The blur is bilinear upscaling of tiny frames (1/32, 1/12, 1/4, most
// blurred first), never CSS filter blur. No frames -> .og-boot-noframes and the vars are `none`.
import { useEffect, type CSSProperties } from "react";
import { bootDuration } from "../shared/themeLoader";
import { endLensOn, useLens } from "./store";

export function LensOn() {
  const ev = useLens((s) => s.lensOn);
  const seq = useLens((s) => s.lensOnSeq);
  // Unmount bootMs × speed after mounting; the theme has made the stage fully transparent by
  // then, and the margin covers the first paint.
  useEffect(() => {
    if (!ev) return;
    const t = setTimeout(() => endLensOn(seq), bootDuration() + 150);
    return () => clearTimeout(t);
  }, [ev, seq]);
  if (!ev) return null;
  const f = ev.frames;
  const vars = Object.fromEntries(
    [0, 1, 2].map((i) => [`--og-frame-${i}`, f ? `url("${f[i]}")` : "none"]),
  ) as CSSProperties;
  return (
    <div key={seq} className={f ? "og-boot" : "og-boot og-boot-noframes"} style={vars}>
      <div className="og-boot-frame og-boot-f0" />
      <div className="og-boot-frame og-boot-f1" />
      <div className="og-boot-frame og-boot-f2" />
      <div className="og-boot-fx og-boot-fx1" />
      <div className="og-boot-fx og-boot-fx2" />
      <div className="og-boot-fx og-boot-fx3" />
    </div>
  );
}
