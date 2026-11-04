// The review document (design 5.7.5), served from app://cupper-review/. No
// preload runs here: it has no ShellAPI and no host. In e2e builds only,
// `window.__cupperReview` sends on the port the document took, as a review
// document gone wrong would (E2E-SEC-review-iso). Other builds lack it.

import "@cupper/ui-core/styles.css";

import { mountReviewDocument } from "@cupper/ui-core/review-document";

const hooks: Parameters<typeof mountReviewDocument>[0] = (port) => {
  const send = (message: unknown) => port.postMessage(message);
  Object.assign(globalThis, { __cupperReview: { send } });
};
mountReviewDocument(__CUPPER_FLAVOR__ === "e2e" ? hooks : undefined);
