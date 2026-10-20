// ANSI in command output (design 5.7.8 "Styles"): the SGR colours and
// attributes become classes from a fixed list (styles/stream.css), so the
// output never picks a colour or a style of its own; every other escape
// sequence is dropped.

const COLORS = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];

const ESC = String.fromCharCode(27);
const BEL = String.fromCharCode(7);
/** CSI (with its parameters and final byte), OSC up to BEL or ST, and two-byte escapes. */
const SEQUENCE = new RegExp(
  `${ESC}\\[([0-9;?]*)([@-~])|${ESC}\\][^${BEL}${ESC}]*(?:${BEL}|${ESC}\\\\)|${ESC}[@-Z\\\\-_]`,
  "g",
);

interface Style {
  color?: string;
  bold: boolean;
  dim: boolean;
  underline: boolean;
}

export interface Span {
  text: string;
  /** Only names of the fixed list, or empty. */
  classes: string;
}

function classes({ color, bold, dim, underline }: Style): string {
  const names = [color && `ansi-${color}`, bold && "ansi-bold", dim && "ansi-dim"];
  return [...names, underline && "ansi-underline"].filter(Boolean).join(" ");
}

function apply(style: Style, params: string): Style {
  const codes = params === "" ? [0] : params.split(";").map(Number);
  let next = { ...style };
  for (let i = 0; i < codes.length; i++) {
    const code = codes[i] ?? 0;
    if (code === 0) {
      next = { bold: false, dim: false, underline: false };
    } else if (code === 1 || code === 2) {
      next[code === 1 ? "bold" : "dim"] = true;
    } else if (code === 22) {
      next = { ...next, bold: false, dim: false };
    } else if (code === 4 || code === 24) {
      next.underline = code === 4;
    } else if ((code >= 30 && code <= 37) || (code >= 90 && code <= 97)) {
      next.color = COLORS[code % 10];
    } else if (code === 39) {
      next.color = undefined;
    } else if (code === 38 || code === 48) {
      // 256 colours and true colour, foreground or background: skipped.
      i += codes[i + 1] === 5 ? 2 : 4;
    }
  }
  return next;
}

/** `output` as runs of text, each with the classes of its SGR state. */
export function ansiSpans(output: string): Span[] {
  const spans: Span[] = [];
  let style: Style = { bold: false, dim: false, underline: false };
  let at = 0;
  const push = (text: string) => {
    const last = spans.at(-1);
    const names = classes(style);
    if (last && last.classes === names) {
      last.text += text;
    } else if (text) {
      spans.push({ text, classes: names });
    }
  };
  for (const match of output.matchAll(SEQUENCE)) {
    push(output.slice(at, match.index));
    at = match.index + match[0].length;
    if (match[2] === "m") {
      style = apply(style, match[1] ?? "");
    }
  }
  push(output.slice(at));
  return spans;
}

export function AnsiText({ text }: { text: string }) {
  return ansiSpans(text).map((span, i) =>
    span.classes ? (
      <span key={i} className={span.classes}>
        {span.text}
      </span>
    ) : (
      span.text
    ),
  );
}
