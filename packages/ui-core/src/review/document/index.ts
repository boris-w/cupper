// The review document (design 5.7.5): the diff view on an origin of its own,
// a view of what the main document holds. It takes one port, the first the
// embedding document sends with `init`, and listens to nothing on `window`
// after that; each message is parsed against the schema, and one that does
// not fit is dropped. Keys go to the main document, whose shortcuts they are.

import { createElement } from "react";
import { createRoot } from "react-dom/client";

import { parse, REVIEW_PROTOCOL, zToReview, type FromReview, type ToReview } from "../protocol";
import { ReviewDocument, type DocState } from "./ReviewDocument";

/** Mounts the review document on `#root`; `onPort` gets the port it took (e2e builds). */
export function mountReviewDocument(onPort?: (port: MessagePort) => void): void {
  const node = document.getElementById("root");
  if (!node) throw new Error("the review document has no #root");
  const root = createRoot(node);
  let state: DocState = { theme: "light", strings: {}, hunks: new Map() };
  let port: MessagePort | undefined;
  /** The files asked for, of the diff on screen. */
  let asked = new Set<string>();
  let seq = 0;
  const send = (message: FromReview) => port?.postMessage(message);
  const need = (path: string) => {
    const token = state.files?.diff_token;
    if (token === undefined || asked.has(path)) return;
    asked.add(path);
    send({ type: "need_file", diff_token: token, path });
  };
  const setCollapsed = (path: string, collapsed: boolean) =>
    send({ type: "set_collapsed", path, collapsed });
  const draw = () => {
    document.documentElement.dataset.theme = state.theme;
    root.render(createElement(ReviewDocument, { state, need, setCollapsed }));
  };

  const apply = (message: ToReview) => {
    switch (message.type) {
      case "init":
        document.documentElement.lang = message.locale;
        state = { ...state, theme: message.theme, strings: message.strings };
        break;
      case "theme_changed":
        state = { ...state, theme: message.theme };
        break;
      case "files": {
        const same = message.diff_token === state.files?.diff_token;
        if (!same) asked = new Set();
        state = { ...state, files: message, hunks: same ? state.hunks : new Map() };
        break;
      }
      case "render":
        if (message.diff_token !== state.files?.diff_token) return;
        state = { ...state, hunks: new Map(state.hunks).set(message.path, message.hunks) };
        break;
      case "scroll_to":
        state = { ...state, scroll: { path: message.path, line: message.line, seq: ++seq } };
        break;
      case "clear":
        asked = new Set();
        state = { ...state, files: undefined, hunks: new Map(), scroll: undefined };
    }
    draw();
  };

  const embedder = location.ancestorOrigins[0];
  const accept = (event: MessageEvent) => {
    const init = parse(zToReview, event.data);
    const [taken] = event.ports;
    const parent = event.source === window.parent && event.origin === embedder;
    if (!parent || init?.type !== "init" || !taken || event.ports.length !== 1) return;
    window.removeEventListener("message", accept);
    port = taken;
    taken.onmessage = ({ data }) => {
      const message = parse(zToReview, data);
      if (message) apply(message);
    };
    apply(init);
    send({ type: "ready", v: REVIEW_PROTOCOL });
    onPort?.(taken);
  };
  window.addEventListener("message", accept);

  // The review document has no shortcuts of its own (design 5.7.5, point 2).
  document.addEventListener("keydown", (event) => {
    const { key, code, metaKey: meta, shiftKey: shift, altKey: alt, ctrlKey: ctrl } = event;
    send({ type: "key_forward", key, code, meta, shift, alt, ctrl });
  });
  draw();
}
