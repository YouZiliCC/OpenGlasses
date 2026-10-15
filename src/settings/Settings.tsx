// Settings window (DESIGN §3.7): edits a draft; Save applies it. API keys never come back.
import { useEffect, useState, type Dispatch, type ReactNode, type SetStateAction } from "react";
import { create } from "zustand";
import { call } from "../shared/bridge";
import type { BboxFormat, GlassMode, HotkeyAction, HudCorners, Provider, Settings, Style } from "../shared/ipc";

/** Last settings known to be persisted (get_settings, og://settings or our own save). */
export const useSaved = create<{ saved: Settings | null; loadError: string | null }>()(() => ({
  saved: null,
  loadError: null,
}));

type Tab = "providers" | "hotkeys" | "appearance" | "behavior";
const TABS: Tab[] = ["providers", "hotkeys", "appearance", "behavior"];
type HotkeyErrors = Partial<Record<HotkeyAction, string>>;
type SetDraft = Dispatch<SetStateAction<Settings>>;

const HOTKEYS: [HotkeyAction, string][] = [
  ["explain", "Explain selection"],
  ["capture", "Capture region"],
  ["scan", "Scan (experimental)"],
  ["toggle", "Glasses on/off"],
];

const STYLES: [Style, string, string][] = [
  ["lockon", "Lock-on", "brackets and a leader line to a floating callout; hover to expand"],
  ["rail", "Lens Rail", "sidebar docked to the right edge, with read-only history"],
  ["caption", "Caption", "two-line subtitle near the bottom of the screen; hover to expand"],
  ["card", "Card", "classic card with the full answer next to the selection"],
];

const CORNERS: [keyof HudCorners, string][] = [
  ["tl", "Top-left: mode, app"],
  ["tr", "Top-right: CPU, RAM, GPU"],
  ["bl", "Bottom-left: model, speed, tokens"],
  ["br", "Bottom-right: hotkey hints"],
];

const GLASS: [GlassMode, string, string][] = [
  ["snapshot", "Snapshot", "blurred, tinted screenshot behind callouts and panels"],
  ["acrylic", "Acrylic (experimental)", "native Windows acrylic on the panel; may turn solid while inactive"],
];

export function SettingsApp() {
  const { saved, loadError } = useSaved();
  if (!saved) return <div className="set-loading">{loadError ? <span className="warn">⚠ {loadError}</span> : "LOADING…"}</div>;
  return <Editor saved={saved} />;
}

function Editor({ saved }: { saved: Settings }) {
  const [tab, setTab] = useState<Tab>("providers");
  const [draft, setDraft] = useState(saved);
  // extraBody is edited as JSON text per slot and parsed on build.
  const [extra, setExtra] = useState(() => extraTexts(saved));
  const [base, setBase] = useState(saved);
  const [hotkeyErrors, setHotkeyErrors] = useState<HotkeyErrors>({});
  const [status, setStatus] = useState<{ text: string; warn: boolean } | null>(null);
  const [busy, setBusy] = useState(false);

  const { settings: final, problem } = build(draft, extra);
  const dirty = !final || JSON.stringify(final) !== JSON.stringify(base);
  const reset = (s: Settings) => {
    setDraft(s);
    setExtra(extraTexts(s));
  };

  // Settings changed underneath us (og://settings or our own save): adopt them unless there are edits.
  if (saved !== base) {
    setBase(saved);
    if (!dirty) reset(saved);
  }

  const save = async () => {
    if (!final) return;
    setBusy(true);
    try {
      const r = await call("save_settings", { settings: final });
      setHotkeyErrors(r.hotkeyErrors);
      useSaved.setState({ saved: final });
      const n = Object.keys(r.hotkeyErrors).length;
      setStatus(
        n ? { text: `Saved, but ${n} hotkey${n > 1 ? "s" : ""} failed to register`, warn: true } : { text: "Saved ✓", warn: false },
      );
    } catch (e) {
      setStatus({ text: `⚠ ${String(e)}`, warn: true });
    } finally {
      setBusy(false);
    }
  };

  const shown = problem
    ? { text: `⚠ ${problem}`, warn: true }
    : status?.warn
      ? status
      : dirty
        ? { text: "Unsaved changes", warn: false }
        : status;

  return (
    <div className="set">
      <nav className="set-tabs">
        <span className="set-brand glow">◉ OPENGLASSES</span>
        {TABS.map((t) => (
          <button key={t} className={t === tab ? "tab on" : "tab"} onClick={() => setTab(t)}>
            {t.toUpperCase()}
          </button>
        ))}
      </nav>
      <main className="set-main">
        {tab === "providers" && <Providers draft={draft} setDraft={setDraft} extra={extra} setExtra={setExtra} />}
        {tab === "hotkeys" && <Hotkeys draft={draft} setDraft={setDraft} errors={hotkeyErrors} />}
        {tab === "appearance" && <Appearance draft={draft} setDraft={setDraft} />}
        {tab === "behavior" && <Behavior draft={draft} setDraft={setDraft} />}
      </main>
      <footer className="set-bar">
        <span className={`set-status ${shown?.warn ? "warn" : "dim"}`} title={shown?.text}>
          {shown?.text}
        </span>
        <button
          className="btn"
          disabled={!dirty || busy}
          onClick={() => {
            reset(base);
            setStatus(null);
          }}
        >
          REVERT
        </button>
        <button className="btn primary" disabled={!final || !dirty || busy} onClick={save}>
          {busy ? "SAVING…" : "SAVE"}
        </button>
      </footer>
    </div>
  );
}

// ------------------------------------------------------------------ draft -> Settings

function extraTexts(s: Settings): Record<string, string> {
  return Object.fromEntries(s.providers.map((p) => [p.id, p.extraBody ? JSON.stringify(p.extraBody, null, 2) : ""]));
}

/** Empty -> null; otherwise must be a JSON object. */
function parseExtra(text: string): { value: Provider["extraBody"]; error: string | null } {
  if (!text.trim()) return { value: null, error: null };
  try {
    const v: unknown = JSON.parse(text);
    if (v && typeof v === "object" && !Array.isArray(v)) return { value: v as Record<string, unknown>, error: null };
    return { value: null, error: "must be a JSON object" };
  } catch (e) {
    return { value: null, error: e instanceof Error ? e.message : String(e) };
  }
}

/** The provider as it would be saved (also what Test sends). */
function finalProvider(p: Provider, extraText: string): { provider: Provider | null; error: string | null } {
  const x = parseExtra(extraText);
  if (x.error) return { provider: null, error: x.error };
  return {
    provider: {
      ...p,
      baseUrl: p.baseUrl.trim(),
      model: p.model.trim(),
      visionModel: p.visionModel?.trim() || null,
      extraBody: x.value,
    },
    error: null,
  };
}

function build(d: Settings, extra: Record<string, string>): { settings: Settings | null; problem: string | null } {
  const providers: Provider[] = [];
  for (const p of d.providers) {
    const r = finalProvider(p, extra[p.id] ?? "");
    if (!r.provider) return { settings: null, problem: `${p.name}: invalid extra body (${r.error})` };
    providers.push(r.provider);
  }
  if (!Number.isInteger(d.maxTokens) || d.maxTokens < 1) {
    return { settings: null, problem: "Max tokens must be a positive whole number" };
  }
  const hotkeys = Object.fromEntries(Object.entries(d.hotkeys).map(([k, v]) => [k, v.trim()])) as Settings["hotkeys"];
  return {
    settings: {
      ...d,
      providers,
      hotkeys,
      proxy: d.proxy?.trim() || null,
      answerLanguage: d.answerLanguage.trim() || "auto",
    },
    problem: null,
  };
}

// ------------------------------------------------------------------ tabs

function Providers({
  draft,
  setDraft,
  extra,
  setExtra,
}: {
  draft: Settings;
  setDraft: SetDraft;
  extra: Record<string, string>;
  setExtra: Dispatch<SetStateAction<Record<string, string>>>;
}) {
  const p = draft.providers.find((x) => x.id === draft.activeProviderId) ?? draft.providers[0];
  if (!p) return <div className="dim">No provider slots.</div>;
  const upd = (patch: Partial<Provider>) =>
    setDraft((d) => ({ ...d, providers: d.providers.map((x) => (x.id === p.id ? { ...x, ...patch } : x)) }));
  const text = extra[p.id] ?? "";
  const { provider, error } = finalProvider(p, text);
  const deepseek = /deepseek/i.test(p.id + p.name);
  return (
    <>
      <Field label="Active provider" hint={deepseek && <span className="warn">text-only: screenshots need a vision model</span>}>
        <select value={p.id} onChange={(e) => setDraft((d) => ({ ...d, activeProviderId: e.target.value }))}>
          {draft.providers.map((x) => (
            <option key={x.id} value={x.id}>
              {x.name}
            </option>
          ))}
        </select>
      </Field>
      <Field label="Name" hint={p.kind === "anthropic" ? "Anthropic Messages API" : "OpenAI-compatible Chat Completions"}>
        <input type="text" value={p.name} readOnly />
      </Field>
      <Field label="Base URL">
        <input type="text" value={p.baseUrl} spellCheck={false} onChange={(e) => upd({ baseUrl: e.target.value })} />
      </Field>
      <Field label="Model">
        <input type="text" value={p.model} spellCheck={false} onChange={(e) => upd({ model: e.target.value })} />
      </Field>
      <Field label="Vision model" hint="Used for screenshots and scans; empty = same as Model">
        <input
          type="text"
          value={p.visionModel ?? ""}
          placeholder="(same as Model)"
          spellCheck={false}
          onChange={(e) => upd({ visionModel: e.target.value || null })}
        />
      </Field>
      <Field label="Box format" hint="Scan box coordinates: pixel (Claude, GPT) or 0–1000 normalized (Qwen-VL)">
        <select value={p.bboxFormat} onChange={(e) => upd({ bboxFormat: e.target.value as BboxFormat })}>
          <option value="pixel">pixel</option>
          <option value="norm1000">norm1000</option>
        </select>
      </Field>
      <Field
        label="Extra body"
        hint={
          error ? (
            <span className="warn">⚠ Invalid JSON: {error}</span>
          ) : (
            "JSON Merge Patch applied to every request; null deletes a field; empty = none"
          )
        }
      >
        <textarea
          rows={4}
          value={text}
          spellCheck={false}
          placeholder="{}"
          onChange={(e) => setExtra((m) => ({ ...m, [p.id]: e.target.value }))}
        />
      </Field>
      <KeyField key={`key:${p.id}`} id={p.id} />
      <TestField key={`test:${p.id}`} provider={provider} />
    </>
  );
}

/** Remounted per slot (keyed), so its state always belongs to `id`. */
function KeyField({ id }: { id: string }) {
  const [has, setHas] = useState<boolean | null>(null);
  const [key, setKey] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const refresh = () => call("has_api_key", { id }).then(setHas, (e) => setErr(String(e)));
  useEffect(() => {
    void refresh();
  }, []);
  const run = (p: Promise<void>) => {
    setErr(null);
    p.then(() => {
      setKey("");
      return refresh();
    }).catch((e) => setErr(String(e)));
  };
  const saveKey = () => {
    if (key.trim()) run(call("set_api_key", { id, key: key.trim() }));
  };
  return (
    <Field
      label="API key"
      hint={err ? <span className="warn">⚠ {err}</span> : "Stored in Windows Credential Manager; it is never shown again"}
    >
      <div className="inline">
        <input
          type="password"
          value={key}
          autoComplete="off"
          placeholder={has ? "•••••••• (replace)" : "paste key"}
          onChange={(e) => setKey(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") saveKey();
          }}
        />
        <button className="btn" disabled={!key.trim()} onClick={saveKey}>
          SAVE KEY
        </button>
        <button className="btn" disabled={!has} onClick={() => run(call("delete_api_key", { id }))}>
          CLEAR
        </button>
        <span className={has ? "glow" : "dim"}>{has === null ? "" : has ? "Saved ✓" : "No key"}</span>
      </div>
    </Field>
  );
}

function TestField({ provider }: { provider: Provider | null }) {
  const [res, setRes] = useState<{ state: "busy" | "ok" | "err"; text: string } | null>(null);
  const test = () => {
    if (!provider) return;
    setRes({ state: "busy", text: "Testing…" });
    call("test_provider", { provider }).then(
      (r) => setRes({ state: "ok", text: `✓ ${r || "OK"}` }),
      (e) => setRes({ state: "err", text: `⚠ ${String(e)}` }),
    );
  };
  return (
    <Field label="" hint="Tests this draft with the saved key; no need to Save first">
      <div className="inline">
        <button className="btn" disabled={!provider || res?.state === "busy"} onClick={test}>
          TEST
        </button>
        {res && <span className={res.state === "err" ? "warn" : res.state === "ok" ? "glow" : "dim"}>{res.text}</span>}
      </div>
    </Field>
  );
}

function Hotkeys({ draft, setDraft, errors }: { draft: Settings; setDraft: SetDraft; errors: HotkeyErrors }) {
  return (
    <>
      <p className="note dim">
        Accelerator text such as Alt+Q or Ctrl+Shift+F1. Prefer Alt+key for triggers: Ctrl+Alt+key is AltGr on German,
        Polish and similar layouts. Applied on Save.
      </p>
      {HOTKEYS.map(([a, label]) => (
        <Field key={a} label={label} hint={errors[a] && <span className="warn">⚠ {errors[a]}</span>}>
          <input
            type="text"
            value={draft.hotkeys[a] ?? ""}
            spellCheck={false}
            onChange={(e) => setDraft((d) => ({ ...d, hotkeys: { ...d.hotkeys, [a]: e.target.value } }))}
          />
        </Field>
      ))}
    </>
  );
}

function Appearance({ draft, setDraft }: { draft: Settings; setDraft: SetDraft }) {
  return (
    <>
      <Section title="Default style · applies from the next answer">
        {STYLES.map(([v, name, desc]) => (
          <Choice key={v} name="style" checked={draft.style === v} onPick={() => setDraft((d) => ({ ...d, style: v }))}>
            <span className="glow">{name}</span> <span className="dim">— {desc}</span>
          </Choice>
        ))}
      </Section>
      <Section title="HUD corners">
        <div className="grid2">
          {CORNERS.map(([k, label]) => (
            <Check
              key={k}
              checked={draft.hudCorners[k]}
              onChange={(v) => setDraft((d) => ({ ...d, hudCorners: { ...d.hudCorners, [k]: v } }))}
            >
              {label}
            </Check>
          ))}
        </div>
      </Section>
      <Section title="Lens-On">
        <Check checked={draft.lensOn} onChange={(v) => setDraft((d) => ({ ...d, lensOn: v }))}>
          Play the Lens-On animation at startup and when putting the glasses on
        </Check>
      </Section>
      <Section title="Glass">
        {GLASS.map(([v, name, desc]) => (
          <Choice key={v} name="glass" checked={draft.glass === v} onPick={() => setDraft((d) => ({ ...d, glass: v }))}>
            <span className="glow">{name}</span> <span className="dim">— {desc}</span>
          </Choice>
        ))}
      </Section>
    </>
  );
}

function Behavior({ draft, setDraft }: { draft: Settings; setDraft: SetDraft }) {
  return (
    <>
      <Field label="Answer language" hint='"auto" follows the source text; or a language such as zh-CN or English'>
        <input
          type="text"
          value={draft.answerLanguage}
          spellCheck={false}
          onChange={(e) => setDraft((d) => ({ ...d, answerLanguage: e.target.value }))}
        />
      </Field>
      <Field label="Context">
        <Check checked={draft.includeWindowTitle} onChange={(v) => setDraft((d) => ({ ...d, includeWindowTitle: v }))}>
          Send the window title along with the selection
        </Check>
      </Field>
      <Field label="Proxy" hint="e.g. http://127.0.0.1:7890; empty = none">
        <input
          type="text"
          value={draft.proxy ?? ""}
          placeholder="none"
          spellCheck={false}
          onChange={(e) => setDraft((d) => ({ ...d, proxy: e.target.value || null }))}
        />
      </Field>
      <Field label="Max tokens">
        <input
          type="number"
          min={1}
          step={1}
          value={draft.maxTokens || ""}
          onChange={(e) => setDraft((d) => ({ ...d, maxTokens: Number(e.target.value) || 0 }))}
        />
      </Field>
      <Field label="Screen capture" hint="Turn off to record demos of the overlay">
        <Check checked={draft.excludeFromCapture} onChange={(v) => setDraft((d) => ({ ...d, excludeFromCapture: v }))}>
          Exclude overlays from screenshots, recordings and screen sharing
        </Check>
      </Field>
    </>
  );
}

// ------------------------------------------------------------------ small pieces

function Field({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="field">
      <div className="lbl">{label}</div>
      <div>
        {children}
        {hint && <div className="hint">{hint}</div>}
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="sec">
      <h3>{title}</h3>
      {children}
    </section>
  );
}

function Check({ checked, onChange, children }: { checked: boolean; onChange: (v: boolean) => void; children: ReactNode }) {
  return (
    <label className="opt">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>{children}</span>
    </label>
  );
}

function Choice({ name, checked, onPick, children }: { name: string; checked: boolean; onPick: () => void; children: ReactNode }) {
  return (
    <label className="opt">
      <input type="radio" name={name} checked={checked} onChange={onPick} />
      <span>{children}</span>
    </label>
  );
}
