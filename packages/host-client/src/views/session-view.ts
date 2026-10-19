import type {
  Entry,
  InteractionRequested,
  ItemStarted,
  ItemView,
  KnownEventBody,
  SessionView as SessionViewJson,
  TurnStarted,
  TurnView,
} from "@cupper/protocol";

import { duplicate, missing, otherStream, StreamView } from "./view";

/** The fields a patch sets on each kind of item (design 5.3.11); an unknown kind takes all. */
const PATCHABLE: Record<string, readonly string[]> = {
  tool_call: [
    ...["tool_kind", "title", "paths", "output_state", "extraction", "query", "url", "command"],
    ...["cwd", "exit_code", "signal", "output", "output_tail", "output_bytes", "diffs"],
    "parent_item_id",
  ],
  late_tool_output: [
    "output_state",
    "output",
    "output_tail",
    "output_bytes",
    "exit_code",
    "signal",
  ],
  plan: ["source", "entries", "markdown"],
  notice: ["params"],
  user_message: [],
  agent_message: [],
  thought: [],
  error: [],
};

/** The fields of `item.updated` and `item.completed` that are not their patch. */
const NOT_PATCH = new Set(["item_id", "status", "ended_at", "duration_ms", "may_still_run"]);

/**
 * The session stream's view (design 5.3.3). Turns are ordered by `turn_no`,
 * so a history page inserts older turns in front; items and requests keep
 * their arrival order inside their turn. An entry without `turn_id` joins
 * the running turn, or the tail after the last turn, which the next new
 * turn takes in; either way it carries that turn's ID from then on. A
 * request whose turn is not in the view waits in `detached` until the
 * turn's history page puts it back in place.
 */
export class SessionView extends StreamView<SessionViewJson> {
  protected empty(): SessionViewJson {
    return { turns: [], tail: [], items: {}, requests: {}, detached: [], late_by_ref: {} };
  }

  protected fold(body: KnownEventBody): void {
    switch (body.type) {
      case "session.configured":
        return this.#set("configured", body.data);
      case "session.capabilities":
        return this.#set("capabilities", body.data);
      case "workspace.preparing":
        return this.#set("workspace", { ...body.data, state: "preparing" });
      case "workspace.ready":
        return this.#set("workspace", { ...body.data, state: "ready" });
      case "workspace.failed":
        return this.#set("workspace", { ...body.data, state: "failed" });
      case "queue.updated":
        return this.#set("queue", body.data);
      case "usage.updated":
        return this.#set("usage", body.data);
      case "turn.started":
        return this.#startTurn(body.data);
      case "turn.completed":
        this.#turn(body.data.turn_id).completed = body.data;
        return this.versions.touch(`turn:${body.data.turn_id}`);
      case "snapshot.recorded": {
        const turn = this.#turn(body.data.turn_id);
        turn.snapshots = turn.snapshots.filter((s) => s.phase !== body.data.phase);
        turn.snapshots.push(body.data);
        return this.versions.touch(`turn:${body.data.turn_id}`);
      }
      case "item.started":
        return this.#startItem(body.data);
      case "item.delta": {
        const { item_id, field, text } = body.data;
        const item = this.#item(item_id);
        // Text this version does not know where to show is dropped, as in Rust.
        if (field === "text") {
          item.text += text;
        } else if (field === "output") {
          item.output += text;
        }
        return this.versions.touch(`item:${item_id}`);
      }
      case "item.updated":
        patch(this.#item(body.data.item_id).item, body.data);
        return this.versions.touch(`item:${body.data.item_id}`);
      case "item.completed": {
        const { item_id, status, ended_at, duration_ms, may_still_run } = body.data;
        const item = this.#item(item_id);
        patch(item.item, body.data);
        item.end = { status, ended_at, duration_ms, ...(may_still_run ? { may_still_run } : {}) };
        return this.versions.touch(`item:${item_id}`);
      }
      case "interaction.requested":
        return this.#request(body.data);
      case "interaction.resolved": {
        const { request_id } = body.data;
        const request = Object.hasOwn(this.state.requests, request_id)
          ? this.state.requests[request_id]
          : undefined;
        if (request === undefined) {
          throw missing(request_id);
        }
        request.resolved = body.data;
        return this.versions.touch(`request:${request_id}`);
      }
      default:
        throw otherStream(body.type);
    }
  }

  #set<K extends "configured" | "capabilities" | "workspace" | "queue" | "usage">(
    key: K,
    value: SessionViewJson[K],
  ): void {
    this.state[key] = value;
    this.versions.touch("session");
  }

  /** The last turn with this ID; -1 when there is none. */
  #turnIndex(turnId: string): number {
    return this.state.turns.findLastIndex((t) => t.started.turn_id === turnId);
  }

  #turn(turnId: string): TurnView {
    const turn = this.state.turns[this.#turnIndex(turnId)];
    if (turn === undefined) {
      throw missing(turnId);
    }
    return turn;
  }

  #item(itemId: string): ItemView {
    const item = Object.hasOwn(this.state.items, itemId) ? this.state.items[itemId] : undefined;
    if (item === undefined) {
      throw missing(itemId);
    }
    return item;
  }

  /** Inserts a turn by `turn_no`; one newer than all others takes in the tail. */
  #startTurn(started: TurnStarted): void {
    const { turns } = this.state;
    const same = (t: TurnView) =>
      t.started.turn_id === started.turn_id || t.started.turn_no === started.turn_no;
    if (turns.some(same)) {
      throw duplicate(started.turn_id);
    }
    const later = turns.findIndex((t) => t.started.turn_no > started.turn_no);
    const at = later === -1 ? turns.length : later;
    let entries: Entry[] = [];
    if (at === turns.length && this.state.tail.length > 0) {
      entries = this.state.tail;
      this.state.tail = [];
      entries.forEach((entry) => this.#setTurn(entry, started.turn_id));
      this.versions.touch("tail");
    }
    turns.splice(at, 0, { started, snapshots: [], entries });
    this.versions.touch("turns", `turn:${started.turn_id}`);
  }

  #setTurn(entry: Entry, turnId: string): void {
    if ("item" in entry) {
      const item = this.state.items[entry.item];
      if (item) {
        item.item.turn_id = turnId;
        this.versions.touch(`item:${entry.item}`);
      }
    } else {
      const request = this.state.requests[entry.request];
      if (request) {
        request.requested.turn_id = turnId;
        this.versions.touch(`request:${entry.request}`);
      }
    }
  }

  /** Appends `entry` to its turn, or without one to the running turn or the tail; returns the turn. */
  #place(entry: Entry, turnId: string | undefined): string | undefined {
    const { turns } = this.state;
    const last = turns.at(-1);
    const turn = turnId === undefined ? (last?.completed ? undefined : last) : this.#turn(turnId);
    if (turn === undefined) {
      this.state.tail.push(entry);
      this.versions.touch("tail");
      return undefined;
    }
    turn.entries.push(entry);
    this.versions.touch(`turn:${turn.started.turn_id}`);
    return turn.started.turn_id;
  }

  #startItem(started: ItemStarted): void {
    const id = started.item_id;
    if (Object.hasOwn(this.state.items, id)) {
      throw duplicate(id);
    }
    const item = withTurn(started, this.#place({ item: id }, started.turn_id ?? undefined));
    if ((item as { kind: string }).kind === "late_tool_output") {
      const of = (item as { ref: { item_id: string } }).ref.item_id;
      this.state.late_by_ref[of] = id;
      this.versions.touch(`item:${of}`);
    }
    this.state.items[id] = { item, text: "", output: "" };
    this.versions.touch(`item:${id}`);
  }

  #request(requested: InteractionRequested): void {
    const { request_id: id } = requested;
    const turnId = requested.turn_id ?? undefined;
    const index = turnId === undefined ? -1 : this.#turnIndex(turnId);
    if (Object.hasOwn(this.state.requests, id)) {
      // Its turn's history page puts a detached request back in place.
      const detached = this.state.detached.indexOf(id);
      const turn = this.state.turns[index];
      if (detached === -1 || turn === undefined) {
        throw duplicate(id);
      }
      this.state.detached.splice(detached, 1);
      turn.entries.push({ request: id });
      this.versions.touch("detached", `turn:${turn.started.turn_id}`, `request:${id}`);
      return;
    }
    if (turnId !== undefined && index === -1) {
      this.state.detached.push(id);
      this.state.requests[id] = { requested };
      this.versions.touch("detached", `request:${id}`);
      return;
    }
    const placed = withTurn(requested, this.#place({ request: id }, turnId));
    this.state.requests[id] = { requested: placed };
    this.versions.touch(`request:${id}`);
  }
}

/** A copy of `value` in `turnId`, or in none. */
function withTurn<T extends { turn_id?: string | null }>(value: T, turnId: string | undefined): T {
  const copy = { ...value };
  delete copy.turn_id;
  return turnId === undefined ? copy : { ...copy, turn_id: turnId };
}

/** Sets the fields of `changes` that `item`'s kind has; an unknown kind takes all of them. */
function patch(item: ItemStarted, changes: Record<string, unknown>): void {
  const { kind } = item as { kind: string };
  const fields = Object.hasOwn(PATCHABLE, kind) ? PATCHABLE[kind] : undefined;
  for (const [key, value] of Object.entries(changes)) {
    if (value != null && !NOT_PATCH.has(key) && (fields?.includes(key) ?? true)) {
      (item as Record<string, unknown>)[key] = value;
    }
  }
}
