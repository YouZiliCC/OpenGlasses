// Panel state: the active session (og://session), the current panel request (og://panel),
// settings (for the glass mode) and the read-only history shown in Rail mode.
import { create } from "zustand";
import { call } from "../shared/bridge";
import type { HistoryEntry, InitState, Message, PanelEvent, Session, SessionEvent, Settings } from "../shared/ipc";

export interface MsgError {
  code: string;
  message: string;
}

export interface PanelSession extends Session {
  /** Errors by msgIndex. */
  errors: Record<number, MsgError>;
}

interface PanelState {
  settings: Settings | null;
  session: PanelSession | null;
  panel: PanelEvent | null;
  /** Fading out after `dismiss` (150 ms). */
  closing: boolean;
  history: HistoryEntry[];
}

export const usePanel = create<PanelState>()(() => ({
  settings: null,
  session: null,
  panel: null,
  closing: false,
  history: [],
}));

const set = usePanel.setState;
const get = usePanel.getState;
let fadeTimer = 0;

export function applyInit(init: InitState) {
  const a = init.active;
  set({
    settings: init.settings,
    panel: init.panel,
    // The snapshot only carries the last error code; attach it to the last message.
    session: a && { ...a, errors: a.error ? { [a.messages.length - 1]: { code: a.error, message: "" } } : {} },
  });
  refreshHistory();
}

export function refreshHistory() {
  call("get_history").then(
    (history) => set({ history }),
    (e) => console.error("get_history", e),
  );
}

/** Update the active session if `id` matches it. */
function patch(id: string, f: (s: PanelSession) => Partial<PanelSession>) {
  const s = get().session;
  if (s?.id === id) set({ session: { ...s, ...f(s) } });
}

export function onSession(e: SessionEvent) {
  switch (e.type) {
    case "start":
      // A running dismiss fade keeps going: the new session becomes visible only when it
      // requests the panel (onPanel), so the old panel never flashes the new content.
      set((st) => ({
        panel: st.panel?.id === e.id ? st.panel : null,
        session: {
          id: e.id,
          kind: e.kind,
          style: e.style,
          lens: e.lens,
          anchor: e.anchor,
          cursor: e.cursor,
          glass: e.glass,
          messages: [
            { role: "user", text: e.preview },
            { role: "assistant", text: "" },
          ],
          streaming: true,
          error: null,
          items: null,
          pinned: false,
          errors: {},
        },
      }));
      refreshHistory();
      break;
    case "user":
      patch(e.id, (s) => ({
        messages: [
          ...s.messages.slice(0, e.msgIndex),
          { role: "user", text: e.text },
          { role: "assistant", text: "" },
        ],
        streaming: true,
        error: null,
        // Rust pins the session on the first follow-up.
        pinned: s.pinned || e.msgIndex >= 2,
      }));
      break;
    case "delta":
      patch(e.id, (s) => {
        const messages: Message[] = s.messages.slice();
        while (messages.length <= e.msgIndex) messages.push({ role: "assistant", text: "" });
        messages[e.msgIndex] = { ...messages[e.msgIndex], text: messages[e.msgIndex].text + e.text };
        return { messages, streaming: true };
      });
      break;
    case "done":
      patch(e.id, () => ({ streaming: false }));
      break;
    case "error":
      patch(e.id, (s) => ({
        streaming: false,
        error: e.code,
        errors: { ...s.errors, [e.msgIndex]: { code: e.code, message: e.message } },
      }));
      break;
    case "scan":
      patch(e.id, () => ({ items: e.items, streaming: false }));
      break;
    case "dismiss": {
      const { session, panel } = get();
      if (session?.id !== e.id && panel?.id !== e.id) break;
      set({ closing: true });
      clearTimeout(fadeTimer);
      fadeTimer = window.setTimeout(() => {
        const st = get();
        set({
          closing: st.session?.id === e.id ? false : st.closing,
          session: st.session?.id === e.id ? null : st.session,
          panel: st.panel?.id === e.id ? null : st.panel,
        });
      }, 150);
      refreshHistory();
      break;
    }
    // "stale" only concerns the lens (brackets and leader lines).
  }
}

export function onPanel(e: PanelEvent) {
  set({ panel: e, closing: false });
}

export function onSettings(s: Settings) {
  set({ settings: s });
}

export function togglePin() {
  const s = get().session;
  if (!s) return;
  const pinned = !s.pinned;
  patch(s.id, () => ({ pinned }));
  call("pin_session", { id: s.id, pinned }).catch((e) => console.error("pin_session", e));
}
