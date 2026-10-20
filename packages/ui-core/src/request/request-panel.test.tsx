// @vitest-environment jsdom
import { HostClient } from "@cupper/host-client";
import { hostError, MockHost } from "@cupper/host-client/mock";
import type { InteractionOption } from "@cupper/protocol";
import type { Handshake } from "@cupper/shell-api";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { HostContext, I18nProvider, ShellProvider } from "../app/providers";
import { RouterProvider } from "../app/router";
import { ComposerArea } from "../composer/QueueList";
import { ToasterProvider } from "../feedback/toaster";
import { DEFAULT_PREFS } from "../layout/layout";
import { StateProvider, UiProvider } from "../state/provider";
import { createUiStore } from "../state/ui";
import { initialSelection, move, panelKey, skipOutcome } from "./logic";
import { record } from "./RequestRow";

beforeEach(() =>
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"], now: 100_000 }),
);
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const option = (option_id: string, scope: string, extra: Partial<InteractionOption> = {}) =>
  ({
    ...{ option_id, name: `选项 ${option_id}`, kind: "allow_once", requires: "write" },
    effect: { scope, persistent: false },
    ...extra,
  }) as InteractionOption;
/** Allow once, allow for the session (privileged), unknown effect, reject. */
const OPTIONS = [
  option("o1", "once"),
  option("o2", "session", { requires: "privileged", kind: "allow_always" }),
  option("o3", "unknown"),
  option("o4", "reject", { kind: "reject_once" }),
];
const requested = (request: object, request_id = "r_1") => ({
  type: "interaction.requested",
  data: { request_id, digest: "d", created_at: 1, actionable_by: ["desktop_ui"], request },
});
const permission = (options = OPTIONS) =>
  requested({
    ...{ type: "permission", title: "Run tests", content_truncated: false, options },
    tool: { tool_kind: "execute", paths: [], command: { type: "text", text: "cargo test" } },
  });

const frame = () => act(() => new Promise((done) => requestAnimationFrame(() => done(undefined))));

/** Session s_1 with `ops`, its host a MockHost; `request` arrives after mount, as live events do. */
async function setup(opts: { ops?: object[]; request?: object; respond?: () => object } = {}) {
  const mock = new MockHost([{ method: "initialize", params: {}, result: {} }]);
  mock.handle("stream/subscribe", ({ id, conn, params }) => {
    const { stream } = params as { stream: string };
    conn.reply(id, { result: { stream, mode: "snapshot", at_seq: 1, head_seq: 1, epoch: 1 } });
    const ops = stream === "index" ? [] : (opts.ops ?? []);
    conn.send({
      jsonrpc: "2.0",
      method: "stream/snapshot",
      params: { stream, at_seq: 1, epoch: 1, page: 0, last: true, ops },
    });
    return undefined;
  });
  mock.handle("interaction/respond", () => (opts.respond?.() ?? { result: {} }) as never);
  mock.handle("session/send", () => ({ result: { disposition: "queued" } }));
  const client = new HostClient();
  const conn = mock.connect();
  conn.initialized = true;
  client.attach(conn.port(), {} as Handshake);
  const { shell, controls } = createBrowserShell();
  controls.setPrivilegedResult({ status: "confirmed" });
  const info = await shell.init();
  const ui = createUiStore(
    { prefs: DEFAULT_PREFS, overlayOpen: false },
    { page: "session", sessionId: "s_1" },
  );
  render(
    <ShellProvider api={shell} info={info}>
      <I18nProvider locale="zh-CN">
        <ToasterProvider>
          <HostContext value={{ client, state: client.state, handshake: undefined }}>
            <UiProvider ui={ui}>
              <StateProvider>
                <RouterProvider>
                  <ComposerArea sessionId="s_1" />
                </RouterProvider>
              </StateProvider>
            </UiProvider>
          </HostContext>
        </ToasterProvider>
      </I18nProvider>
    </ShellProvider>,
  );
  await frame();
  let seq = 2;
  const push = async (...bodies: object[]) => {
    const events = bodies.map((body) => ({ seq: seq++, v: 1, ts: 1, ...body }));
    conn.send({
      jsonrpc: "2.0",
      method: "stream/events",
      params: { stream: "session/s_1", events },
    });
    await frame();
  };
  if (opts.request) await push(opts.request);
  const panel = () => document.querySelector<HTMLElement>("[data-request-panel]");
  const responses = () =>
    mock.requests
      .filter((r) => r.method === "interaction/respond")
      .map((r) => (r.params as { outcome: object }).outcome);
  const press = async (key: string, init: object = {}) => {
    await act(
      async () => void fireEvent.keyDown(document.activeElement ?? document.body, { key, ...init }),
    );
  };
  const ready = () => act(() => vi.advanceTimersByTime(500));
  const methods = () => mock.requests.map((r) => r.method).filter((m) => !m.startsWith("stream/"));
  const sent = () => mock.requests.filter((r) => r.method === "session/send").map((r) => r.params);
  return { ui, controls, push, panel, responses, press, ready, methods, sent, mock };
}

/** blob/get serves `text`, its one chunk carrying `bytes` (other bytes fail the hash). */
async function serveBlob(mock: MockHost, text: string, bytes = new TextEncoder().encode(text)) {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  const hash = Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
  mock.handle("blob/get", ({ conn }) => {
    conn.sendBlob({ hash, offset: 0, last: true, bytes });
    return { result: { hash, chunks: 1, offset: 0, length: bytes.length, size: bytes.length } };
  });
  return { hash, media_type: "text/plain", size: bytes.length };
}

describe("UNIT-request-panel", () => {
  test("rules: initial selection, moving past unknown effects, skip, and what a key is", () => {
    expect(initialSelection(OPTIONS)).toBe(0);
    expect(initialSelection([OPTIONS[1], OPTIONS[2]] as InteractionOption[])).toBe(-1);
    expect(initialSelection(OPTIONS, true)).toBe(3);
    expect(move(OPTIONS, 1, 1)).toBe(3);
    expect(move(OPTIONS, 3, -1)).toBe(1);
    expect(skipOutcome({ type: "permission", options: OPTIONS } as never)).toEqual({
      type: "selected",
      option_id: "o4",
    });
    const persistentOnly = [
      option("o5", "reject", { effect: { scope: "reject", persistent: true } }),
    ];
    expect(skipOutcome({ type: "permission", options: persistentOnly } as never)).toEqual({
      type: "dismissed",
    });
    expect(panelKey({ key: "3" })).toEqual({ type: "digit", index: 2 });
    expect(panelKey({ key: "Enter", metaKey: true })).toEqual({ type: "cmd_submit" });
    expect(panelKey({ key: "a" })).toEqual({ type: "typing" });
    expect(panelKey({ key: "Enter", isComposing: true })).toEqual({ type: "typing" });
    expect(panelKey({ key: "Shift" })).toEqual({ type: "other" });
  });

  test("arming: keys restart the 500 ms, clicks pressed before it ends do nothing", async () => {
    const { panel, press, ready, responses } = await setup({ ops: [permission()] });
    expect(panel()?.dataset.ready).toBeUndefined();
    act(() => vi.advanceTimersByTime(400));
    fireEvent.keyDown(panel() as HTMLElement, { key: "1" });
    act(() => vi.advanceTimersByTime(400));
    expect(panel()?.dataset.ready).toBeUndefined();
    const reject = screen.getByRole("option", { name: /选项 o4/ });
    fireEvent.mouseDown(reject);
    await ready();
    expect(panel()?.dataset.ready).toBe("true");
    await act(async () => void fireEvent.click(reject));
    await press("1");
    expect(responses()).toEqual([]);
  });

  test("the focus moves to the panel only from an empty composer not typed in for 2 s", async () => {
    const auto = await setup();
    const area = screen.getByRole("textbox");
    area.focus();
    act(() => vi.advanceTimersByTime(2000));
    await auto.push(permission());
    expect(auto.panel()?.dataset.activation).toBe("auto");
    expect(auto.panel()?.contains(document.activeElement)).toBe(true);
    cleanup();

    for (const before of ["draft", "typing", "elsewhere"] as const) {
      const run = await setup();
      const box = screen.getByRole("textbox");
      if (before !== "elsewhere") box.focus();
      if (before === "draft") run.ui.getState().setDraft("s_1", "还没发");
      if (before === "typing") fireEvent.keyDown(box, { key: "a" });
      else act(() => vi.advanceTimersByTime(2000));
      await run.push(permission());
      expect([before, run.panel()?.dataset.activation]).toEqual([before, "manual"]);
      expect(screen.getByText("按 Tab 或点击以处理请求")).toBeTruthy();
      expect(run.panel()?.contains(document.activeElement)).toBe(false);
      cleanup();
    }
  });

  test("a typing key revokes the keyboard; Tab or a click brings it back", async () => {
    const run = await setup();
    screen.getByRole("textbox").focus();
    act(() => vi.advanceTimersByTime(2000));
    await run.push(permission());
    await run.ready();
    await run.press("x");
    expect(run.panel()?.dataset.activation).toBe("manual");
    await run.press("1");
    expect(run.responses()).toEqual([]);
    fireEvent.focus(screen.getByRole("button", { name: /跳过/ }));
    expect(run.panel()?.dataset.activation).toBe("auto");
  });

  test("a click on a panel waiting for Tab or a click keeps the hint until the click is over", async () => {
    const run = await setup();
    await run.push(permission());
    await run.ready();
    const skip = screen.getByRole("button", { name: /跳过/ });
    fireEvent.mouseDown(skip);
    // Nothing moved under the pointer between its press and its release.
    expect(run.panel()?.dataset.activation).toBe("auto");
    expect(screen.getByRole("status").textContent).toBe("按 Tab 或点击以处理请求");
    await act(async () => void fireEvent.click(skip));
    expect(screen.queryByRole("status")).toBeNull();
    expect(run.responses()).toEqual([{ type: "selected", option_id: "o4" }]);
  });

  test("numbers are positions in options: privileged confirms, unknown effect does nothing", async () => {
    const run = await setup();
    screen.getByRole("textbox").focus();
    act(() => vi.advanceTimersByTime(2000));
    await run.push(permission());
    await run.ready();
    await run.press("3");
    await run.press("2");
    expect(run.controls.privilegedActions).toEqual([
      { kind: "interaction", session_id: "s_1", request_id: "r_1", option_id: "o2" },
    ]);
    expect(run.responses()).toEqual([]);
    await run.press("4");
    expect(run.responses()).toEqual([{ type: "selected", option_id: "o4" }]);
  });

  test("Enter submits the selection; the first ⌘Enter only hints; Esc skips, never privileged", async () => {
    const run = await setup();
    screen.getByRole("textbox").focus();
    act(() => vi.advanceTimersByTime(2000));
    await run.push(permission([OPTIONS[1], OPTIONS[0], OPTIONS[3]] as InteractionOption[]));
    await run.ready();
    await run.press("Enter", { metaKey: true });
    expect(screen.getByText("按 Enter 提交")).toBeTruthy();
    expect(run.responses()).toEqual([]);
    await run.press("ArrowDown");
    await run.press("Enter", { metaKey: true });
    await run.press("Escape");
    expect(run.responses()).toEqual([
      { type: "selected", option_id: "o4" },
      { type: "selected", option_id: "o4" },
    ]);
    expect(run.controls.privilegedActions).toEqual([]);
  });

  test('"请再按一次" only within the cooldown after the window came forward', async () => {
    const lacking = () => hostError("forbidden", "user_activation_required");
    for (const [focusedAgo, expected] of [
      [200, "again"],
      [5000, "manual"],
    ] as const) {
      const run = await setup({ respond: lacking });
      screen.getByRole("textbox").focus();
      act(() => vi.advanceTimersByTime(2000));
      await run.push(permission());
      await run.ready();
      act(() => run.controls.setFocus(true, Date.now() - focusedAgo));
      await run.press("Enter");
      // The rejection travels through the mock port's microtasks.
      await act(async () => {
        for (let tick = 0; tick < 20; tick++) await Promise.resolve();
      });
      expect(screen.queryByText("请再按一次") !== null).toBe(expected === "again");
      expect(run.panel()?.dataset.activation).toBe(expected === "again" ? "auto" : "manual");
      cleanup();
    }
  });

  test("a question answers field by field; a closed or unsupported one is dismissed", async () => {
    const field = (field_id: string) => ({
      ...{
        field_id,
        label: `问题 ${field_id}`,
        type: "single_choice",
        required: false,
        allow_custom: false,
      },
      choices: [
        { label: "甲", value: "a" },
        { label: "乙", value: "b" },
      ],
    });
    const question = {
      type: "question",
      prompt: "怎么做？",
      unsupported_schema: false,
      fields: [field("f1"), field("f2")],
    };
    const run = await setup();
    screen.getByRole("textbox").focus();
    act(() => vi.advanceTimersByTime(2000));
    await run.push(requested(question));
    await run.ready();
    await run.press("2");
    expect(screen.getByText("问题 f2")).toBeTruthy();
    await run.press("Enter");
    expect(run.responses()).toEqual([{ type: "answer", values: { f1: "b", f2: "a" } }]);
    cleanup();

    const unsupported = await setup({
      ops: [requested({ ...question, unsupported_schema: true })],
    });
    await unsupported.ready();
    await act(async () => void fireEvent.mouseDown(screen.getByRole("button", { name: /关闭/ })));
    await act(async () => void fireEvent.click(screen.getByRole("button", { name: /关闭/ })));
    expect(unsupported.responses()).toEqual([{ type: "dismissed" }]);
  });

  test("typed text answers its field by 提交 or by Enter, beside a multiple choice's ticks", async () => {
    const field = (type: string, choices?: object[]) => ({
      ...{ field_id: "f", label: "问题", type, required: true, allow_custom: true },
      ...(choices && { choices }),
    });
    const ask = (f: object) =>
      requested({ type: "question", prompt: "怎么做？", unsupported_schema: false, fields: [f] });
    const run = await setup({ ops: [ask(field("text"))] });
    await run.ready();
    fireEvent.change(screen.getByRole("textbox", { name: "在此输入" }), {
      target: { value: "用 pnpm" },
    });
    const submit = screen.getByRole("button", { name: /提交/ });
    fireEvent.mouseDown(submit);
    await act(async () => void fireEvent.click(submit));
    expect(run.responses()).toEqual([{ type: "answer", values: { f: "用 pnpm" } }]);
    cleanup();

    const choices = [
      { label: "甲", value: "a" },
      { label: "乙", value: "b" },
    ];
    const multi = await setup({ ops: [ask(field("multi_choice", choices))] });
    await multi.ready();
    const first = screen.getByRole("option", { name: /甲/ });
    fireEvent.mouseDown(first);
    await act(async () => void fireEvent.click(first));
    const other = screen.getByRole("textbox", { name: "在此输入" });
    fireEvent.change(other, { target: { value: "丙" } });
    await act(async () => void fireEvent.keyDown(other, { key: "Enter" }));
    expect(multi.responses()).toEqual([{ type: "answer", values: { f: ["a", "丙"] } }]);
  });

  test("a request answered by another client folds away with a toast", async () => {
    const run = await setup({ ops: [permission()] });
    const resolved_by = { client_kind: "cli", client_name: "cupper-cli", principal_id: "p" };
    await run.push({
      type: "interaction.resolved",
      data: { request_id: "r_1", status: "resolved", resolved_by },
    });
    expect(run.panel()).toBeNull();
    expect(screen.getByText("已在 cupper-cli 处理")).toBeTruthy();
    expect(screen.getByRole("textbox")).toBeTruthy();
  });
});

describe("UNIT-request-followup", () => {
  /** A permission on screen with the keyboard, and the text typed in its free row. */
  async function rejecting(respond?: () => object) {
    const run = await setup(respond ? { respond } : {});
    screen.getByRole("textbox").focus();
    act(() => vi.advanceTimersByTime(2000));
    await run.push(permission());
    await run.ready();
    await run.press("5");
    const row = screen.getByRole("textbox", { name: "否，并告诉 Claude 应该怎么做" });
    expect(document.activeElement).toBe(row);
    fireEvent.change(row, { target: { value: "先别跑测试" } });
    act(() => vi.advanceTimersByTime(1000));
    return { ...run, row };
  }
  const settle = () =>
    act(async () => {
      for (let tick = 0; tick < 20; tick++) await Promise.resolve();
    });

  test("the free row is number 5; the rejection goes first, then the follow-up with reply_to", async () => {
    const run = await rejecting();
    await run.press("Enter");
    await settle();
    expect(run.methods()).toEqual(["interaction/respond", "session/send"]);
    expect(run.responses()).toEqual([{ type: "selected", option_id: "o4" }]);
    expect(run.sent()).toEqual([
      expect.objectContaining({
        mode: "queue",
        reply_to: "r_1",
        content: [{ type: "text", text: "先别跑测试" }],
      }),
    ]);
  });

  test("answered elsewhere: no follow-up, the text goes back to the composer", async () => {
    const run = await rejecting(() => hostError("conflict", "already_resolved"));
    await run.press("Enter");
    await settle();
    expect(run.sent()).toEqual([]);
    expect(run.ui.getState().drafts.s_1).toBe("先别跑测试");
    expect(screen.getByText("已在别处处理")).toBeTruthy();
  });

  test("answered elsewhere with a draft in the composer: the draft stays, the text follows it", async () => {
    const run = await setup({ respond: () => hostError("conflict", "already_resolved") });
    act(() => run.ui.getState().setDraft("s_1", "原来的草稿"));
    await run.push(permission());
    await run.ready();
    const row = screen.getByRole("textbox", { name: "否，并告诉 Claude 应该怎么做" });
    fireEvent.mouseDown(row);
    row.focus();
    fireEvent.change(row, { target: { value: "先别跑测试" } });
    act(() => vi.advanceTimersByTime(1000));
    await run.press("Enter");
    await settle();
    expect(run.sent()).toEqual([]);
    expect(run.ui.getState().drafts.s_1).toBe("原来的草稿\n\n先别跑测试");
  });

  test("an Enter right after typing only says to click; the click submits", async () => {
    const run = await rejecting();
    fireEvent.keyDown(run.row, { key: "x" });
    await run.press("Enter");
    expect(screen.getByText("点击提交")).toBeTruthy();
    expect(run.methods()).toEqual([]);
    const submit = screen.getByRole("button", { name: /提交/ });
    fireEvent.mouseDown(submit);
    await act(async () => void fireEvent.click(submit));
    await settle();
    expect(run.methods()).toEqual(["interaction/respond", "session/send"]);
  });

  test("several requests show their count; the record line follows the outcome", async () => {
    const run = await setup();
    await run.push(permission(), {
      ...permission(),
      data: { ...permission().data, request_id: "r_2" },
    });
    expect(screen.getByText("1 / 2")).toBeTruthy();
    const view = (outcome?: object, status = "resolved") => ({
      requested: {
        ...(permission().data as object),
        request: { type: "permission", title: "Run tests", options: OPTIONS },
      },
      ...(outcome || status !== "resolved"
        ? { resolved: { request_id: "r_1", status, outcome } }
        : {}),
    });
    expect(record(view() as never)).toEqual(["request:record_pending"]);
    expect(record(view({ type: "selected", option_id: "o1" }) as never)).toEqual([
      "request:record_approved",
      { title: "Run tests" },
    ]);
    expect(record(view({ type: "selected", option_id: "o4" }) as never)).toEqual([
      "request:record_rejected",
    ]);
    expect(record(view({ type: "dismissed" }) as never)).toEqual(["request:record_unanswered"]);
    expect(record(view(undefined, "expired") as never)).toEqual(["request:record_unanswered"]);
    const asked = {
      requested: { request: { type: "question", fields: [{}, {}] } },
      resolved: { status: "resolved", outcome: { type: "answer", values: {} } },
    };
    expect(record(asked as never)).toEqual(["request:record_asked", { count: 2 }]);
  });
});

describe("UNIT-blob-fulltext: the request panel", () => {
  const FULL = `rm -rf build${String.fromCodePoint(0x202e)}`;
  const truncated = (blob: object) =>
    requested({
      ...{ type: "permission", title: "Run", content_truncated: true, options: OPTIONS },
      tool: { tool_kind: "execute", paths: [], command: { type: "blob", blob, preview: "rm -rf" } },
    });
  /** Whether "允许一次" is off. */
  const allowOff = () => screen.getAllByRole("option")[0]?.getAttribute("aria-disabled") === "true";
  const until = async (done: () => boolean) => {
    for (let n = 0; n < 20 && !done(); n++) await frame();
  };

  test("allowing waits for the full text, escaped; collapsing takes it back", async () => {
    const run = await setup();
    await run.push(truncated(await serveBlob(run.mock, FULL)));
    expect(allowOff()).toBe(true);
    expect(screen.getByText("rm -rf")).toBeTruthy();
    expect(run.methods()).not.toContain("blob/get");
    fireEvent.click(screen.getByRole("button", { name: "展开" }));
    await until(() => !allowOff());
    // Shown in a box that scrolls; allowing needs no scrolling to its end.
    expect(screen.getByText("rm -rf build\\u{202E}").className).toContain("overflow-y-auto");
    expect(screen.queryByText(/已截断/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "收起" }));
    expect(allowOff()).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "展开" }));
    expect(allowOff()).toBe(false);
    expect(run.methods().filter((method) => method === "blob/get")).toHaveLength(1);
  });

  test("a failed download or other bytes keep only rejecting, with a retry", async () => {
    const run = await setup();
    const blob = await serveBlob(run.mock, FULL, new TextEncoder().encode("rm -rf /"));
    await run.push(truncated(blob));
    fireEvent.click(screen.getByRole("button", { name: "展开" }));
    await until(() => screen.queryByText("全文没有载入") !== null);
    expect(allowOff()).toBe(true);
    expect(screen.queryByText("rm -rf /")).toBeNull();
    await serveBlob(run.mock, FULL);
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    await until(() => !allowOff());
    expect(screen.getByText("rm -rf build\\u{202E}")).toBeTruthy();
  });
});
