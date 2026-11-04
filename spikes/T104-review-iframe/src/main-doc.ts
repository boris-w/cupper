// The main document's side of the review protocol (design 5.7.5): the
// iframe, its MessageChannel, `init` and `render` on the port, and what came
// back. A second port and a plain `window` message try the review document's
// "first port only". The main process reads `t104`.
import { fixture } from "./fixture";

const REVIEW = "app://cupper-review";
const now = () => performance.timeOrigin + performance.now();
const seen = { violations: 0, strays: 0, created: 0, ready: 0, forwarded: [] as string[] };
document.addEventListener("securitypolicyviolation", () => seen.violations++);
window.addEventListener("message", () => seen.strays++);

const frame = document.createElement("iframe");
frame.src = `${REVIEW}/index.html`;
frame.setAttribute("sandbox", "allow-scripts allow-same-origin");
frame.setAttribute("allow", "clipboard-write");
frame.style.cssText = "border: 0; width: 100%; height: 100vh; display: block";
const { port1: port, port2 } = new MessageChannel();
port.onmessage = ({ data }) => {
  if (data.type === "ready") seen.ready = now();
  if (data.type === "key_forward") seen.forwarded.push(data.key);
};
frame.addEventListener(
  "load",
  () => {
    const send = (message: object, ports: MessagePort[] = []) =>
      frame.contentWindow?.postMessage(message, REVIEW, ports);
    send({ type: "init", v: 1, theme: "light", locale: "zh-CN" }, [port2]);
    const stray = new MessageChannel();
    stray.port1.onmessage = () => seen.strays++;
    send({ type: "init", v: 1, theme: "dark", locale: "en" }, [stray.port2]);
    send({ type: "theme_changed", theme: "dark" });
    port.postMessage({ type: "render", path: "src/generated.ts", ...fixture() });
  },
  { once: true },
);
seen.created = now();
document.body.append(frame);
Object.assign(window, { t104: { seen, port } });
