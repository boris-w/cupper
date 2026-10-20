// @vitest-environment jsdom
import { HostClient } from "@cupper/host-client";
import { MockHost } from "@cupper/host-client/mock";
import type { Handshake } from "@cupper/shell-api";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, test } from "vitest";

import { HostContext, I18nProvider, ShellProvider } from "../app/providers";
import { DEFAULT_PREFS } from "../layout/layout";
import { StateProvider, UiProvider } from "../state/provider";
import { createUiStore } from "../state/ui";
import { ScrollKeeper } from "./scroll";
import { StreamList } from "./StreamList";

afterEach(cleanup);

// jsdom lays nothing out: the list is 800 px high, a row 40.
beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return this.dataset.streamList === undefined ? 40 : 800;
    },
  });
});

/** Turn `n` and its message `text`, as event bodies. */
const turn = (n: number, text: string) => [
  { type: "turn.started", data: { turn_id: `t_${n}`, turn_no: n, started_at: n, origin: "user" } },
  {
    type: "item.started",
    data: {
      item_id: `i_${n}`,
      turn_id: `t_${n}`,
      started_at: n,
      kind: "user_message",
      content: [{ type: "text", text }],
      steered: false,
    },
  },
];

/** Session s_1 from turn 2 on; its history page brings turn 1. */
async function setup() {
  const mock = new MockHost([{ method: "initialize", params: {}, result: {} }]);
  mock.handle("stream/subscribe", ({ id, conn, params }) => {
    const { stream } = params as { stream: string };
    conn.reply(id, { result: { stream, mode: "snapshot", at_seq: 1, head_seq: 1, epoch: 1 } });
    const ops = stream === "index" ? [] : [...turn(2, "第二个"), ...turn(3, "第三个")];
    conn.send({
      jsonrpc: "2.0",
      method: "stream/snapshot",
      params: { stream, at_seq: 1, epoch: 1, page: 0, last: true, ops },
    });
    return undefined;
  });
  mock.handle("history/page", () => ({ result: { ops: turn(1, "第一个") } }));
  const client = new HostClient();
  const conn = mock.connect();
  conn.initialized = true;
  client.attach(conn.port(), {} as Handshake);
  const { shell } = createBrowserShell();
  const ui = createUiStore(
    { prefs: DEFAULT_PREFS, overlayOpen: false },
    { page: "session", sessionId: "s_1" },
  );
  render(
    <ShellProvider api={shell} info={await shell.init()}>
      <I18nProvider locale="zh-CN">
        <HostContext value={{ client, state: client.state, handshake: undefined }}>
          <UiProvider ui={ui}>
            <StateProvider>
              <StreamList sessionId="s_1" />
            </StateProvider>
          </UiProvider>
        </HostContext>
      </I18nProvider>
    </ShellProvider>,
  );
  const frame = () =>
    act(() => new Promise((done) => requestAnimationFrame(() => done(undefined))));
  await frame();
  const pages = () => mock.requests.filter((r) => r.method === "history/page").map((r) => r.params);
  return { frame, pages };
}

describe("UNIT-stream-anchor", () => {
  test("held rows go back where they were once rows went in front, not before", () => {
    const keeper = new ScrollKeeper();
    keeper.hold("turn:t_2", 1, 300, 280);
    expect(
      keeper.restore(() => ({ index: 1, start: 300 })),
      "nothing in front yet",
    ).toBeUndefined();
    expect(keeper.restore(() => ({ index: 4, start: 950 }))).toBe(930);
    expect(
      keeper.restore(() => ({ index: 4, start: 950 })),
      "once",
    ).toBeUndefined();
  });

  test("at the bottom within 24 px it follows; above, it does not", () => {
    const keeper = new ScrollKeeper();
    expect(keeper.scrolled(776, 1000, 200)).toBe(true);
    expect(keeper.scrolled(775, 1000, 200)).toBe(false);
  });

  test("earlier history comes in front on request, from the first turn's cursor", async () => {
    const { frame, pages } = await setup();
    expect(screen.getByText("第二个")).toBeTruthy();
    await act(
      async () => void fireEvent.click(screen.getByRole("button", { name: "加载更早的历史" })),
    );
    await frame();
    expect(pages()).toEqual([{ session_id: "s_1", cursor: { turn_no: 2 } }]);
    expect(screen.getByText("第一个")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "加载更早的历史" }), "turn 1 is in").toBeNull();
  });

  test("scrolled up, a button goes back to the bottom", async () => {
    await setup();
    const list = document.querySelector("[data-stream-list]") as HTMLElement;
    Object.defineProperties(list, { scrollHeight: { value: 1000 }, clientHeight: { value: 200 } });
    list.scrollTop = 100;
    fireEvent.scroll(list);
    const down = screen.getByRole("button", { name: "回到底部" });
    fireEvent.click(down);
    expect(screen.queryByRole("button", { name: "回到底部" })).toBeNull();
  });
});
