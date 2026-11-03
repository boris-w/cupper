// Code highlighting (design 5.7.6, 5.7.10). Shiki loads with the first code
// block that needs it, on the JavaScript regex engine: no WebAssembly, so the
// CSP stays as it is. Its theme has a placeholder colour for each of the
// design's seven categories; a token's colour only says which category it
// is, and the page gets a fixed `hl-*` class (styles/stream.css), never a
// colour or a style. Both themes come from those classes.

import type { LanguageRegistration, ThemeRegistrationRaw } from "@shikijs/primitive";
import { useEffect, useState } from "react";

type Grammar = () => Promise<{ default: LanguageRegistration[] }>;

/** The languages highlighted, by Shiki's name; a block in any other stays plain. */
const GRAMMARS: Record<string, Grammar> = {
  c: () => import("@shikijs/langs/c"),
  cpp: () => import("@shikijs/langs/cpp"),
  csharp: () => import("@shikijs/langs/csharp"),
  css: () => import("@shikijs/langs/css"),
  dart: () => import("@shikijs/langs/dart"),
  diff: () => import("@shikijs/langs/diff"),
  docker: () => import("@shikijs/langs/docker"),
  go: () => import("@shikijs/langs/go"),
  graphql: () => import("@shikijs/langs/graphql"),
  html: () => import("@shikijs/langs/html"),
  ini: () => import("@shikijs/langs/ini"),
  java: () => import("@shikijs/langs/java"),
  javascript: () => import("@shikijs/langs/javascript"),
  json: () => import("@shikijs/langs/json"),
  jsonc: () => import("@shikijs/langs/jsonc"),
  jsx: () => import("@shikijs/langs/jsx"),
  kotlin: () => import("@shikijs/langs/kotlin"),
  lua: () => import("@shikijs/langs/lua"),
  make: () => import("@shikijs/langs/make"),
  markdown: () => import("@shikijs/langs/markdown"),
  "objective-c": () => import("@shikijs/langs/objective-c"),
  php: () => import("@shikijs/langs/php"),
  powershell: () => import("@shikijs/langs/powershell"),
  python: () => import("@shikijs/langs/python"),
  ruby: () => import("@shikijs/langs/ruby"),
  rust: () => import("@shikijs/langs/rust"),
  scala: () => import("@shikijs/langs/scala"),
  scss: () => import("@shikijs/langs/scss"),
  shellscript: () => import("@shikijs/langs/shellscript"),
  sql: () => import("@shikijs/langs/sql"),
  swift: () => import("@shikijs/langs/swift"),
  toml: () => import("@shikijs/langs/toml"),
  tsx: () => import("@shikijs/langs/tsx"),
  typescript: () => import("@shikijs/langs/typescript"),
  xml: () => import("@shikijs/langs/xml"),
  yaml: () => import("@shikijs/langs/yaml"),
  zig: () => import("@shikijs/langs/zig"),
};

/** Other names of those languages in a fence's info string. */
const ALIASES: Record<string, string> = {
  bash: "shellscript",
  sh: "shellscript",
  shell: "shellscript",
  zsh: "shellscript",
  js: "javascript",
  mjs: "javascript",
  ts: "typescript",
  py: "python",
  rb: "ruby",
  rs: "rust",
  yml: "yaml",
  md: "markdown",
  "c++": "cpp",
  cs: "csharp",
  dockerfile: "docker",
  makefile: "make",
  objc: "objective-c",
  kt: "kotlin",
};

/** The `language-*` classes a code block may carry into the renderer: the names above. */
export const LANGUAGE_CLASSES = [...Object.keys(GRAMMARS), ...Object.keys(ALIASES)].map(
  (name) => `language-${name}`,
);

/** The language `info` names, by Shiki's name, if it is highlighted. */
export function language(info: string | undefined): string | undefined {
  const name = info !== undefined && Object.hasOwn(ALIASES, info) ? ALIASES[info] : info;
  return name !== undefined && Object.hasOwn(GRAMMARS, name) ? name : undefined;
}

/** The categories of design 5.7.10's table, each with the TextMate scopes it colours. */
const CATEGORIES = {
  keyword: ["keyword", "storage", "keyword.control"],
  string: ["string", "punctuation.definition.string", "markup.inline.raw"],
  literal: ["constant.numeric", "constant.language", "constant.character", "support.constant"],
  variable: ["variable", "support.variable", "variable.other.constant"],
  attribute: [
    "entity.other.attribute-name",
    "support.type.property-name",
    "meta.object-literal.key",
  ],
  name: ["entity.name", "support.function", "support.class", "support.type", "entity.name.tag"],
  comment: ["comment", "punctuation.definition.comment"],
};
export type Category = keyof typeof CATEGORIES;

const TEXT = "#000000";
const placeholder = (n: number) => `#00000${n + 1}`;
const BY_COLOUR = new Map(
  Object.keys(CATEGORIES).map((name, n) => [placeholder(n), name as Category]),
);

const THEME: ThemeRegistrationRaw = {
  name: "cupper",
  settings: [
    { settings: { foreground: TEXT, background: "#ffffff" } },
    ...Object.values(CATEGORIES).map((scope, n) => ({
      scope,
      settings: { foreground: placeholder(n) },
    })),
    // Operators and punctuation keep the text's colour.
    { scope: ["keyword.operator", "punctuation"], settings: { foreground: TEXT } },
  ],
};

/** A line of code: its pieces, each in a category or in the text's colour. */
export type Line = { text: string; category?: Category }[];

/** Longer blocks stay plain: tokenizing runs on the renderer's thread. */
const MAX_CHARS = 50_000;

const load = async () => {
  const [{ createShikiPrimitiveAsync, codeToTokensBase }, { createJavaScriptRegexEngine }] =
    await Promise.all([import("@shikijs/primitive"), import("@shikijs/engine-javascript")]);
  const engine = createJavaScriptRegexEngine({ forgiving: true });
  const shiki = await createShikiPrimitiveAsync({ engine, themes: [THEME], warnings: false });
  return { shiki, codeToTokensBase };
};
let loading: ReturnType<typeof load> | undefined;

/** `code` in `lang` (Shiki's name), as lines of categorized pieces. */
export async function highlight(code: string, lang: string): Promise<Line[]> {
  const { shiki, codeToTokensBase } = await (loading ??= load());
  const grammar = GRAMMARS[lang];
  if (grammar && !shiki.getLoadedLanguages().includes(lang)) {
    await shiki.loadLanguage(...(await grammar()).default);
  }
  return codeToTokensBase(shiki, code, { lang, theme: "cupper" }).map((tokens) =>
    tokens.map(({ content, color }) => ({ text: content, category: BY_COLOUR.get(color ?? "") })),
  );
}

/** Highlighted blocks by language and text: a row scrolled back into view does not redo its own. */
const done = new Map<string, Line[]>();
const KEEP = 200;

/**
 * The lines of `code` once highlighted; `undefined` before, and for a block
 * with no `lang` (unknown, or still streaming), which is never highlighted.
 */
export function useHighlight(code: string, lang: string | undefined): Line[] | undefined {
  const key = lang === undefined || code.length > MAX_CHARS ? undefined : `${lang}\n${code}`;
  const [, ready] = useState(0);
  useEffect(() => {
    if (key === undefined || lang === undefined || done.has(key)) return undefined;
    let live = true;
    highlight(code, lang).then(
      (lines) => {
        done.set(key, lines);
        const oldest = done.keys().next().value;
        if (done.size > KEEP && oldest !== undefined) done.delete(oldest);
        if (live) ready((n) => n + 1);
      },
      // A grammar that fails leaves the block plain.
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [key, code, lang]);
  return key === undefined ? undefined : done.get(key);
}
