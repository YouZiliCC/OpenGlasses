// Answer format contract (DESIGN §2.3): first line = one-sentence TL;DR, blank line, details.

const MAX = 120;

/** Strip inline Markdown so the TL;DR reads as plain text. */
function plain(line: string): string {
  return line
    .replace(/^#+\s*/, "")
    .replace(/^[-*+>]\s+/, "")
    .replace(/\*\*|__|`/g, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .trim();
}

function clip(s: string): string {
  return s.length > MAX ? s.slice(0, MAX - 1).trimEnd() + "…" : s;
}

/** One walk over the lines: the TL;DR line and everything after it. */
function split(text: string): { head: string; rest: string } {
  const lines = text.split(/\r?\n/);
  let inBlock = false;
  let afterBlock = false;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trim();
    if (line.startsWith("```") || line.startsWith("$$")) {
      // Toggle on an opening fence; a one-line $$…$$ opens and closes.
      if (!(line.startsWith("$$") && line.length > 2 && line.endsWith("$$"))) inBlock = !inBlock;
      afterBlock = true;
      continue;
    }
    if (inBlock || !line) continue;
    const p = plain(line);
    if (!p) continue;
    // Only when the answer opened with code/math is the TL;DR cut to the next line's first sentence.
    const sentence = afterBlock ? p.match(/^.*?[。.!?！？](\s|$)/) : null;
    return { head: clip(sentence ? sentence[0].trim() : p), rest: lines.slice(i + 1).join("\n").trim() };
  }
  return { head: "", rest: "" };
}

/** TL;DR = first non-empty line; if the answer opens with code or display math, the first sentence after it. */
export function tldr(text: string): string {
  return split(text).head;
}

/** Everything after the TL;DR line (used where the TL;DR is shown separately). */
export function body(text: string): string {
  return split(text).rest;
}
