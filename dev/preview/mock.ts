// Fake Tauri IPC for one preview window (DESIGN §10.8). Imported before the real window code, which
// then runs unmodified. Commands go to the harness page (the fake Rust core, harness.ts) by
// postMessage; events come back the same way and reach only this window's listeners, like
// getCurrentWebviewWindow().listen in the app.
// Mirrors what @tauri-apps/api 2.12.4 needs from window.__TAURI_INTERNALS__: invoke,
// transformCallback, metadata.currentWindow/currentWebview, and plugin:event|listen / unlisten
// with __TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener. (@tauri-apps/api/mocks is close, but
// its unlisten reads `args.id` instead of `eventId` and never removes the listener.)

const page = location.pathname.split("/").pop() ?? "";
/** lens.html -> "lens-1" (the primary monitor), panel.html -> "panel", settings.html -> "settings". */
const label = page.startsWith("lens") ? "lens-1" : page.replace(/\.html$/, "");
const host = window.parent;

type Callback = (data: unknown) => void;
const callbacks = new Map<number, Callback>();
/** Event name -> handler ids; the handler id doubles as the eventId, as in the mocks module. */
const listeners = new Map<string, Set<number>>();
const replies = new Map<number, { resolve: (v: unknown) => void; reject: (e: unknown) => void }>();
let nextId = 1;

function transformCallback(cb?: Callback, once = false): number {
  const id = nextId++;
  callbacks.set(id, (data) => {
    if (once) callbacks.delete(id);
    cb?.(data);
  });
  return id;
}

function unlisten(event: string, id: number) {
  listeners.get(event)?.delete(id);
  callbacks.delete(id);
}

async function invoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  if (cmd === "plugin:event|listen") {
    const { event, handler } = args as { event: string; handler: number };
    if (!listeners.has(event)) listeners.set(event, new Set());
    listeners.get(event)!.add(handler);
    return handler;
  }
  if (cmd === "plugin:event|unlisten") return unlisten(args.event as string, args.eventId as number);
  if (host === window) throw new Error(`no preview harness for ${cmd}: open /dev/preview/`);
  const id = nextId++;
  return new Promise((resolve, reject) => {
    replies.set(id, { resolve, reject });
    // The JSON round trip is what Tauri's serde IPC does (drops undefined, applies toJSON).
    host.postMessage({ og: "invoke", id, label, cmd, args: JSON.parse(JSON.stringify(args)) }, "*");
  });
}

addEventListener("message", (e: MessageEvent) => {
  const m = e.data;
  if (e.source !== host || !m?.og) return;
  if (m.og === "reply") {
    const r = replies.get(m.id);
    replies.delete(m.id);
    if (m.ok) r?.resolve(m.result);
    else r?.reject(m.error);
  } else if (m.og === "event") {
    for (const id of listeners.get(m.event) ?? []) callbacks.get(id)?.({ event: m.event, id, payload: m.payload });
  }
});

Object.assign(window, {
  __TAURI_INTERNALS__: {
    invoke,
    transformCallback,
    unregisterCallback: (id: number) => callbacks.delete(id),
    runCallback: (id: number, data: unknown) => callbacks.get(id)?.(data),
    callbacks,
    convertFileSrc: (path: string, protocol = "asset") => `http://${protocol}.localhost/${encodeURIComponent(path)}`,
    metadata: { currentWindow: { label }, currentWebview: { windowLabel: label, label } },
  },
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: unlisten },
});

// A (re)loaded window gets no events until its next frontend_ready, as in the app.
if (host !== window) host.postMessage({ og: "hello", label }, "*");
