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

/** TL;DR = first non-empty line; if the answer opens with code or display math, the first sentence after it. */
export function tldr(text: string): string {
  const lines = text.split(/\r?\n/);
  let inBlock = false;
  for (const raw of lines) {
    const line = raw.trim();
    if (line.startsWith("```") || line.startsWith("$$")) {
      // Toggle on an opening fence; a one-line $$…$$ opens and closes.
      if (!(line.startsWith("$$") && line.length > 2 && line.endsWith("$$"))) inBlock = !inBlock;
      continue;
    }
    if (inBlock || !line) continue;
    const p = plain(line);
    if (!p) continue;
    const sentence = p.match(/^.*?[。.!?！？](\s|$)/);
    return clip(sentence && sentence[0].length < p.length ? sentence[0].trim() : p);
  }
  return "";
}

/** Everything after the TL;DR paragraph (used where TL;DR is shown separately). */
export function body(text: string): string {
  const i = text.search(/\r?\n\s*\r?\n/);
  return i < 0 ? "" : text.slice(i).trim();
}
