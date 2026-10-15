// Monochrome Markdown + LaTeX renderer for streamed answers.
import "katex/dist/katex.min.css";
import { memo, type MouseEvent } from "react";
import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import remend from "remend";
import { openUrl } from "@tauri-apps/plugin-opener";

/** Normalise \( \) and \[ \] to $ / $$ outside code, so remark-math picks them up. */
function normalizeMath(src: string): string {
  return src
    .split(/(```[\s\S]*?(?:```|$)|`[^`\n]*`)/g)
    .map((part, i) =>
      i % 2 === 1
        ? part
        : part
            .replace(/\\\[([\s\S]*?)\\\]/g, (_, m) => `$$${m}$$`)
            .replace(/\\\(([\s\S]*?)\\\)/g, (_, m) => `$${m}$`),
    )
    .join("");
}

function onClick(e: MouseEvent<HTMLDivElement>) {
  // Never navigate the overlay webview; open links in the default browser.
  const a = (e.target as HTMLElement).closest("a");
  if (!a) return;
  e.preventDefault();
  const href = a.getAttribute("href");
  if (href && /^https?:\/\//i.test(href)) void openUrl(href);
}

export const Markdown = memo(function Markdown({ text, streaming }: { text: string; streaming: boolean }) {
  // remend closes unterminated syntax (``` ** $$ …) so partial streams render cleanly.
  const src = normalizeMath(streaming ? remend(text) : text);
  return (
    <div className="md" onClick={onClick}>
      <ReactMarkdown remarkPlugins={[remarkGfm, remarkMath]} rehypePlugins={[rehypeKatex]}>
        {src}
      </ReactMarkdown>
    </div>
  );
});
