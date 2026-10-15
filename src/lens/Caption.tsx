// Caption style (DESIGN §2.3 ③): a ≤2-line subtitle near the bottom; no marquee.
import { body, tldr } from "../shared/tldr";
import { GlassBg } from "./Glass";
import { captionBand } from "./geometry";
import { viewSize, workArea, type LensSession } from "./store";

/** Body as one plain run of text: Markdown blocks make no sense inside a 2-line subtitle. */
const flat = (md: string) => md.replace(/[#>*_`$|]+/g, "").replace(/\s+/g, " ").trim();

export function Caption({ s }: { s: LensSession }) {
  if (s.expanded) return null;
  const band = captionBand(workArea(), viewSize());
  const rest = flat(body(s.text));
  return (
    <div className="caption-band" style={{ left: band.x, top: band.y, width: band.w, height: band.h }}>
      <div className="caption" data-hover="caption">
        <GlassBg glass={s.glass} />
        <div className={s.streaming ? "caption-text caret" : "caption-text"}>
          <span className="glow">{tldr(s.text)}</span>
          {rest && ` ${rest}`}
          {s.error && <span className="warn"> ⚠ {s.error}</span>}
        </div>
      </div>
    </div>
  );
}
