// Four-corner HUD on the primary lens (DESIGN §2.5). Corners fade near the cursor, light up in
// sequence on every Lens-On (theme CSS staggers them by --i), and only the THINKING label loops.
import { useEffect, useRef, useState, type CSSProperties, type ReactNode, type RefObject } from "react";
import type { HudState } from "../shared/ipc";
import { bootDuration } from "../shared/themeLoader";
import { useLens, viewSize, workArea } from "./store";

type Pos = "tl" | "tr" | "bl" | "br";
const INSET = 14;
const AVOID_PX = 60;
/** --i of each corner (DESIGN §10.5). */
const ORDER: Record<Pos, number> = { tl: 0, tr: 1, bl: 2, br: 3 };

/** Rolls up from 0 once per mount (corners remount on every Lens-On), over the boot stage, so the
 * digits still move when the theme lights the corner; later updates jump straight to the value,
 * so 1 Hz telemetry never keeps a rAF loop running while idle. */
function useTween(target: number, decimals = 0, ms = bootDuration()): number {
  const [shown, setShown] = useState(0);
  const cur = useRef(0);
  const rolled = useRef(false);
  useEffect(() => {
    const f = 10 ** decimals;
    if (rolled.current) {
      cur.current = target;
      setShown(Math.round(target * f) / f);
      return;
    }
    rolled.current = true;
    const from = cur.current;
    const t0 = performance.now();
    let raf = 0;
    const step = (now: number) => {
      const k = Math.min(1, (now - t0) / ms);
      cur.current = from + (target - from) * (1 - (1 - k) ** 3);
      setShown(Math.round(cur.current * f) / f);
      if (k < 1) raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, [target, decimals, ms]);
  return shown;
}

/** True while the cursor is within AVOID_PX of the element's box. */
function useNear(ref: RefObject<HTMLElement | null>): boolean {
  const [near, setNear] = useState(false);
  useEffect(
    () =>
      useLens.subscribe((s, prev) => {
        if (s.cursor === prev.cursor) return;
        const c = s.cursor;
        const r = ref.current?.getBoundingClientRect();
        setNear(
          !!c && !!r && c.x > r.left - AVOID_PX && c.x < r.right + AVOID_PX && c.y > r.top - AVOID_PX && c.y < r.bottom + AVOID_PX,
        );
      }),
    [ref],
  );
  return near;
}

function Corner({ pos, lit, children }: { pos: Pos; lit: boolean; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const near = useNear(ref);
  const w = workArea();
  const v = viewSize();
  const top = pos[0] === "t";
  const left = pos[1] === "l";
  const style = {
    top: top ? w.y + INSET : undefined,
    bottom: top ? undefined : v.h - (w.y + w.h) + INSET,
    left: left ? w.x + INSET : undefined,
    right: left ? undefined : v.w - (w.x + w.w) + INSET,
    "--i": ORDER[pos],
  } as CSSProperties;
  return (
    <div ref={ref} className={`hud hud-${pos} og-hud-corner${near ? " near" : ""}${lit ? " lit" : ""}`} style={style}>
      {children}
    </div>
  );
}

function Status({ hud }: { hud: HudState | null }) {
  const mode = hud?.mode ?? "STANDBY";
  return (
    <>
      <div>◉ OPENGLASSES</div>
      <div>
        <span className={mode === "THINKING" ? "breathe" : undefined}>{mode}</span>
        {hud?.app && <span className="dim"> · {hud.app}</span>}
      </div>
    </>
  );
}

function System({ hud }: { hud: HudState | null }) {
  const cpu = useTween(hud?.cpu ?? 0);
  const ram = useTween(hud?.ramUsedGb ?? 0, 1);
  const gpu = useTween(hud?.gpu?.util ?? 0);
  const vram = useTween(hud?.gpu?.vramUsedGb ?? 0, 1);
  if (!hud) return <div>CPU --  RAM --</div>;
  return (
    <>
      <div>
        CPU {cpu}%  RAM {ram.toFixed(1)}/{Math.round(hud.ramTotalGb)}G
      </div>
      {hud.gpu && (
        <div>
          GPU {gpu}%  VRAM {vram.toFixed(1)}/{Math.round(hud.gpu.vramTotalGb)}G
        </div>
      )}
    </>
  );
}

const kilo = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)}K` : String(n));

function Model({ hud }: { hud: HudState | null }) {
  const ttft = useTween(hud?.ttftMs ?? 0);
  const tps = useTween(hud?.tps ?? 0, 1);
  const today = useTween(hud?.tokensToday ?? 0);
  if (!hud) return null;
  return (
    <>
      <div>
        {hud.provider} · {hud.model}
      </div>
      <div className="dim">
        TTFT {hud.ttftMs === null ? "--" : `${ttft}MS`} · {hud.tps === null ? "--" : tps.toFixed(1)} TOK/S · {kilo(today)} TODAY
      </div>
    </>
  );
}

export function Hud() {
  const settings = useLens((s) => s.settings);
  const hud = useLens((s) => s.hud);
  // Re-keying on each Lens-On remounts the corners: light-up animation + numbers roll from 0.
  const seq = useLens((s) => s.lensOnSeq);
  if (!settings) return null;
  const on = settings.hudCorners;
  const hk = settings.hotkeys;
  const lit = seq > 0;
  return (
    <>
      {on.tl && (
        <Corner key={`tl${seq}`} pos="tl" lit={lit}>
          <Status hud={hud} />
        </Corner>
      )}
      {on.tr && (
        <Corner key={`tr${seq}`} pos="tr" lit={lit}>
          <System hud={hud} />
        </Corner>
      )}
      {on.br && (
        <Corner key={`br${seq}`} pos="br" lit={lit}>
          <div className="dim">
            {`${hk.explain} EXPLAIN · ${hk.capture} CAPTURE · ${hk.scan} SCAN`.toUpperCase()}
          </div>
        </Corner>
      )}
      {on.bl && (
        <Corner key={`bl${seq}`} pos="bl" lit={lit}>
          <Model hud={hud} />
        </Corner>
      )}
    </>
  );
}
