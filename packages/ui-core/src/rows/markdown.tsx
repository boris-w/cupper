// The assistant's Markdown (design 5.7.8 "Markdown and untrusted content"):
// the main way agent text reaches the renderer. streamdown with every plugin
// named: GFM only; no `rehype-raw`, so raw HTML is never parsed; the
// sanitizer's schema narrowed to http(s) URLs and no agent class names; then
// rehype-harden. Links open through `ShellAPI.openExternal`, never in the
// document; an image is a link to it and never loads. The look is static CSS
// (`.md` in styles/stream.css, design 5.7.10).

import { harden } from "rehype-harden";
import rehypeSanitize, { defaultSchema, type Options as Schema } from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import type { ReactNode } from "react";
import { Streamdown, type Components, type StreamdownProps } from "streamdown";

import { useShell } from "../app/providers";

type PluggableList = NonNullable<StreamdownProps["rehypePlugins"]>;

const WEB = ["http", "https"];

/** Whether `url` is an absolute http(s) URL. */
export const isWeb = (url: unknown): url is string =>
  typeof url === "string" && /^https?:\/\//i.test(url);

const schema: Schema = {
  ...defaultSchema,
  protocols: { href: WEB, src: WEB, cite: WEB },
  // A class name could pick a style of the app's own (T136): none from agents.
  attributes: { ...defaultSchema.attributes, code: [] },
};

export const remarkPlugins: PluggableList = [remarkGfm];
export const rehypePlugins: PluggableList = [
  [rehypeSanitize, schema],
  // The schema already keeps http(s) only; specific prefixes would make
  // harden resolve relative URLs against an origin, so it gets `*`.
  [
    harden,
    {
      allowedLinkPrefixes: ["*"],
      allowedImagePrefixes: ["*"],
      allowDataImages: false,
      linkBlockPolicy: "text-only",
      imageBlockPolicy: "text-only",
    },
  ],
];

function Link({ href, children }: { href?: string; children?: ReactNode }) {
  const { api } = useShell();
  if (!isWeb(href)) {
    return <span>{children}</span>;
  }
  return (
    <a
      href={href}
      className="text-accent hover:underline"
      onClick={(event) => {
        event.preventDefault();
        api.openExternal(href);
      }}
    >
      {children}
    </a>
  );
}

const components: Components = {
  a: ({ href, children }) => <Link href={href}>{children}</Link>,
  // streamdown draws these as spans with utility classes this app lacks.
  strong: ({ children }) => <strong>{children}</strong>,
  // Plain blocks: highlighting and code block controls are T143's.
  pre: ({ children }) => <pre>{children}</pre>,
  table: ({ children }) => (
    <div className="overflow-x-auto">
      <table>{children}</table>
    </div>
  ),
  code: ({ children }) => <code>{children}</code>,
  // Shown as a link to the picture: an agent's image never loads.
  img: ({ src, alt }) => <Link href={typeof src === "string" ? src : undefined}>{alt || src}</Link>,
};

/** Agent Markdown, `streaming` while it grows. */
export function Markdown({ text, streaming }: { text: string; streaming: boolean }) {
  return (
    <Streamdown
      className="md"
      mode={streaming ? "streaming" : "static"}
      isAnimating={streaming}
      remarkPlugins={remarkPlugins}
      rehypePlugins={rehypePlugins}
      components={components}
      skipHtml
      controls={false}
      linkSafety={{ enabled: false }}
    >
      {text}
    </Streamdown>
  );
}
