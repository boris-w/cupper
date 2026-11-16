// The shortcuts of design 5.7.7 that move between sessions or act on the one
// on screen (T153): the menu sends the command, and these decide what it
// does. Rename, archive and "在终端中打开" are T265's, with its session actions.

import type { SessionSummary } from "@cupper/protocol";
import { commandHandler, type ShellCommand } from "@cupper/shell-api";
import { useEffect, useEffectEvent } from "react";
import { useStore } from "zustand";

import { useShell } from "../app/providers";
import { useSend } from "../composer/send";
import { nextNeedsAction, sidebarOrder, sidebarStep } from "../sidebar/order";
import { useStreams, useUi } from "../state/provider";

export function SessionShortcuts(): null {
  const { api } = useShell();
  const ui = useUi();
  const { view } = useStreams().index;
  const current = useStore(ui, ({ route }) =>
    route.page === "session" ? route.sessionId : undefined,
  );
  const { interrupt } = useSend(current, undefined);

  const index = () => view.toJSON();
  const order = () => {
    const { projects, sessions } = index();
    return sidebarOrder(Object.values(projects), Object.values(sessions), ui.getState().expanded);
  };
  const go = (session: SessionSummary | undefined) =>
    session && ui.getState().navigate({ page: "session", sessionId: session.session_id });
  const live = (sessionId: string) => Object.hasOwn(index().sessions, sessionId);
  // An effect event: the commands see the session on screen when they come.
  const onCommand = useEffectEvent(
    commandHandler({
      interrupt: () => {
        const status = current === undefined ? undefined : index().sessions[current]?.status;
        if (status === "running" || status === "needs_action") interrupt();
      },
      next_needs_action: () => {
        const next = nextNeedsAction(order(), current);
        // A session behind "展开显示" opens its project, so that its row shows.
        if (next && !next.shown) ui.getState().expand(next.session.project_id, true);
        go(next?.session);
      },
      prev_sidebar_item: () => go(sidebarStep(order(), current, -1)),
      next_sidebar_item: () => go(sidebarStep(order(), current, 1)),
      session_n: (n = 0) => go(order().filter((entry) => entry.shown)[n - 1]?.session),
      prev_recent: () => ui.getState().cycleRecent(-1, live),
      next_recent: () => ui.getState().cycleRecent(1, live),
      model_picker: () => {
        const { route, menu, openMenu } = ui.getState();
        if (route.page === "session") openMenu(menu === "model" ? null : "model");
      },
    }),
  );
  useEffect(() => api.onCommand((cmd: ShellCommand) => onCommand(cmd)), [api]);

  // ⌃Tab's walk ends when ⌃ comes up, at a click, or when the window loses the
  // focus, as Codex's "切换聊天…" does.
  useEffect(() => {
    const end = () => ui.getState().endCycle();
    const onKeyUp = (event: KeyboardEvent) => event.key === "Control" && end();
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("pointerdown", end, true);
    window.addEventListener("blur", end);
    return () => {
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("pointerdown", end, true);
      window.removeEventListener("blur", end);
    };
  }, [ui]);
  return null;
}
