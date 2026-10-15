// Canned data for the preview harness: what the Rust core would know about the fake desktop
// (assets/desktop.png: 1600×900 CSS px at 100% scale, 48 px taskbar) plus sample answers.
import type { HistoryEntry, HudState, Provider, Rect, ScanItem, Settings } from "../../src/shared/ipc";

export const DESKTOP_URL = new URL("./assets/desktop.png", import.meta.url).href;
export const SCREEN = { w: 1600, h: 900 };
export const WORK_AREA: Rect = { x: 0, y: 0, w: 1600, h: 852 };

/** Boxes on desktop.png, printed by `shoot.mjs --scenario desktop`. */
export const AT = {
  ide: { x: 12, y: 12, w: 786, h: 510 },
  line7: { x: 380, y: 224, w: 57, h: 16 },
  line12: { x: 348, y: 324, w: 398, h: 16 },
  line14: { x: 348, y: 364, w: 309, h: 16 },
  selection: { x: 1312, y: 351, w: 112, h: 16 },
  error: { x: 23, y: 604, w: 337, h: 15 },
} satisfies Record<string, Rect>;

/** The drag shoot.mjs makes in the freeze scenario: the compiler error block in the terminal. */
export const SELECT_RECT: Rect = { x: 16, y: 598, w: 480, h: 116 };

const slot = (
  id: string,
  name: string,
  kind: Provider["kind"],
  baseUrl: string,
  model: string,
  extraBody: Provider["extraBody"],
  visionModel: string | null = null,
  bboxFormat: Provider["bboxFormat"] = "pixel",
): Provider => ({ id, name, kind, baseUrl, model, visionModel, extraBody, bboxFormat });

/** Same as settings.rs defaults(). */
export function defaultSettings(): Settings {
  const effort = { output_config: { effort: "low" } };
  return {
    providers: [
      slot("openai", "OpenAI", "openai", "https://api.openai.com/v1", "gpt-6-luna", {
        max_tokens: null,
        max_completion_tokens: 2000,
        reasoning_effort: "none",
      }),
      slot("anthropic", "Anthropic", "anthropic", "https://api.anthropic.com", "claude-opus-5-5", effort),
      slot("deepseek", "DeepSeek", "openai", "https://api.deepseek.com/v1", "deepseek-chat", {}),
      slot(
        "qwen",
        "Qwen (DashScope)",
        "openai",
        "https://dashscope.aliyuncs.com/compatible-mode/v1",
        "qwen-plus",
        { enable_thinking: false },
        "qwen3-vl-plus",
        "norm1000",
      ),
      slot("openrouter", "OpenRouter", "openai", "https://openrouter.ai/api/v1", "anthropic/claude-haiku-5-5", {}),
      slot("ollama", "Ollama (local)", "openai", "http://localhost:11434/v1", "qwen3-vl:8b", {}, null, "norm1000"),
      slot("custom-openai", "Custom (OpenAI)", "openai", "", "", null),
      slot("custom-anthropic", "Custom (Anthropic)", "anthropic", "https://api.anthropic.com", "claude-opus-5-5", effort),
    ],
    activeProviderId: "anthropic",
    hotkeys: { explain: "Alt+Q", capture: "Alt+W", scan: "Alt+E", toggle: "Ctrl+Alt+G" },
    style: "lockon",
    hudCorners: { tl: true, tr: true, bl: true, br: true },
    lensOn: true,
    glass: "snapshot",
    answerLanguage: "auto",
    includeWindowTitle: true,
    proxy: null,
    maxTokens: 2000,
    excludeFromCapture: true,
    theme: "terminal",
    animSpeed: "normal",
  };
}

/** Slots that "have" a key in the fake credential store. */
export const KEYS = ["anthropic", "openai", "deepseek"];

export const HUD: HudState = {
  mode: "STANDBY",
  app: "msedge.exe",
  cpu: 21,
  ramUsedGb: 12.4,
  ramTotalGb: 32,
  gpu: { name: "NVIDIA GeForce RTX 4070", util: 9, vramUsedGb: 3.2, vramTotalGb: 12 },
  provider: "Anthropic",
  model: "claude-opus-5-5",
  ttftMs: null,
  tps: null,
  tokensToday: 18432,
};

/** Alt+Q on the selected "borrow checker" in the browser. */
export const SOURCE_TEXT = "borrow checker";

export const TEXT_ANSWER = `借用检查器（borrow checker）是 Rust 编译期的一组规则：任何引用都不能比它指向的数据活得更久。

编译器为每个引用推导一个**生命周期** $'a$，然后检查两条规则：

1. 同一时刻，要么有任意多个共享引用 \`&T\`，要么只有一个可变引用 \`&mut T\`；
2. 若 \`r: &'a T\` 指向 \`x\`，则必须满足 $'a \\subseteq \\mathrm{scope}(x)$。

终端里的 E0597 违反的就是第 2 条：

\`\`\`rust
let r;
{
    let x = 5;
    r = &x;          // x 在下一行的 } 处被释放
}
println!("{r}");     // r 仍在使用
\`\`\`

把 \`println!\` 移进内层作用域，或者把 \`x\` 提到外层即可。检查只在编译期进行，代价约为 $O(n)$，运行时零开销。`;

/** Alt+W over the compiler error in the terminal. */
export const IMAGE_ANSWER = `\`x\` is dropped at the end of the inner block while \`r\` still borrows it, so rustc rejects the program (E0597).

The borrow \`&x\` gets a lifetime $'a$ that must cover every later use of \`r\`. The last use is the \`println!\` on line 9, but \`x\` only lives until line 8, so $'a \\not\\subseteq \\mathrm{scope}(x)$.

Make \`x\` outlive the borrow:

\`\`\`rust
let x = 5;
let r = &x;
println!("r: {r}"); // ok: x is still alive
\`\`\`

Or move the \`println!\` into the inner block. Either way there is no runtime cost: borrow checking happens entirely at compile time.`;

export const FOLLOWUP_ANSWER = `可以，但没必要。\`Rc<T>\` 把单一所有权换成引用计数，每次 \`clone\` 只是 $O(1)$ 的计数加一：

\`\`\`rust
let r;
{
    let x = Rc::new(5);
    r = Rc::clone(&x);
}
println!("{r}"); // 5
\`\`\`

这里直接把 \`x\` 移到外层更简单，也没有计数开销。`;

/** Alt+E with VS Code in front: stage A list + stage B anchors (lens CSS px). */
export const SCAN_ITEMS: ScanItem[] = [
  {
    key: "0",
    label: "Dangling borrow (E0597)",
    brief: "`r = &x` outlives `x`, which is dropped at the end of the block.",
    detail:
      "`x` lives only inside the inner block, but `r` is used by `println!` after it. Move `let x = 5;` above the block or print inside it.",
    rect: AT.line7,
  },
  {
    key: "1",
    label: "unwrap() can panic",
    brief: '`scores.get("alice")` returns `None` if the key is missing.',
    detail:
      'Use `scores.get("alice").copied().unwrap_or(0)` or handle the `None` case with `if let Some(best) = …` instead of panicking.',
    rect: AT.line12,
  },
  {
    key: "2",
    label: "Division by zero",
    brief: "`total / scores.len()` panics when the map is empty.",
    detail: "Guard with `if scores.is_empty()` or compute the mean as `f64` and accept `NaN`: $\\bar{x} = \\frac{1}{n}\\sum x_i$ needs $n > 0$.",
    rect: AT.line14,
  },
];

const now = Date.now();
export const HISTORY: HistoryEntry[] = [
  {
    id: "h1",
    kind: "text",
    tldr: "`Cow<'a, str>` borrows when it can and clones only when it must.",
    text: "`Cow<'a, str>` borrows when it can and clones only when it must.\n\nIt is an enum of `Borrowed(&'a str)` and `Owned(String)`; `to_mut()` clones on first write.",
    createdAt: now - 26 * 60_000,
  },
  {
    id: "h2",
    kind: "image",
    tldr: "The chart shows p99 latency doubling after the 14:00 deploy.",
    text: "The chart shows p99 latency doubling after the 14:00 deploy.\n\np50 is flat, so it is a tail problem: look at lock contention or GC pauses.",
    createdAt: now - 3 * 3_600_000,
  },
  {
    id: "h3",
    kind: "text",
    tldr: "幂等（idempotent）指同一操作执行一次和多次的效果相同。",
    text: "幂等（idempotent）指同一操作执行一次和多次的效果相同。\n\n例如 HTTP 的 `PUT` 和 `DELETE`；`POST` 一般不是。数学上 $f(f(x)) = f(x)$。",
    createdAt: now - 26 * 3_600_000,
  },
];
