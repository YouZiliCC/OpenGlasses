// Rail style (DESIGN §2.3 ②): the panel docks on the right; the lens only draws the leader line
// from the anchor to the rail's "current session" row. Hidden once the source window changed.
import { railLeader } from "./geometry";
import { Leader } from "./LockOn";
import type { LensSession } from "./store";

export function RailLink({ s }: { s: LensSession }) {
  if (s.stale || !s.rail) return null;
  return (
    <svg className="svg-layer">
      <Leader points={railLeader(s.anchor, s.rail)} delay={0} />
    </svg>
  );
}
