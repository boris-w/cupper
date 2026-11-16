// The sidebar's order (design 5.7.4): the projects by position, each with its
// sessions most recent first, the first SHOWN_SESSIONS of them unless the
// project is expanded. The shortcuts that move between sessions walk it as the
// sidebar shows it (design 5.7.7, T153).

import type { ProjectUpserted, SessionSummary } from "@cupper/protocol";

/** Sessions a project shows before "展开显示". */
export const SHOWN_SESSIONS = 5;

export const byPosition = (a: ProjectUpserted, b: ProjectUpserted) => a.position - b.position;

export const byActivity = (a: SessionSummary, b: SessionSummary) =>
  b.last_activity_at - a.last_activity_at;

/** A session in the sidebar: whether its row shows, or waits behind "展开显示". */
export interface SidebarEntry {
  session: SessionSummary;
  shown: boolean;
}

/** Every session the sidebar lists, in its order. */
export function sidebarOrder(
  projects: readonly ProjectUpserted[],
  sessions: readonly SessionSummary[],
  expanded: Partial<Record<string, boolean>>,
): SidebarEntry[] {
  const recent = sessions.toSorted(byActivity);
  return projects.toSorted(byPosition).flatMap((project) =>
    recent
      .filter((session) => session.project_id === project.project_id)
      .map((session, index) => ({
        session,
        shown: index < SHOWN_SESSIONS || expanded[project.project_id] === true,
      })),
  );
}

/**
 * The row next to the current session among the shown ones, as Codex's
 * "下一个侧边栏项目": the first or the last when no shown row is current, and
 * nothing past either end.
 */
export function sidebarStep(
  order: readonly SidebarEntry[],
  current: string | undefined,
  direction: 1 | -1,
): SessionSummary | undefined {
  const shown = order.filter((entry) => entry.shown).map((entry) => entry.session);
  const at = shown.findIndex((session) => session.session_id === current);
  if (at < 0) {
    return direction > 0 ? shown[0] : shown.at(-1);
  }
  return shown[at + direction];
}

/**
 * The next session that needs the user, after the current one in the
 * sidebar's order and round to the start, rows behind "展开显示" too.
 */
export function nextNeedsAction(
  order: readonly SidebarEntry[],
  current: string | undefined,
): SidebarEntry | undefined {
  const at = order.findIndex((entry) => entry.session.session_id === current);
  const after = [...order.slice(at + 1), ...order.slice(0, at + 1)];
  return after.find(
    (entry) => entry.session.status === "needs_action" && entry.session.session_id !== current,
  );
}
