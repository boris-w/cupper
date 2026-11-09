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
import { installResizeObserver, observed } from "./test-resize";

afterEach(cleanup);

// jsdom lays nothing out: the list is 800 px high, a row 40.
beforeAll(() => {
  installResizeObserver();
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
      params: {
        stream,
        at_seq: 1,
        epoch: 1,
        page: 0,
        last: true,
        ops: stream === "index" ? [] : ops,
      },
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
    // The user's wheel at 0 (T245).
    keeper.intend(0);
    expect(keeper.scrolled(776, 1000, 200, 0)).toBe(true);
    expect(keeper.scrolled(775, 1000, 200, 0)).toBe(false);
  });

  test("a list not where it was put was scrolled, whether or not its event came", () => {
    const keeper = new ScrollKeeper();
    keeper.put(800);
    expect(keeper.follows(800, 1400, 200, 0), "grown, not moved").toBe(true);
    keeper.intend(0);
    expect(keeper.follows(0, 1400, 200, 0), "moved to the top by the user").toBe(false);
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

  test("a scroll whose event has not come yet is not undone by a render before it (T208)", async () => {
    await setup();
    const list = document.querySelector("[data-stream-list]") as HTMLElement;
    Object.defineProperties(list, { scrollHeight: { value: 1000 }, clientHeight: { value: 200 } });
    list.scrollTop = 800;
    fireEvent.scroll(list);
    // Scrolled to the top between frames: the virtualizer's end of scrolling,
    // 150 ms after the last scroll event, reads it and renders first. The
    // user's wheel came first, as it always does (T245).
    fireEvent.wheel(list);
    list.scrollTop = 0;
    await act(() => new Promise((done) => setTimeout(done, 300)));
    expect(list.scrollTop).toBe(0);
  });

  test("at the bottom it follows what comes in", async () => {
    await setup();
    const list = document.querySelector("[data-stream-list]") as HTMLElement;
    let height = 1000;
    Object.defineProperties(list, {
      scrollHeight: { get: () => height },
      clientHeight: { value: 200 },
    });
    list.scrollTop = 800;
    fireEvent.scroll(list);
    height = 1400;
    await act(() => new Promise((done) => setTimeout(done, 300)));
    expect(list.scrollTop).toBe(1400);
  });

  test("scrolled up, a button goes back to the bottom", async () => {
    await setup();
    const list = document.querySelector("[data-stream-list]") as HTMLElement;
    Object.defineProperties(list, { scrollHeight: { value: 1000 }, clientHeight: { value: 200 } });
    fireEvent.wheel(list);
    list.scrollTop = 100;
    fireEvent.scroll(list);
    const down = screen.getByRole("button", { name: "回到底部" });
    fireEvent.click(down);
    expect(screen.queryByRole("button", { name: "回到底部" })).toBeNull();
  });
});

describe("UNIT-stream-follow", () => {
  test("a scroll no user made, as the browser's scroll anchoring makes, keeps it at the bottom", async () => {
    await setup();
    const list = document.querySelector("[data-stream-list]") as HTMLElement;
    let height = 1000;
    Object.defineProperties(list, {
      scrollHeight: { get: () => height },
      clientHeight: { value: 200 },
    });
    list.scrollTop = 800;
    fireEvent.scroll(list);
    // Rows re-wrap in a narrower list: they grow, and the browser moves the
    // scroll top to keep a row in place, with no wheel, key or touch.
    height = 1400;
    list.scrollTop = 850;
    fireEvent.scroll(list);
    await act(() => new Promise((done) => setTimeout(done, 300)));
    expect(list.scrollTop).toBe(1400);
  });

  test("only the user leaves the bottom: within 500 ms of a wheel, touch or key, or holding the scrollbar", () => {
    const keeper = new ScrollKeeper();
    expect(keeper.scrolled(500, 1000, 200, 1000), "no user").toBe(true);
    keeper.intend(1000);
    expect(keeper.scrolled(500, 1000, 200, 1500), "500 ms after the wheel").toBe(false);
    expect(keeper.scrolled(800, 1000, 200, 9000), "back at the bottom, by anything").toBe(true);
    expect(keeper.scrolled(500, 1000, 200, 9000), "a wheel long ago").toBe(true);
    keeper.intend(9000, true);
    expect(keeper.scrolled(500, 1000, 200, 60000), "the scrollbar held").toBe(false);
    keeper.scrolled(800, 1000, 200, 60000);
    keeper.letGo(60000);
    expect(keeper.scrolled(500, 1000, 200, 60600), "let go 600 ms ago").toBe(true);
    keeper.leave();
    expect(keeper.follows(500, 1400, 200, 60600), "the app's jump").toBe(false);
  });

  test("keys and the scrollbar leave the bottom; a press on a row does not", async () => {
    await setup();
    const list = document.querySelector("[data-stream-list]") as HTMLElement;
    Object.defineProperties(list, { scrollHeight: { value: 1000 }, clientHeight: { value: 200 } });
    const away = (before: () => void) => {
      list.scrollTop = 800;
      fireEvent.scroll(list);
      before();
      list.scrollTop = 100;
      fireEvent.scroll(list);
      return screen.queryByRole("button", { name: "回到底部" }) !== null;
    };
    expect(
      away(() => fireEvent.pointerDown(screen.getByText("第二个"))),
      "a row",
    ).toBe(false);
    expect(
      away(() => fireEvent.keyDown(list, { key: "PageUp" })),
      "a key",
    ).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "回到底部" }));
    expect(
      away(() => fireEvent.pointerDown(list)),
      "the scrollbar",
    ).toBe(true);
  });

  test("at the bottom, a list of another size goes back to the bottom", async () => {
    await setup();
    const list = document.querySelector("[data-stream-list]") as HTMLElement;
    let height = 1000;
    Object.defineProperties(list, {
      scrollHeight: { get: () => height },
      clientHeight: { value: 200 },
    });
    list.scrollTop = 800;
    fireEvent.scroll(list);
    height = 1600;
    act(() => observed.filter((o) => o.target === list).forEach((o) => o.resize()));
    expect(list.scrollTop).toBe(1600);
  });
});
