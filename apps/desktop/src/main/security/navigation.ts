import { isExternalUrl } from "@cupper/shell-api";
import type { WebContents } from "electron";

/**
 * Keeps every webContents on the documents it loaded (design 5.7.8): no
 * navigation, redirect, new window or webview. A link the page tries to
 * follow goes to the default browser instead, when it is http or https.
 *
 * `loadURL` from the main process does not emit `will-navigate`, so the
 * main document still loads. The review document's first load joins the
 * exceptions in M1b.
 */
export function hardenWebContents(contents: WebContents, openExternal: (url: string) => void) {
  contents.on("will-navigate", (event) => {
    event.preventDefault();
    if (event.isMainFrame && isExternalUrl(event.url)) {
      openExternal(event.url);
    }
  });
  contents.on("will-frame-navigate", (event) => event.preventDefault());
  contents.on("will-redirect", (event) => event.preventDefault());
  contents.on("will-attach-webview", (event) => event.preventDefault());
  contents.setWindowOpenHandler(({ url }) => {
    if (isExternalUrl(url)) {
      openExternal(url);
    }
    return { action: "deny" };
  });
}
