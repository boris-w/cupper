// @vitest-environment jsdom
import { decodeBlobChunk, FRAME_BLOB, HostClient } from "@cupper/host-client";
import { hostError, MockHost } from "@cupper/host-client/mock";
import type { Handshake } from "@cupper/shell-api";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
  for (const method of ["session/send", "session/interrupt", "session/queue_resume"]) {
    mock.handle(method, () => ({ result: { disposition: "queued" } }));
  }
  const client = new HostClient();
  const conn = mock.connect();
  conn.initialized = true;
  client.attach(conn.port(), {} as Handshake);
  const { shell } = createBrowserShell();
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
  return { ui, area, requests, mock, conn };
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
    expect(screen.getByText("评论：src/a.ts 第 3 行")).toBeTruthy();
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
