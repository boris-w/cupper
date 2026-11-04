// @vitest-environment jsdom
import { decodeBlobChunk, FRAME_BLOB, HostClient } from "@cupper/host-client";
import { hostError, MockHost } from "@cupper/host-client/mock";
import type { Handshake } from "@cupper/shell-api";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test } from "vitest";

import { HostContext, I18nProvider, ShellProvider } from "../app/providers";
import { RouterProvider } from "../app/router";
import { ToasterProvider } from "../feedback/toaster";
import { DEFAULT_PREFS } from "../layout/layout";
import { StateProvider, UiProvider } from "../state/provider";
import { createUiStore } from "../state/ui";
import { escapeAction, sends } from "./keys";
import { ComposerArea } from "./QueueList";

afterEach(cleanup);

const summary = (status: string) => ({
  type: "session.upserted",
  data: {
    ...{ session_id: "s_1", title: "t", status, agent: "claude", project_id: "p_1" },
    ...{ title_source: "prompt", last_activity_at: 1, pending_interactions: 0 },
    ...{ read_only: false, unread: false, workspace_kind: "local" },
  },
});

/** A composer on session s_1, its host a MockHost that answers every command. */
async function setup(status = "idle", session: object[] = []) {
  const mock = new MockHost([{ method: "initialize", params: {}, result: {} }]);
  mock.handle("stream/subscribe", ({ id, conn, params }) => {
    const { stream } = params as { stream: string };
    conn.reply(id, { result: { stream, mode: "snapshot", at_seq: 1, head_seq: 1, epoch: 1 } });
    const ops = stream === "index" ? [summary(status)] : session;
    const page = { stream, at_seq: 1, epoch: 1, page: 0, last: true, ops };
    conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: page });
    return undefined;
  });
  const methods = [
    "session/send",
    "session/interrupt",
    "session/queue_resume",
    "session/set_config",
  ];
  for (const method of methods) {
    mock.handle(method, () => ({ result: { disposition: "queued" } }));
  }
  const client = new HostClient();
  const conn = mock.connect();
  conn.initialized = true;
  client.attach(conn.port(), {} as Handshake);
  const { shell, controls } = createBrowserShell();
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
  // The snapshot arrives in a microtask and the views flush in an animation frame.
  await act(() => new Promise((done) => requestAnimationFrame(() => done(undefined))));
  const area = screen.getByRole("textbox");
  const requests = (method: string) =>
    mock.requests.filter((request) => request.method === method).map((request) => request.params);
  return { ui, area, requests, mock, conn, controls };
}

const type = (area: HTMLElement, text: string) =>
  fireEvent.change(area, { target: { value: text } });

describe("UNIT-composer-ime", () => {
  test("Enter sends unless an input method composes; Shift and Option keep the newline", () => {
    expect(sends({ key: "Enter" }, "enter")).toBe(true);
    expect(sends({ key: "Enter", isComposing: true }, "enter")).toBe(false);
    expect(sends({ key: "Enter", keyCode: 229 }, "enter")).toBe(false);
    expect(sends({ key: "Enter", shiftKey: true }, "enter")).toBe(false);
    expect(sends({ key: "Enter", altKey: true }, "enter")).toBe(false);
    expect(sends({ key: "Enter", metaKey: true }, "enter")).toBe(true);
    expect(sends({ key: "Enter" }, "mod_enter")).toBe(false);
    expect(sends({ key: "Enter", metaKey: true, shiftKey: true }, "mod_enter")).toBe(true);
    expect(sends({ key: "Enter", metaKey: true, isComposing: true }, "mod_enter")).toBe(false);
  });

  test("the Enter that picks a candidate stays in the composer; the next one sends", async () => {
    const { area, requests, ui } = await setup();
    type(area, "你好");
    fireEvent.keyDown(area, { key: "Enter", isComposing: true });
    fireEvent.keyDown(area, { key: "Enter", keyCode: 229 });
    expect(requests("session/send")).toEqual([]);
    await act(async () => void fireEvent.keyDown(area, { key: "Enter" }));
    expect(requests("session/send")).toEqual([
      expect.objectContaining({
        session_id: "s_1",
        mode: "queue",
        content: [{ type: "text", text: "你好" }],
      }),
    ]);
    expect(ui.getState().drafts.s_1).toBe("");
  });
});

describe("UNIT-composer-esc", () => {
  test("the order: overlay first, then arm, then interrupt; outside, focus", () => {
    const at = (state: Partial<Parameters<typeof escapeAction>[0]>) =>
      escapeAction({ focused: true, overlayOpen: false, running: false, armed: false, ...state });
    expect(at({ focused: false, running: true })).toBe("focus");
    expect(at({ overlayOpen: true, running: true, armed: true })).toBe("close_overlay");
    expect(at({ running: true })).toBe("arm");
    expect(at({ running: true, armed: true })).toBe("interrupt");
    expect(at({})).toBe("none");
  });

  test("a running turn stops on the second Esc; another key or a blur disarms", async () => {
    const { area, requests } = await setup("running");
    const send = screen.getByRole("button", { name: "停止" });
    expect(send.dataset.sendState).toBe("stop");
    fireEvent.keyDown(area, { key: "Escape" });
    expect(screen.getByText("再按一次 Esc 停止")).toBeTruthy();
    fireEvent.keyDown(area, { key: "a" });
    expect(screen.queryByText("再按一次 Esc 停止")).toBeNull();
    fireEvent.keyDown(area, { key: "Escape" });
    fireEvent.blur(area);
    fireEvent.keyDown(area, { key: "Escape" });
    expect(requests("session/interrupt")).toEqual([]);
    await act(async () => void fireEvent.keyDown(area, { key: "Escape" }));
    expect(requests("session/interrupt")).toEqual([expect.objectContaining({ session_id: "s_1" })]);
  });

  test("Esc from outside the composer brings the focus back; an idle turn ignores Esc", async () => {
    const { area, requests } = await setup();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(document.activeElement).toBe(area);
    fireEvent.keyDown(area, { key: "Escape" });
    fireEvent.keyDown(area, { key: "Escape" });
    expect(screen.queryByText("再按一次 Esc 停止")).toBeNull();
    expect(requests("session/interrupt")).toEqual([]);
  });
});

describe("the attachment strip", () => {
  test("shows each attachment, removes one, and sends the rest with the text", async () => {
    const { area, requests, ui } = await setup();
    const comment = {
      ...{ type: "review_comment", path: "src/a.ts", line_start: 3, line_end: 3 },
      ...{ side: "new", quote: "x", comment: "改名" },
    } as const;
    act(() =>
      ui.getState().setAttachments("s_1", [{ type: "file_ref", path: "README.md" }, comment]),
    );
    expect(screen.getByText("README.md")).toBeTruthy();
    // Review comments go together, and open to each one's file, line and text.
    fireEvent.click(screen.getByRole("button", { name: "1 条评论" }));
    expect(screen.getByText("src/a.ts:3")).toBeTruthy();
    expect(screen.getByText("改名")).toBeTruthy();
    fireEvent.click(screen.getAllByRole("button", { name: "移除" })[0] as HTMLElement);
    type(area, "看一下");
    await act(async () => void fireEvent.keyDown(area, { key: "Enter" }));
    expect(requests("session/send")).toEqual([
      expect.objectContaining({ content: [{ type: "text", text: "看一下" }, comment] }),
    ]);
    expect(ui.getState().attachments.s_1).toEqual([]);
  });
});

describe("sending while the queue is paused", () => {
  test("asks first: send resumes the queue after the message, cancel keeps the draft", async () => {
    const paused = {
      type: "queue.updated",
      data: { version: 1, entries: [], paused: { reason: "turn_interrupted" } },
    };
    const { area, requests, ui } = await setup("idle", [paused]);
    type(area, "接着做");
    fireEvent.keyDown(area, { key: "Enter" });
    expect(screen.getByText("队列已暂停。发出这条消息，并让队列继续吗？")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(requests("session/send")).toEqual([]);
    expect(ui.getState().drafts.s_1).toBe("接着做");
    fireEvent.keyDown(area, { key: "Enter" });
    await act(
      async () =>
        void fireEvent.click(screen.getAllByRole("button", { name: "发送" })[0] as HTMLElement),
    );
    expect(requests("session/send")).toHaveLength(1);
    expect(requests("session/queue_resume")).toEqual([
      expect.objectContaining({ session_id: "s_1" }),
    ]);
  });
});

describe("UNIT-composer-long-text", () => {
  // 90,000 bytes of UTF-8: past one frame's 64,000.
  const long = "长".repeat(30_000);

  test("text past the frame budget goes up as a blob and out as a text_blob", async () => {
    const { area, requests, mock, conn } = await setup();
    mock.handle("blob/put", () => ({ result: {} }));
    type(area, long);
    fireEvent.keyDown(area, { key: "Enter" });
    await waitFor(() => expect(requests("session/send")).toHaveLength(1));
    const [put] = requests("blob/put") as { hash: string }[];
    expect(put).toEqual({ hash: expect.any(String), size: 90_000, media_type: "text/plain" });
    const chunks = conn.frames
      .filter((f) => f.type === FRAME_BLOB)
      .map((f) => decodeBlobChunk(f.payload));
    expect(chunks.map((c) => [c.hash, c.last])).toEqual([
      [put?.hash, false],
      [put?.hash, true],
    ]);
    expect(requests("session/send")).toEqual([
      expect.objectContaining({ content: [{ type: "text_blob", blob: put }] }),
    ]);
    // What fits goes as text, with no upload.
    type(area, "短的");
    await act(async () => void fireEvent.keyDown(area, { key: "Enter" }));
    expect(requests("session/send")[1]).toMatchObject({
      content: [{ type: "text", text: "短的" }],
    });
    expect(requests("blob/put")).toHaveLength(1);
  });

  test("an upload that fails puts the draft back and says why", async () => {
    const { area, requests, mock, ui } = await setup();
    mock.handle("blob/put", () => hostError("too_large"));
    type(area, long);
    fireEvent.keyDown(area, { key: "Enter" });
    await waitFor(() => expect(screen.getByText("消息没有发出（too_large）")).toBeTruthy());
    expect(requests("session/send")).toEqual([]);
    expect(ui.getState().drafts.s_1).toBe(long);
  });
});

describe("UNIT-composer-config", () => {
  const value = (value: string, name: string, risk = "normal") => ({ value, name, risk });
  const option = (id: string, current: string, values: object[]) => ({
    id,
    agent_option_id: id,
    current,
    values,
  });
  const caps = {
    type: "session.capabilities",
    data: {
      ...{
        agent: "claude",
        commands: [],
        session: { resume: true, load_history: false, fork: false },
      },
      versions: { adapter: "1", cli: "1", cli_compat: "verified" },
      prompt: { image: true, file_ref: true, max_image_bytes: 1 },
      turn: {
        ...{ queue: true, steer: { supported: false, side_effects: false } },
        ...{ interrupt: true, interrupt_stops_commands: true },
      },
      interactions: { reject_ends_turn: false, question_forms: true, plan_confirm: true },
      extraction: {
        ...{ exit_code: true, output_streaming: false, diff: "per_file", thinking: true },
        ...{ plan: "markdown_only", run_state: "inferred" },
      },
      config: [
        option("permission_mode", "default", [
          value("default", "Default"),
          value("acceptEdits", "Accept edits", "elevated"),
          value("bypassPermissions", "Bypass", "full_access"),
        ]),
        option("model", "opus", [value("opus", "Opus"), value("sonnet", "Sonnet")]),
        option("reasoning_effort", "high", [value("high", "High"), value("max", "Max")]),
      ],
    },
  };
  const configured = (mode: string) => ({
    type: "session.configured",
    data: {
      ...{ agent: "claude", project_config: "loaded", read_only: false },
      workspace: { kind: "local", path: "/p" },
      config: { permission_mode: mode, model: "opus", reasoning_effort: "high" },
    },
  });
  const open = async (name: string) => {
    screen.getByRole("button", { name }).focus();
    await userEvent.keyboard("{Enter}");
  };
  const pick = (name: RegExp) =>
    act(async () => void fireEvent.click(screen.getByRole("menuitemradio", { name })));

  test("the permission menu sets a mode; full access goes through the native confirmation", async () => {
    const { requests, controls } = await setup("idle", [caps, configured("default")]);
    expect(screen.getByRole("button", { name: "权限" }).textContent).toBe("Default");
    await open("权限");
    expect(screen.getByText("agent 的操作要怎样批准？")).toBeTruthy();
    expect(screen.getByText("由 agent 判断哪些操作需要你批准")).toBeTruthy();
    expect(screen.getByRole("menuitemradio", { checked: true }).textContent).toMatch(/^Default/);
    await pick(/Accept edits/);
    await open("权限");
    await pick(/Bypass/);
    const set = { session_id: "s_1", option: "permission_mode" };
    expect(requests("session/set_config")).toEqual([
      expect.objectContaining({ ...set, value: "acceptEdits" }),
    ]);
    expect(controls.privilegedActions).toEqual([
      { kind: "set_config", ...set, value: "bypassPermissions" },
    ]);
  });

  test("model and effort in one button; a refusal says why", async () => {
    const { requests, mock, ui } = await setup("idle", [caps, configured("default")]);
    expect(screen.getByRole("button", { name: "模型与推理强度" }).textContent).toBe("Opus High");
    await open("模型与推理强度");
    expect(screen.getByText("选择推理强度")).toBeTruthy();
    await pick(/Sonnet/);
    // "换模型" in an error opens the same menu.
    act(() => ui.getState().openMenu("model"));
    await pick(/Max/);
    expect(requests("session/set_config")).toEqual([
      expect.objectContaining({ option: "model", value: "sonnet" }),
      expect.objectContaining({ option: "reasoning_effort", value: "max" }),
    ]);
    mock.handle("session/set_config", () => hostError("agent_error"));
    await open("模型与推理强度");
    await pick(/Sonnet/);
    await waitFor(() => expect(screen.getByText("没有改成（agent_error）")).toBeTruthy());
  });

  test("above the composer: full access is on, or the low-interruption mode", async () => {
    await setup("idle", [caps, configured("bypassPermissions")]);
    expect(screen.getByText("完全访问权限已开启")).toBeTruthy();
    cleanup();
    await setup("idle", [caps, configured("acceptEdits")]);
    expect(screen.getByRole("status").textContent).toContain("低打扰模式已开启");
    cleanup();
    await setup("idle", [configured("default")]);
    expect(screen.queryByRole("button", { name: "权限" })).toBeNull();
  });
});

describe("file references", () => {
  const configured = {
    type: "session.configured",
    data: {
      ...{ agent: "claude", project_config: "loaded", read_only: false, config: {} },
      workspace: { kind: "local", path: "/p" },
    },
  };
  const results = [
    { path: "src/review/a.ts", score: 9, ranges_utf16: [[4, 7]] },
    { path: "docs/<b>rev</b>.md", score: 5, ranges_utf16: [[8, 11]] },
  ];

  test("@ lists the matches, ↓ and Enter take one as a file_ref, Esc closes", async () => {
    const { area, requests, mock, ui } = await setup("idle", [configured]);
    mock.handle("files/search", () => ({ result: { results } }));
    await act(async () => void fireEvent.input(area, { target: { value: "see @rev" } }));
    const list = await screen.findByRole("listbox", { name: "引用文件" });
    expect(requests("files/search")).toEqual([{ session_id: "s_1", query: "rev", limit: 20 }]);
    const options = within(list).getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(results.map((r) => r.path));
    expect(options[1]?.querySelector("span")?.textContent).toBe("rev");
    fireEvent.keyDown(area, { key: "ArrowDown" });
    expect(options[1]?.getAttribute("aria-selected")).toBe("true");
    await act(async () => void fireEvent.keyDown(area, { key: "Enter" }));
    expect(ui.getState().drafts.s_1).toBe("see ");
    expect(ui.getState().attachments.s_1).toEqual([{ type: "file_ref", path: results[1]?.path }]);
    expect(requests("session/send")).toEqual([]);
    expect(screen.queryByRole("listbox")).toBeNull();

    await act(async () => void fireEvent.input(area, { target: { value: "see @" } }));
    await screen.findByRole("listbox");
    await act(async () => void fireEvent.keyDown(area, { key: "Escape" }));
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  test("a dropped file in the workspace becomes a file_ref; one outside says so", async () => {
    const { area, controls, ui } = await setup("idle", [configured]);
    controls.setFilePath("a.ts", "/p/src/a.ts");
    controls.setFilePath("x.ts", "/elsewhere/x.ts");
    const files = [new File([], "a.ts"), new File([], "x.ts"), new File([], "made-up.ts")];
    const box = area.closest("[data-composer]") as HTMLElement;
    await act(async () => void fireEvent.drop(box, { dataTransfer: { files, types: ["Files"] } }));
    expect(ui.getState().attachments.s_1).toEqual([{ type: "file_ref", path: "src/a.ts" }]);
    expect(screen.getByText("只能引用工作区里的文件")).toBeTruthy();
  });
});
