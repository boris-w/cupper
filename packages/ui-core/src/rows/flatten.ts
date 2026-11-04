// The session view as the stream's rows (design 5.7.6): each turn's opening
// messages, its header, then the rest of its entries; the tail after the last
// turn; the events of types this build does not know at the end. A tool call
// starts an activity group with the tool calls right after it that share its
// `group_id`; a group of one is a plain tool row. A completed turn folds to
// its header unless open (design 5.7.4), and still shows its final answer and
// the rows of `ALWAYS`.

import type { Placeholder, SessionView } from "@cupper/host-client";
import type { Entry, ItemView, PromptBlock } from "@cupper/protocol";

import type { StreamRow, ToolData } from "./stream";

type ViewJson = ReturnType<SessionView["toJSON"]>;

/** Where the history before the window stands. */
export type History = "none" | "more" | "loading" | "failed";

/** What a folded turn still shows besides its final answer (design 5.7.4); T132's change card joins. */
export const ALWAYS: ReadonlySet<string> = new Set([
  ...["plan", "error", "notice", "late_tool_output", "request"],
]);

/** Steered messages show in a folded turn; the others open it, above its header. */
const opening = (row: StreamRow) => row.kind === "user_message" && !row.steered;

export function flatten(
  view: ViewJson,
  unknown: Placeholder[],
  history: History,
  open: ReadonlySet<string> = new Set(),
): StreamRow[] {
  const rows: StreamRow[] =
    history === "none" ? [] : [{ kind: "history", key: "history", state: history }];
  for (const [n, turn] of view.turns.entries()) {
    const { turn_id, turn_no, started_at } = turn.started;
    const completed = turn.completed ?? undefined;
    const folds = completed?.status === "completed";
    const folded = folds && !open.has(turn_id);
    const body: StreamRow[] = [];
    entries(view, turn.entries, body, n === view.turns.length - 1);
    const lead = body.findIndex((row) => !opening(row));
    const work = lead === -1 ? [] : body.splice(lead);
    const final = new Set(completed?.final_item_ids);
    const shown = (row: StreamRow) =>
      !folded ||
      ALWAYS.has(row.kind) ||
      row.kind === "user_message" ||
      ("itemId" in row && final.has(row.itemId));
    rows.push(...body, {
      kind: "turn_header",
      key: `turn:${turn_id}`,
      turnId: turn_id,
      turnNo: turn_no,
      status: completed?.status ?? "running",
      startedAt: started_at,
      ...(completed ? { durationMs: completed.duration_ms } : {}),
      ...(folds ? { folded } : {}),
    });
    rows.push(...work.filter(shown));
  }
  entries(view, view.tail, rows, false);
  unknown.forEach(({ body }, n) =>
    rows.push({ kind: "unknown", key: `unknown:${n}`, type: body.type, body }),
  );
  return rows;
}

function entries(view: ViewJson, list: Entry[], rows: StreamRow[], last: boolean): void {
  let group: { id: string; tools: ToolData[] } | undefined;
  let prompt: PromptBlock[] | undefined;
  const close = () => {
    const [first, ...rest] = group?.tools ?? [];
    if (group && first) {
      const running = group.tools.some((tool) => tool.running);
      rows.push(
        rest.length === 0
          ? { kind: "tool", key: `item:${first.itemId}`, ...first }
          : {
              kind: "activity_group",
              key: `group:${first.itemId}`,
              groupId: group.id,
              tools: group.tools,
              running,
            },
      );
    }
    group = undefined;
  };
  for (const entry of list) {
    if ("request" in entry) {
      close();
      const request = view.requests[entry.request];
      const key = `request:${entry.request}`;
      rows.push({
        kind: "request",
        key,
        requestId: entry.request,
        ...(request ? { request } : {}),
      });
      continue;
    }
    const item = view.items[entry.item];
    if (item?.item.kind === "tool_call") {
      const tool = toolData(view, item);
      const id = item.item.group_id ?? undefined;
      if (group === undefined || id === undefined || id !== group.id) {
        close();
      }
      if (id === undefined) {
        rows.push({ kind: "tool", key: `item:${tool.itemId}`, ...tool });
      } else {
        group ??= { id, tools: [] };
        group.tools.push(tool);
      }
      continue;
    }
    close();
    const row = item && itemRow(item);
    if (row?.kind === "user_message" && !row.steered) {
      prompt ??= row.content;
    }
    if (row?.kind === "error" && last && prompt) {
      row.retry = prompt;
    }
    if (row?.kind === "user_message" && row.replyTo) {
      const asked = view.requests[row.replyTo]?.requested.request as {
        title?: unknown;
        prompt?: unknown;
      };
      const title = asked?.title ?? asked?.prompt;
      if (typeof title === "string") row.replyTitle = title;
    }
    if (row) {
      rows.push(row);
    }
  }
  close();
}

type Started = ItemView["item"];
type Known = "user_message" | "tool_call" | "notice" | "error" | "plan" | "late_tool_output";
type Fields<K extends Known> = Omit<
  Extract<Started, { kind: K }>,
  "kind" | "item_id" | "group_id" | "started_at" | "turn_id"
>;

const COMMON = new Set(["kind", "item_id", "group_id", "started_at", "turn_id"]);

/** The fields of an item of kind `K` besides those every item has. */
function fields<K extends Known>(item: Started): Fields<K> {
  const rest = Object.entries(item).filter(([key]) => !COMMON.has(key));
  return Object.fromEntries(rest) as Fields<K>;
}

function toolData(view: ViewJson, { item, output, end }: ItemView): ToolData {
  const call = fields<"tool_call">(item);
  const lateId = view.late_by_ref[item.item_id];
  const late = lateId === undefined ? undefined : view.items[lateId]?.item;
  const code = (late as { exit_code?: number } | undefined)?.exit_code;
  return {
    ...{ itemId: item.item_id, call, output, running: !end, ...(end ? { end } : {}) },
    ...(lateId ? { late: { itemId: lateId, ...(code != null ? { exitCode: code } : {}) } } : {}),
  };
}

function itemRow({ item, text, output, end }: ItemView): StreamRow | undefined {
  const key = `item:${item.item_id}`;
  const itemId = item.item_id;
  switch (item.kind) {
    case "user_message": {
      const { content, steered, reply_to } = fields<"user_message">(item);
      return {
        kind: "user_message",
        key,
        itemId,
        content,
        steered,
        ...(reply_to ? { replyTo: reply_to } : {}),
      };
    }
    case "agent_message":
      return { kind: "agent_message", key, itemId, text, streaming: !end };
    case "thought": {
      const duration = end ? { durationMs: end.duration_ms } : {};
      return { kind: "thought", key, itemId, text, running: !end, ...duration };
    }
    case "notice":
      return { kind: "notice", key, itemId, notice: fields<"notice">(item) };
    case "error":
      return { kind: "error", key, itemId, error: fields<"error">(item) };
    case "plan":
      return { kind: "plan", key, itemId, plan: fields<"plan">(item), running: !end };
    case "late_tool_output":
      return {
        kind: "late_tool_output",
        key,
        itemId,
        output: fields<"late_tool_output">(item),
        // Its output streams as a tool call's does (`field: output`).
        text: output,
      };
    default:
      return { kind: "unknown", key, type: String(item.kind) };
  }
}
