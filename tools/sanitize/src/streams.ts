/**
 * Text that a recording carries over several frames (design 5.9.3): an agent
 * sends a message, a thought or a command's output in chunks, so a path or a
 * name may be whole in no single frame. The scan and the redaction fold each
 * text before they match it.
 */

// eslint-disable-next-line @typescript-eslint/no-explicit-any -- recordings are free-form JSON
type Json = any;

/** One chunk of a streamed text: the frame it is in, and the object and key that hold it. */
export interface Piece {
  frame: number;
  at: Json;
  key: string;
}

/**
 * The streamed texts in a recording's frames, each as its chunks in order:
 * message and thought chunks grouped by session and message ID, a command's
 * output by tool call, and chunks with no ID by run of adjacent chunks of one
 * kind.
 */
export function streams(frames: readonly Json[]): Piece[][] {
  const groups = new Map<string, Piece[]>();
  let run = 0;
  let previous = "";
  frames.forEach((f, frame) => {
    const params = f?.msg?.params;
    const u = params?.update;
    if (typeof u?.sessionUpdate !== "string") return;
    const kind = `${params.sessionId}\0${u.sessionUpdate}`;
    if (kind !== previous) run++;
    previous = kind;
    const output = u._meta?.terminal_output;
    const piece =
      u.sessionUpdate.endsWith("_chunk") && typeof u.content?.text === "string"
        ? { frame, at: u.content, key: "text", id: u.messageId }
        : typeof output?.data === "string"
          ? { frame, at: output, key: "data", id: u.toolCallId }
          : undefined;
    if (!piece) return;
    const group = `${kind}\0${typeof piece.id === "string" ? piece.id : `#${run}`}`;
    const pieces = groups.get(group) ?? [];
    groups.set(group, pieces);
    pieces.push(piece);
  });
  return [...groups.values()];
}

/** The pieces joined, and for each UTF-16 unit of the result the piece it came from. */
export function fold(pieces: readonly string[]): { text: string; owner: number[] } {
  return {
    text: pieces.join(""),
    owner: pieces.flatMap((p, i) => Array.from({ length: p.length }, () => i)),
  };
}
