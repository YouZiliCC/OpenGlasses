// The panel window (DESIGN §2.3): Card / expanded Lock-on / Caption / scan list, or the Rail.
// The whole window is the panel: Rust sizes it from the content height we report.
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type RefObject } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { call } from "../shared/bridge";
import type { GlassMode, Message, PanelEvent } from "../shared/ipc";
import { tldr } from "../shared/tldr";
import { Conversation, GLYPH } from "./Conversation";
import { History } from "./History";
import { togglePin, usePanel, type PanelSession } from "./store";

/** Vertical border + padding of the panel box: the theme decides them (--og-panel-border). */
function frame(el: HTMLElement | null): number {
  if (!el) return 2;
  const cs = getComputedStyle(el);
  return [cs.borderTopWidth, cs.borderBottomWidth, cs.paddingTop, cs.paddingBottom].reduce((n, v) => n + parseFloat(v), 0);
}

export function Panel() {
  const session = usePanel((s) => s.session);
  return session ? <PanelView key={session.id} s={session} /> : null;
}

function PanelView({ s }: { s: PanelSession }) {
  const panel = usePanel((st) => st.panel);
  const closing = usePanel((st) => st.closing);
  const glassMode = usePanel((st) => st.settings?.glass ?? "snapshot");
  const winH = useWindowHeight();
  const root = useRef<HTMLDivElement>(null);
  const head = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);
  const foot = useRef<HTMLDivElement>(null);
  useReportHeight(panel, root, [head, inner, foot]);
  useStickToBottom(body, s.messages);

  // CRT flicker each time the panel is (re)requested, i.e. when it appears.
  useLayoutEffect(() => {
    const el = root.current;
    if (!panel || !el) return;
    el.classList.remove("crt-on");
    void el.offsetWidth; // restart the animation
    el.classList.add("crt-on");
  }, [panel]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.isComposing) return;
      e.preventDefault();
      call("dismiss_session", { id: s.id }).catch(console.error);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [s.id]);

  const rail = panel?.mode === "rail";
  const style: CSSProperties = { width: panel ? panel.box.w : "100vw", ...background(s, panel, glassMode, winH) };
  return (
    <div ref={root} className={`panel${closing ? " closing" : ""}`} style={style}>
      <div className="ph" ref={head}>
        <Header s={s} rail={rail} />
      </div>
      <div className="pb" ref={body}>
        <div className="pb-inner" ref={inner}>
          <Conversation s={s} panel={panel} />
        </div>
      </div>
      <div className="pf" ref={foot}>
        <Composer id={s.id} streaming={s.streaming} />
      </div>
      {rail && <History />}
    </div>
  );
}

/** Snapshot glass aligned to screen coordinates; acrylic lets the native effect through. */
function background(s: PanelSession, panel: PanelEvent | null, mode: GlassMode, winH: number): CSSProperties {
  // Longhands only, so React never mixes `background` with its sub-properties.
  if (mode === "acrylic") return { backgroundColor: "color-mix(in srgb, var(--og-glass-tint) 55%, transparent)" };
  const g = s.glass;
  if (!g || !panel) return { backgroundColor: "var(--og-panel-bg)", backgroundImage: "var(--og-scanlines)" };
  const { x, y, h } = panel.box;
  const top = panel.growUp ? y + h - winH : y;
  return {
    backgroundImage: `var(--og-scanlines), linear-gradient(var(--og-glass-tint), var(--og-glass-tint)), url("${g.url}")`,
    backgroundSize: `auto, auto, ${g.w}px ${g.h}px`,
    backgroundPosition: `0 0, 0 0, ${-x}px ${-top}px`,
    backgroundRepeat: "repeat, no-repeat, no-repeat",
  };
}

function useWindowHeight(): number {
  const [h, setH] = useState(window.innerHeight);
  useEffect(() => {
    const f = () => setH(window.innerHeight);
    window.addEventListener("resize", f);
    return () => window.removeEventListener("resize", f);
  }, []);
  return h;
}

/**
 * Report the natural content height for the current panel request: immediately on every new
 * PanelEvent (Rust shows the panel only after that), then on resize, throttled to ~100 ms.
 * Rail always reports box.h. Rust caps the height at box.h; longer content scrolls in `.pb`.
 */
function useReportHeight(
  panel: PanelEvent | null,
  root: RefObject<HTMLElement | null>,
  parts: RefObject<HTMLElement | null>[],
) {
  const last = useRef({ id: "", h: 0, at: 0 });
  useLayoutEffect(() => {
    if (!panel) return;
    const rail = panel.mode === "rail";
    const send = (force: boolean) => {
      const cssH = rail
        ? panel.box.h
        : Math.ceil(parts.reduce((sum, r) => sum + (r.current?.getBoundingClientRect().height ?? 0), frame(root.current)));
      if (!force && last.current.id === panel.id && last.current.h === cssH) return;
      last.current = { id: panel.id, h: cssH, at: performance.now() };
      call("panel_set_height", { id: panel.id, cssH }).catch(console.error);
    };
    send(true);
    if (rail) return;
    let timer = 0;
    const ro = new ResizeObserver(() => {
      if (timer) return;
      const wait = Math.max(0, last.current.at + 100 - performance.now());
      timer = window.setTimeout(() => {
        timer = 0;
        send(false);
      }, wait);
    });
    for (const r of parts) if (r.current) ro.observe(r.current);
    return () => {
      ro.disconnect();
      clearTimeout(timer);
    };
    // `root` and `parts` are stable refs.
  }, [panel]);
}

/** Jump to a newly sent follow-up and keep following its stream while the user stays near the bottom. */
function useStickToBottom(ref: RefObject<HTMLDivElement | null>, messages: Message[]) {
  const len = useRef(messages.length);
  useLayoutEffect(() => {
    const el = ref.current;
    const grew = messages.length > len.current;
    len.current = messages.length;
    if (!el || messages.length <= 2) return;
    if (grew || el.scrollHeight - el.scrollTop - el.clientHeight < 48) el.scrollTop = el.scrollHeight;
  }, [ref, messages]);
}

function headline(s: PanelSession): { text: string; warn: boolean } {
  if (s.kind === "scan" && s.items) {
    const n = s.items.length;
    return { text: n ? `${n} POINT${n > 1 ? "S" : ""} FOUND` : "NO POINTS FOUND", warn: false };
  }
  const t = tldr(s.messages[1]?.text ?? "");
  if (t) return { text: t, warn: false };
  const err = s.errors[1];
  if (err) return { text: `⚠ ${err.code}`, warn: true };
  return { text: s.streaming ? "THINKING…" : "—", warn: false };
}

/** Latest non-empty answer; for a scan without follow-ups, the item list. */
function copyText(s: PanelSession): string {
  for (let i = s.messages.length - 1; i > 0; i--) {
    const m = s.messages[i];
    if (m.role === "assistant" && m.text.trim()) return m.text;
  }
  return (s.items ?? []).map((it) => `${it.label} — ${it.brief}\n\n${it.detail}`).join("\n\n");
}

function Header({ s, rail }: { s: PanelSession; rail: boolean }) {
  const [copied, setCopied] = useState(false);
  const title = headline(s);
  const copy = () => {
    const text = copyText(s);
    if (!text) return;
    writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    }, console.error);
  };
  return (
    <>
      <span className="glyph glow">{GLYPH[s.kind]}</span>
      <span className={title.warn ? "title warn" : "title glow"}>{title.text}</span>
      <button className="tb copy" title="Copy answer" onClick={copy}>
        {copied ? "COPIED" : "COPY"}
      </button>
      {/* Rail sessions are never closed by clicking outside, so pinning means nothing there. */}
      {!rail && (
        <button className={s.pinned ? "tb pin on" : "tb pin"} title={s.pinned ? "Unpin" : "Pin"} onClick={togglePin}>
          📌
        </button>
      )}
      <button className="tb" title="Close (Esc)" onClick={() => call("dismiss_session", { id: s.id }).catch(console.error)}>
        ×
      </button>
    </>
  );
}

function Composer({ id, streaming }: { id: string; streaming: boolean }) {
  const [text, setText] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const send = () => {
    const t = text.trim();
    if (!t || streaming) return;
    setText("");
    setErr(null);
    call("ask_followup", { id, text: t }).catch((e) => {
      setText(t);
      setErr(String(e));
    });
  };
  return (
    <>
      {err && <div className="warn small">⚠ {err}</div>}
      <div className="composer">
        <span className={streaming ? "prompt dim" : "prompt glow"}>›</span>
        <textarea
          rows={1}
          value={text}
          spellCheck={false}
          placeholder={streaming ? "streaming…" : "Follow up · ⏎ send · ⇧⏎ newline"}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              send();
            }
          }}
        />
      </div>
    </>
  );
}
