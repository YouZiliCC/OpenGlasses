// Lens-On (DESIGN §2.4), ~400 ms. The blur is bilinear upscaling of tiny frames (1/32, 1/12, 1/4)
// with object-fit: fill; never CSS filter blur. frames null -> vignette + one scan-line sweep.
import { useLens, viewSize } from "./store";

export function LensOn() {
  const ev = useLens((s) => s.lensOn);
  const seq = useLens((s) => s.lensOnSeq);
  if (!ev) return null;
  const v = viewSize();
  return (
    <div key={seq} className="lens-on">
      {ev.frames?.slice(0, 3).map((url, i) => <img key={url} src={url} className={`lo-frame lo-f${i}`} alt="" />)}
      <div className={ev.frames ? "lo-fx" : "lo-fx bare"} />
      {!ev.frames && <div className="lo-sweep" />}
      <svg className="svg-layer">
        <path className="lo-border" d={`M6 6H${v.w - 6}V${v.h - 6}H6Z`} pathLength={1} />
      </svg>
    </div>
  );
}
