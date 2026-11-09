// The review document (design 5.7.5), served from app://cupper-review/. No
// preload runs here: it has no ShellAPI and no host. In e2e builds only,
// `window.__cupperReview` sends on the port the document took, as a review
// document gone wrong would, gives the diff token it was last sent
// (E2E-SEC-review-iso), and copies a text, to see where it lands
// (E2E-review-comment). Other builds lack it.

import "@cupper/ui-core/styles.css";

import { mountReviewDocument } from "@cupper/ui-core/review-document";

interface Clipboard {
  writeText(text: string): Promise<void>;
}

const hooks: Parameters<typeof mountReviewDocument>[0] = (port) => {
  let token: unknown;
  port.addEventListener(
    "message",
    ({ data }: { data?: { type?: string; diff_token?: string } }) => {
      if (data?.type === "files") token = data.diff_token;
    },
  );
  const send = (message: unknown) => port.postMessage(message);
  // This project's types are Node's: the page's clipboard, as the page has it.
  const { clipboard } = (globalThis as unknown as { navigator: { clipboard: Clipboard } })
    .navigator;
  const copy = (text: string) =>
    clipboard.writeText(text).then(
      () => "ok",
      (error: unknown) => (error as Error).name,
    );
  Object.assign(globalThis, { __cupperReview: { send, token: () => token, copy } });
};
mountReviewDocument(__CUPPER_FLAVOR__ === "e2e" ? hooks : undefined);
