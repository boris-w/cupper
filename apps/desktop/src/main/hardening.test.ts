import { EventEmitter } from "node:events";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import type { Session, WebContents } from "electron";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

import type { Flavor } from "../shared/flavor";
import {
  APP_SCHEME_PRIVILEGES,
  MAIN_DOCUMENT_CSP,
  appProtocolHandler,
} from "./security/app-protocol";
import { hardenWebContents } from "./security/navigation";
import { restrictPermissions } from "./security/permissions";
import { enforceSwitchAllowlist } from "./security/switches";
import { mainWindowOptions, webPreferences } from "./window";

const FLAVORS: Flavor[] = ["release", "local", "e2e", "dev"];

describe("UNIT-main-hardening", () => {
  describe("webPreferences", () => {
    it.each(FLAVORS)("are locked down in the %s build", (flavor) => {
      expect(webPreferences(flavor, "/app/preload.cjs")).toMatchObject({
        preload: "/app/preload.cjs",
        sandbox: true,
        contextIsolation: true,
        nodeIntegration: false,
        nodeIntegrationInWorker: false,
        nodeIntegrationInSubFrames: false,
        webviewTag: false,
        webSecurity: true,
        allowRunningInsecureContent: false,
        experimentalFeatures: false,
        navigateOnDragDrop: false,
      });
    });

    it("have no DevTools in release and local builds", () => {
      expect(FLAVORS.map((flavor) => webPreferences(flavor, "p").devTools)).toEqual([
        false,
        false,
        true,
        true,
      ]);
    });

    it("go into the main window, which follows the design", () => {
      const options = mainWindowOptions("release", "p");
      expect(options).toMatchObject({
        width: 1280,
        height: 820,
        minWidth: 480,
        minHeight: 600,
        titleBarStyle: "hiddenInset",
        trafficLightPosition: { x: 16, y: 15 },
        show: false,
      });
      expect(options.webPreferences).toEqual(webPreferences("release", "p"));
      expect(options.acceptFirstMouse).toBeUndefined();
    });
  });

  describe("navigation", () => {
    function hardened() {
      const contents = new EventEmitter() as EventEmitter & {
        openHandler?: (details: { url: string }) => { action: string };
        setWindowOpenHandler(handler: (details: { url: string }) => { action: string }): void;
      };
      contents.setWindowOpenHandler = (handler) => {
        contents.openHandler = handler;
      };
      const openExternal = vi.fn();
      hardenWebContents(contents as unknown as WebContents, openExternal);
      const emit = (name: string, url: string, isMainFrame = true) => {
        const event = { url, isMainFrame, preventDefault: vi.fn() };
        contents.emit(name, event);
        return event.preventDefault;
      };
      const open = (url: string) => contents.openHandler?.({ url });
      return { emit, open, openExternal };
    }

    it("blocks every navigation, redirect and webview", () => {
      const { emit, openExternal } = hardened();
      for (const name of [
        "will-navigate",
        "will-frame-navigate",
        "will-redirect",
        "will-attach-webview",
      ]) {
        expect(emit(name, "app://cupper/other.html"), name).toHaveBeenCalledOnce();
      }
      expect(openExternal).not.toHaveBeenCalled();
    });

    it("hands http and https links of the main frame to the browser", () => {
      const { emit, openExternal } = hardened();
      expect(emit("will-navigate", "https://example.com/a")).toHaveBeenCalledOnce();
      expect(emit("will-navigate", "http://example.com/b")).toHaveBeenCalledOnce();
      emit("will-navigate", "https://example.com/frame", false);
      emit("will-frame-navigate", "https://example.com/c");
      emit("will-redirect", "https://example.com/d");
      expect(openExternal.mock.calls).toEqual([
        ["https://example.com/a"],
        ["http://example.com/b"],
      ]);
    });

    it("denies every new window and opens only http and https externally", () => {
      const { open, openExternal } = hardened();
      for (const url of [
        "https://example.com/",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "app://cupper/index.html",
        "x-apple.systempreferences:com.apple.preference.security",
        "not a url",
      ]) {
        expect(open(url), url).toEqual({ action: "deny" });
      }
      expect(openExternal.mock.calls).toEqual([["https://example.com/"]]);
    });
  });

  describe("permissions", () => {
    function handlers() {
      const session = {
        setPermissionRequestHandler: vi.fn(),
        setPermissionCheckHandler: vi.fn(),
        setDevicePermissionHandler: vi.fn(),
      };
      restrictPermissions(session as unknown as Session);
      const [request] = session.setPermissionRequestHandler.mock.lastCall ?? [];
      const [check] = session.setPermissionCheckHandler.mock.lastCall ?? [];
      const [device] = session.setDevicePermissionHandler.mock.lastCall ?? [];
      return { request, check, device };
    }

    const DENIED = [
      "clipboard-read",
      "media",
      "geolocation",
      "notifications",
      "midi",
      "openExternal",
      "fullscreen",
      "display-capture",
      "hid",
    ];

    it("grants requests for clipboard-sanitized-write only", () => {
      const { request } = handlers();
      const answer = (permission: string) => {
        const callback = vi.fn();
        request(null, permission, callback, {});
        return callback.mock.lastCall?.[0];
      };
      expect(answer("clipboard-sanitized-write")).toBe(true);
      for (const permission of DENIED) {
        expect(answer(permission), permission).toBe(false);
      }
    });

    it("answers checks the same way and refuses every device", () => {
      const { check, device } = handlers();
      expect(check(null, "clipboard-sanitized-write", "app://cupper", {})).toBe(true);
      for (const permission of DENIED) {
        expect(check(null, permission, "app://cupper", {}), permission).toBe(false);
      }
      expect(device({ deviceType: "hid", origin: "app://cupper" })).toBe(false);
    });
  });

  describe("app protocol", () => {
    // <dir>/renderer is served; <dir>/secret.txt and <dir>/renderer-x/secret.txt
    // sit right next to it.
    let dir = "";
    let root = "";
    beforeAll(async () => {
      dir = await mkdtemp(join(tmpdir(), "cupper-app-protocol-"));
      root = join(dir, "renderer");
      await mkdir(join(root, "assets"), { recursive: true });
      await mkdir(join(dir, "renderer-x"));
      await writeFile(join(root, "index.html"), "<!doctype html>");
      await writeFile(join(root, "assets", "app.js"), "export {};");
      await writeFile(join(dir, "secret.txt"), "secret");
      await writeFile(join(dir, "renderer-x", "secret.txt"), "secret");
    });
    afterAll(async () => {
      if (dir) {
        await rm(dir, { recursive: true, force: true });
      }
    });

    function serve() {
      const read = vi.fn((file: string) => readFile(file));
      const handler = appProtocolHandler({ cupper: { root, csp: MAIN_DOCUMENT_CSP } }, read);
      return { read, get: (url: string) => handler(new Request(url)) };
    }

    it("registers app as a standard, secure scheme", () => {
      expect(APP_SCHEME_PRIVILEGES).toEqual({
        scheme: "app",
        privileges: { standard: true, secure: true, supportFetchAPI: true, codeCache: true },
      });
    });

    it("serves the bundle with the main document's CSP", async () => {
      const { get } = serve();
      for (const [url, type] of [
        ["app://cupper/index.html", "text/html; charset=utf-8"],
        ["app://cupper/", "text/html; charset=utf-8"],
        ["app://cupper/assets/app.js", "text/javascript; charset=utf-8"],
      ] as const) {
        const response = await get(url);
        expect(response.status, url).toBe(200);
        expect(response.headers.get("content-type"), url).toBe(type);
        expect(response.headers.get("content-security-policy"), url).toBe(MAIN_DOCUMENT_CSP);
        expect(response.headers.get("x-content-type-options"), url).toBe("nosniff");
      }
      expect(MAIN_DOCUMENT_CSP).toContain("default-src 'none'");
      expect(MAIN_DOCUMENT_CSP).toContain("script-src 'self';");
      expect(MAIN_DOCUMENT_CSP).toContain("require-trusted-types-for 'script'");
    });

    it("refuses paths that leave the root, before reading anything", async () => {
      const { get, read } = serve();
      for (const url of [
        "app://cupper/..%2fsecret.txt",
        "app://cupper/%2e%2e%2fsecret.txt",
        "app://cupper/%2E%2E%2Fsecret.txt",
        "app://cupper/assets/..%2f..%2fsecret.txt",
        "app://cupper/..%2frenderer-x%2fsecret.txt",
        "app://cupper/%2e%2e%5csecret.txt",
        "app://cupper/index.html%00.js",
        "app://cupper/%E0%A4%A",
      ]) {
        const response = await get(url);
        expect(response.status, url).toBe(404);
        expect(await response.text(), url).toBe("");
      }
      expect(read).not.toHaveBeenCalled();
    });

    it("refuses other hosts and methods, and files that are not there", async () => {
      const { get } = serve();
      expect((await get("app://cupper-review/index.html")).status).toBe(404);
      expect((await get("app://evil/index.html")).status).toBe(404);
      expect((await get("app://cupper/missing.js")).status).toBe(404);
      expect((await get("app://cupper/assets")).status).toBe(404);
      const post = appProtocolHandler({ cupper: { root, csp: MAIN_DOCUMENT_CSP } });
      const response = await post(new Request("app://cupper/index.html", { method: "POST" }));
      expect(response.status).toBe(405);
    });
  });

  describe("command-line switches", () => {
    function start(args: string[]) {
      const exit = vi.fn();
      const report = vi.fn();
      enforceSwitchAllowlist(args, exit, report);
      return { exit, report };
    }

    it.each([
      ["--inspect"],
      ["--inspect-brk=9229"],
      ["--remote-debugging-port=9222"],
      ["--js-flags=--allow-natives-syntax"],
      ["--no-sandbox"],
      ["--disable-web-security"],
      ["--user-data-dir=/tmp/x"],
      ["--proxy-server=127.0.0.1:8080"],
      ["-inspect"],
      ["--"],
      ["--some-future-switch"],
    ])("exits on %s", (flag) => {
      const { exit, report } = start(["positional", flag]);
      expect(exit).toHaveBeenCalledExactlyOnceWith(1);
      expect(report.mock.lastCall?.[0]).toContain(flag);
    });

    it("starts without switches", () => {
      expect(start([]).exit).not.toHaveBeenCalled();
      expect(start(["positional"]).exit).not.toHaveBeenCalled();
    });
  });
});
