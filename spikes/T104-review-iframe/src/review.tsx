// The review document (design 5.7.5): the first port from app://cupper and
// nothing else, and the diff by @pierre/diffs as it is: its worker started
// by us (`workerFactory`), highlighting by `shiki-js`. Esc and ⌘Enter stay
// here; other keys go to the main document as `key_forward`. The main
// process reads `t104`.
import { MultiFileDiff, Virtualizer, WorkerPoolContextProvider } from "@pierre/diffs/react";
import { useState } from "react";
import { createRoot } from "react-dom/client";

interface Render {
  path: string;
  old: string;
  changed: string;
}
const now = () => performance.timeOrigin + performance.now();
const seen = { violations: 0, ignored: 0, accepted: 0, workers: 0, rows: 0, highlighted: 0, keys: [] as string[] };
document.addEventListener("securitypolicyviolation", () => seen.violations++);
let port: MessagePort | undefined;
let update: (change: { file?: Render; theme?: string }) => void = () => undefined;

window.addEventListener("message", (event) => {
  const data = event.data as { type?: string; theme?: string; locale?: string };
  const first = port === undefined && event.origin === "app://cupper" && event.ports.length === 1;
  if (!first || data?.type !== "init") return void seen.ignored++;
  [port] = event.ports;
  seen.accepted++;
  document.documentElement.lang = String(data.locale);
  update({ theme: data.theme });
  port.onmessage = ({ data: message }) => {
    if (message.type === "render") update({ file: message });
    if (message.type === "theme_changed") update({ theme: message.theme });
    if (message.type === "scroll_to") scrollToLine(message.line);
  };
  port.postMessage({ type: "ready", v: 1 });
});

document.addEventListener("keydown", (event) => {
  seen.keys.push(event.key);
  if (event.key === "Escape" || (event.key === "Enter" && event.metaKey)) return;
  const modifiers = { meta: event.metaKey, shift: event.shiftKey, alt: event.altKey };
  port?.postMessage({ type: "key_forward", key: event.key, modifiers });
});

/** Every shadow root's elements, where pierre renders. */
const shadow = (selector: string) =>
  [...document.querySelectorAll("*")].flatMap((el) => [...(el.shadowRoot?.querySelectorAll(selector) ?? [])]);
const scroller = () => document.querySelector<HTMLElement>(".t104-scroller");
const line = (n: number) => shadow(`[data-line="${n}"]`)[0];

function scrollToLine(n: number) {
  const rows = shadow("[data-line]");
  const height = (rows[1]?.getBoundingClientRect().top ?? 0) - (rows[0]?.getBoundingClientRect().top ?? 0);
  scroller()?.scrollTo({ top: n * (height || 20) });
}

/** Scrolls from top to bottom, 120 px a frame; the frames' intervals. */
async function scrollFrames(): Promise<number[]> {
  const el = scroller();
  if (!el) return [];
  el.scrollTop = 0;
  const frames: number[] = [];
  let last = await new Promise<number>(requestAnimationFrame);
  while (el.scrollTop + el.clientHeight < el.scrollHeight - 1) {
    el.scrollTop += 120;
    const at = await new Promise<number>(requestAnimationFrame);
    frames.push(at - last);
    last = at;
  }
  return frames;
}

/** When rows, then highlighted tokens, first show. */
function watch() {
  if (!seen.rows && shadow("[data-line]").length) seen.rows = now();
  if (!seen.highlighted && shadow("[data-line] span[style]").length) seen.highlighted = now();
  if (!seen.highlighted) requestAnimationFrame(watch);
}

function App() {
  const [state, setState] = useState<{ file?: Render; theme: string }>({ theme: "light" });
  update = (change) => setState((old) => ({ ...old, ...change }));
  document.documentElement.dataset.theme = state.theme;
  if (!state.file) return null;
  requestAnimationFrame(watch);
  const workerFactory = () => (seen.workers++, new Worker(new URL("./worker.js", import.meta.url), { type: "module" }));
  const options = { themeType: state.theme as "light" | "dark", preferredHighlighter: "shiki-js" as const };
  return (
    <WorkerPoolContextProvider
      poolOptions={{ workerFactory, poolSize: 2 }}
      highlighterOptions={{ preferredHighlighter: "shiki-js", langs: ["typescript"] }}
    >
      <Virtualizer className="t104-scroller" style={{ height: "100vh", overflow: "auto" }}>
        <MultiFileDiff
          oldFile={{ name: state.file.path, contents: state.file.old }}
          newFile={{ name: state.file.path, contents: state.file.changed }}
          options={options}
        />
      </Virtualizer>
    </WorkerPoolContextProvider>
  );
}

Object.assign(window, { t104: { seen, shadow, line, scrollFrames, scrollToLine } });
createRoot(document.getElementById("root") as HTMLElement).render(<App />);
