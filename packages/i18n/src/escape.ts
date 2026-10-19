// Agent text on the request panel and in the native confirmation dialogs is
// shown verbatim, except for the characters that change how text looks without
// showing themselves (design 5.7.8). Those become visible escapes, so that
// what the user approves reads the way it will run.

/**
 * - Control characters (Cc) except tab and line feed: ESC, CR, NUL, DEL, C1.
 * - Format characters (Cf): the bidirectional controls (U+202A–U+202E,
 *   U+2066–U+2069, LRM, RLM, ALM), the zero-width characters (U+200B–U+200D,
 *   U+2060, U+FEFF), the soft hyphen, the tag characters (U+E0000–U+E007F).
 * - Line and paragraph separators (Zl, Zp), and lone surrogates (Cs).
 * - Default ignorable code points: Unicode's list of characters that render
 *   as nothing, such as the Hangul fillers, the combining grapheme joiner and
 *   the variation selectors. An emoji with a variation selector or a
 *   zero-width joiner shows its escapes too; in text to approve, nothing
 *   hidden is the better trade.
 */
const INVISIBLE = /[[\p{Cc}\p{Cf}\p{Cs}\p{Zl}\p{Zp}\p{Default_Ignorable_Code_Point}]--[\t\n]]/gv;

/** Replaces each invisible character with `\u{XXXX}`, its code point in hex. */
export function escapeInvisible(text: string): string {
  return text.replace(INVISIBLE, (char) => {
    const code = char.codePointAt(0) ?? 0;
    return `\\u{${code.toString(16).toUpperCase()}}`;
  });
}
