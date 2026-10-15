// Conversation body: source quote, first answer (or scan list), then the follow-up thread.
import { useState } from "react";
import { call } from "../shared/bridge";
import type { Kind, PanelEvent } from "../shared/ipc";
import { Markdown } from "../shared/Markdown";
import { ScanList } from "./ScanList";
import type { MsgError, PanelSession } from "./store";

export const GLYPH: Record<Kind, string> = { text: "◈", image: "▣", scan: "◎" };

export function Conversation({ s, panel }: { s: PanelSession; panel: PanelEvent | null }) {
  const [srcOpen, setSrcOpen] = useState(false);
  const live = (i: number) => s.streaming && i === s.messages.length - 1;
  return (
    <>
      <div
        className={srcOpen ? "src open" : "src"}
        title={srcOpen ? "Collapse" : "Show the full source"}
        onClick={() => setSrcOpen(!srcOpen)}
      >
        {s.messages[0]?.text}
      </div>
      {s.kind === "scan" ? (
        <div className="ans">
          {s.items ? (
            <ScanList items={s.items} panel={panel} />
          ) : (
            live(1) && <span className="dim caret">SCANNING…</span>
          )}
          {s.errors[1] && <ErrLine err={s.errors[1]} />}
        </div>
      ) : (
        <Answer text={s.messages[1]?.text ?? ""} streaming={live(1)} err={s.errors[1]} />
      )}
      {s.messages.slice(2).map((m, j) =>
        m.role === "user" ? (
          <div key={j + 2} className="q">
            › {m.text}
          </div>
        ) : (
          <Answer key={j + 2} text={m.text} streaming={live(j + 2)} err={s.errors[j + 2]} />
        ),
      )}
    </>
  );
}

function Answer({ text, streaming, err }: { text: string; streaming: boolean; err: MsgError | undefined }) {
  return (
    <div className="ans">
      {text ? (
        <div className={streaming ? "live" : undefined}>
          <Markdown text={text} streaming={streaming} />
        </div>
      ) : (
        streaming && <span className="caret" />
      )}
      {err && <ErrLine err={err} />}
    </div>
  );
}

function ErrLine({ err }: { err: MsgError }) {
  const needsKey = err.code.startsWith("AUTH") || err.code.startsWith("NO API KEY");
  return (
    <div className="err warn">
      <span>⚠ {err.code}</span>
      {needsKey && (
        <button className="tb warn" onClick={() => call("open_settings").catch(console.error)}>
          OPEN SETTINGS
        </button>
      )}
      {err.message && <span className="err-msg">{err.message}</span>}
    </div>
  );
}
