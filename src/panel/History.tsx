// Rail mode: collapsible, read-only history of recent answers (DESIGN §2.3 ②).
import { useState } from "react";
import { Markdown } from "../shared/Markdown";
import { GLYPH } from "./Conversation";
import { usePanel } from "./store";

export function History() {
  const history = usePanel((s) => s.history);
  const [open, setOpen] = useState(false);
  const [expanded, setExpanded] = useState<string | null>(null);
  const list = [...history].sort((a, b) => b.createdAt - a.createdAt);
  return (
    <div className="hist">
      <div className="hist-head" onClick={() => setOpen(!open)}>
        {open ? "▾" : "▸"} HISTORY <span className="dim">({history.length})</span>
      </div>
      {open && (
        <div className="hist-list">
          {list.length === 0 && <div className="dim">No answers yet.</div>}
          {list.map((h) => (
            <div key={h.id} className="hist-item">
              <div className="hist-tldr" onClick={() => setExpanded(expanded === h.id ? null : h.id)}>
                {GLYPH[h.kind]} {h.tldr || "(empty)"}
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
