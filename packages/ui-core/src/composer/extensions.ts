// The composer's extension points: M1b's tasks (T140, T141, T142, T145,
// T149) add files only. A file in `extensions/` exporting a
// `ComposerExtension` by default joins the bottom bar; an overlay (`@`, `/`)
// mounts in the popover container and registers its close for Esc.

import { createContext, type ComponentType } from "react";

export interface ComposerExtension {
  /** Left: attachments, permissions; right: model and effort, before the send button. */
  side: "left" | "right";
  order: number;
  Component: ComponentType<{ sessionId?: string }>;
}

const found = import.meta.glob<ComposerExtension>("./extensions/*.tsx", {
  eager: true,
  import: "default",
});

export const EXTENSIONS = Object.values(found).toSorted((a, b) => a.order - b.order);

export interface ComposerOverlays {
  /** Where popovers mount, just above the composer. */
  container: HTMLElement | null;
  /** The open overlay's close, set while it is open: Esc calls it first. */
  setClose(close: (() => void) | null): void;
}

export const OverlayContext = createContext<ComposerOverlays>({
  container: null,
  setClose: () => undefined,
});
