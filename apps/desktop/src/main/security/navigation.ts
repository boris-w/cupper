import { isExternalUrl } from "@cupper/shell-api";
import type { WebContents } from "electron";

import { REVIEW_DOCUMENT_URL } from "../window";

/**
 * Keeps every webContents on the documents it loaded (design 5.7.8): no
 * navigation, redirect, new window or webview. A link the page tries to
 * follow goes to the default browser instead, when it is http or https.
 *
 * `loadURL` from the main process does not emit `will-navigate`, so the
 * main document still loads. A frame may load the review document once, as
 * its first navigation; nothing after (design 5.7.5).
 */
export function hardenWebContents(contents: WebContents, openExternal: (url: string) => void) {
  contents.on("will-navigate", (event) => {
    event.preventDefault();
    if (event.isMainFrame && isExternalUrl(event.url)) {
      openExternal(event.url);
    }
  });
  const framed = new Set<number>();
  contents.on("will-frame-navigate", (event) => {
    const frame = event.frame?.frameTreeNodeId;
    const first = frame !== undefined && !framed.has(frame);
    if (frame !== undefined) framed.add(frame);
    if (event.isMainFrame || !first || event.url !== REVIEW_DOCUMENT_URL) event.preventDefault();
  });
  contents.on("will-redirect", (event) => event.preventDefault());
  contents.on("will-attach-webview", (event) => event.preventDefault());
  contents.setWindowOpenHandler(({ url }) => {
    if (isExternalUrl(url)) {
      openExternal(url);
    }
    return { action: "deny" };
  });
}
