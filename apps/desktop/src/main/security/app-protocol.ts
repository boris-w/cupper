// The `app` protocol (design 5.7.8). The UI only ever loads from it: never
// from `file://`, never from a remote server. Each host serves one directory
// of the bundle and sends its own CSP.

import { readFile } from "node:fs/promises";
import { extname, resolve, sep } from "node:path";

import type { CustomScheme } from "electron";

export const APP_SCHEME = "app";

/**
 * Registered before `ready`. Standard and secure give every host its own
 * origin, so the review document stays cross-origin to the main document.
 */
export const APP_SCHEME_PRIVILEGES: CustomScheme = {
  scheme: APP_SCHEME,
  privileges: { standard: true, secure: true, supportFetchAPI: true, codeCache: true },
};

/** The main document's CSP (design 5.7.8). */
export const MAIN_DOCUMENT_CSP = [
  "default-src 'none'",
  "script-src 'self'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' blob: data:",
  "font-src 'self'",
  "worker-src 'self'",
  "connect-src 'none'",
  "frame-src app://cupper-review",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'",
  "require-trusted-types-for 'script'",
  "trusted-types cupper-worker",
].join("; ");

export interface AppHost {
  /** The directory the host serves. */
  root: string;
  csp: string;
}

const CONTENT_TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json",
  ".map": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".webp": "image/webp",
  ".woff2": "font/woff2",
  ".wasm": "application/wasm",
};

/** The `protocol.handle` handler that serves `hosts`, keyed by host name. */
export function appProtocolHandler(
  hosts: Record<string, AppHost>,
  read: (file: string) => Promise<Uint8Array> = readFile,
): (request: Request) => Promise<Response> {
  return async (request) => {
    const url = new URL(request.url);
    const host = Object.hasOwn(hosts, url.host) ? hosts[url.host] : undefined;
    if (host === undefined) {
      return new Response(null, { status: 404 });
    }
    const headers = { "Content-Security-Policy": host.csp, "X-Content-Type-Options": "nosniff" };
    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response(null, { status: 405, headers });
    }
    const file = fileFor(host.root, url.pathname);
    if (file === null) {
      return new Response(null, { status: 404, headers });
    }
    let body: Uint8Array;
    try {
      body = await read(file);
    } catch {
      return new Response(null, { status: 404, headers });
    }
    const type = CONTENT_TYPES[extname(file)] ?? "application/octet-stream";
    return new Response(body, { headers: { ...headers, "Content-Type": type } });
  };
}

/**
 * The file `pathname` names under `root`, or `null` when it leaves `root`.
 * The path is decoded first, so `%2e%2e%2f` counts as `../`; then it is
 * resolved, and only then checked against the root.
 */
function fileFor(root: string, pathname: string): string | null {
  let path: string;
  try {
    path = decodeURIComponent(pathname);
  } catch {
    return null;
  }
  if (path.includes("\0") || path.includes("\\")) {
    return null;
  }
  if (path.endsWith("/")) {
    path += "index.html";
  }
  const base = resolve(root);
  const file = resolve(base, `.${path}`);
  return file.startsWith(base + sep) ? file : null;
}
