// Rail mode: collapsible, read-only history of recent answers, newest first (DESIGN §2.3 ②).
import { useEffect, useState } from "react";
import { call } from "../shared/bridge";
import type { HistoryEntry } from "../shared/ipc";
import { Markdown } from "../shared/Markdown";
import { tldr } from "../shared/tldr";
import { GLYPH } from "./Conversation";

export function History() {
  // Fetched on mount: the panel remounts its content for every session.
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  useEffect(() => {
    call("get_history").then(setHistory, console.error);
  }, []);
  const [open, setOpen] = useState(false);
  const [expanded, setExpanded] = useState<string | null>(null);
  return (
    <div className="hist">
      <div className="hist-head" onClick={() => setOpen(!open)}>
        {open ? "▾" : "▸"} HISTORY <span className="dim">({history.length})</span>
      </div>
      {open && (
        <div className="hist-list">
          {history.length === 0 && <div className="dim">No answers yet.</div>}
          {history.map((h) => (
            <div key={h.id} className="hist-item">
              <div className="hist-tldr" onClick={() => setExpanded(expanded === h.id ? null : h.id)}>
                {GLYPH[h.kind]} {tldr(h.text) || "(empty)"}
              </div>
              {expanded === h.id && (
                <div className="hist-text">
                  <Markdown text={h.text} streaming={false} />
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
