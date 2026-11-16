// The blocks of an agent's Markdown while it streams (T159). streamdown parses
// each block on its own and keeps those that did not change, so an update
// costs what its last block costs; but a block grows for as long as the text
// has no blank line, and an agent's text can run to megabytes. While a message
// streams, a block longer than `PIECE` is cut into pieces, each parsed and
// laid out on its own, and only the text after the settled pieces is split
// again. Once the message is done it renders whole, without any of this.

import { parseMarkdownIntoBlocks } from "streamdown";

/** The most text of one block parsed and laid out again on each update. */
export const PIECE = 16 * 1024;

/** A piece of a block: its text, and how much of the block it stands for. */
interface Piece {
  text: string;
  length: number;
}

const FENCE = /^ {0,3}(`{3,}|~{3,})/;
/** Where a piece of text may start: a word, not a list's number. */
const WORD = /^(?!\d+[.)]\s)[\p{L}\p{N}]/u;

/** `block` in pieces of at most `PIECE`; a single longer line stays whole. */
export function cut(block: string): Piece[] {
  if (block.length <= PIECE) return [{ text: block, length: block.length }];
  const fence = FENCE.exec(block)?.[1];
  return fence ? cutFence(block, fence) : cutText(block);
}

/** A fenced block at the ends of its lines, each piece fenced as the block is. */
function cutFence(block: string, fence: string): Piece[] {
  const lines = block.match(/[^\n]*\n|[^\n]+$/g) ?? [block];
  const opener = lines[0] ?? "";
  const pieces: Piece[] = [];
  let source = "";
  for (const line of lines) {
    if (source.length > 0 && source.length + line.length > PIECE) {
      pieces.push({
        text: `${pieces.length ? opener : ""}${source}${fence}\n`,
        length: source.length,
      });
      source = "";
    }
    source += line;
  }
  pieces.push({ text: `${pieces.length ? opener : ""}${source}`, length: source.length });
  return pieces;
}

/** Text, after a space before a word, or else where `PIECE` ends. */
function cutText(block: string): Piece[] {
  const pieces: Piece[] = [];
  let at = 0;
  while (block.length - at > PIECE) {
    let end = at + PIECE;
    for (let i = end - 1; i > at + PIECE / 2; i--) {
      if (/\s/.test(block[i] ?? "") && WORD.test(block.slice(i + 1, i + 16))) {
        end = i + 1;
        break;
      }
    }
    // Not between the halves of a surrogate pair.
    if (/[\uDC00-\uDFFF]/.test(block[end] ?? "")) end -= 1;
    pieces.push({ text: block.slice(at, end), length: end - at });
    at = end;
  }
  pieces.push({ text: block.slice(at), length: block.length - at });
  return pieces;
}

/**
 * The blocks of one message while it streams, for streamdown's
 * `parseMarkdownIntoBlocksFn`: each call splits only what follows the pieces
 * an earlier call settled, if the text still starts with them. Settled are
 * the pieces of every block but the last, and of a long last block that is
 * no fenced one, every piece but its last.
 */
export function streamingBlocks(): (markdown: string) => string[] {
  let settled = "";
  let kept: string[] = [];
  return (markdown) => {
    if (!markdown.startsWith(settled)) {
      settled = "";
      kept = [];
    }
    const tail = markdown.slice(settled.length);
    const blocks = parseMarkdownIntoBlocks(tail);
    const pieces = blocks.map(cut);
    const out = [...kept, ...pieces.flat().map((piece) => piece.text)];
    // Blocks that are not the text, in order and whole, settle nothing.
    if (blocks.reduce((sum, block) => sum + block.length, 0) !== tail.length) return out;
    let length = settled.length;
    pieces.forEach((list, n) => {
      const last = n === pieces.length - 1;
      const done = !last ? list : FENCE.test(blocks[n] ?? "") ? [] : list.slice(0, -1);
      for (const piece of done) {
        kept.push(piece.text);
        length += piece.length;
      }
    });
    settled = markdown.slice(0, length);
    return out;
  };
}
