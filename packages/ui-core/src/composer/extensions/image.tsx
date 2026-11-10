// Images (design 5.7.4, CP-2): pasted into the text area, dropped on the
// composer, or picked with "+", each becomes an `image` attachment, its
// bytes put to the host as a blob first. PNG, JPEG, GIF and WebP only, up to
// the agent's `max_image_bytes`; an agent that takes no images says so. While
// files are dragged over the composer, an overlay says what a drop does. The
// thumbnails are object URLs of the local files, released with their
// attachments and with the composer.

import type { PromptBlock } from "@cupper/protocol";
import { use, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useStore } from "zustand";

import { useHost, useT } from "../../app/providers";
import { PlusIcon } from "../../components/icons";
import { useToasts } from "../../feedback/toaster";
import { useStreams, useUi } from "../../state/provider";
import { BarButton } from "../config";
import { OverlayContext, type ComposerExtension } from "../extensions";
import { NEW_DRAFT, codeOf } from "../send";
import { keepThumbnail, releaseThumbnails } from "../thumbnails";

export const IMAGE_TYPES = ["image/png", "image/jpeg", "image/gif", "image/webp"];
export const isImage = (file: File) => IMAGE_TYPES.includes(file.type);

const NONE: readonly PromptBlock[] = [];
const mib = (bytes: number) => `${Math.round((bytes / 1024 / 1024) * 10) / 10} MB`;

function Images({ sessionId }: { sessionId?: string }) {
  const t = useT();
  const ui = useUi();
  const { client } = useHost();
  const { show } = useToasts();
  const streams = useStreams();
  const { area } = use(OverlayContext);
  const key = sessionId ?? NEW_DRAFT;
  const picker = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  const attachments = useStore(ui, (state) => state.attachments[key] ?? NONE);

  const add = async (files: File[]) => {
    const prompt = sessionId && streams.session(sessionId)?.view.toJSON().capabilities?.prompt;
    for (const file of files) {
      const refused = !isImage(file)
        ? t("composer:image_type")
        : prompt && !prompt.image
          ? t("composer:image_unsupported")
          : prompt && file.size > prompt.max_image_bytes
            ? t("composer:image_size", { size: mib(prompt.max_image_bytes) })
            : undefined;
      if (refused) {
        show({ id: "image", text: refused });
        continue;
      }
      try {
        const bytes = new Uint8Array(await file.arrayBuffer());
        const blob = await client.putBlob(bytes, file.type);
        keepThumbnail(blob.hash, file);
        const now = ui.getState().attachments[key] ?? [];
        ui.getState().setAttachments(key, [...now, { type: "image", blob }]);
      } catch (error) {
        show({ id: "image", text: t("composer:image_failed", { code: codeOf(error) }) });
      }
    }
  };
  const latest = useRef(add);
  latest.current = add;

  // An image no longer attached lets its thumbnail go; so does the composer.
  useEffect(() => {
    const hashes = attachments.flatMap((block) =>
      block.type === "image" ? [(block as { blob: { hash: string } }).blob.hash] : [],
    );
    releaseThumbnails(new Set(hashes));
  }, [attachments]);
  useEffect(() => () => releaseThumbnails(), []);

  // Pasted images; dropped ones, while other files are T142's references.
  const box = area?.closest<HTMLElement>("[data-composer]");
  // The composer holds the overlay only while files are over it: positioned
  // for good, its shadow would paint over what follows it.
  useEffect(() => {
    box?.toggleAttribute("data-dragging", over);
  }, [box, over]);
  useEffect(() => {
    if (!area || !box) return;
    const images = (list?: FileList | null) => [...(list ?? [])].filter(isImage);
    // A pasted image of another type is refused with a hint; a dropped one is a file.
    const paste = (event: ClipboardEvent) => {
      const files = [...(event.clipboardData?.files ?? [])];
      const found = files.filter((file) => file.type.startsWith("image/"));
      if (found.length === 0) return;
      event.preventDefault();
      void latest.current(found);
    };
    const drag = (event: DragEvent) => {
      if (!event.dataTransfer?.types.includes("Files")) return;
      event.preventDefault();
      setOver(event.type !== "dragleave" || box.contains(event.relatedTarget as Node | null));
    };
    const drop = (event: DragEvent) => {
      setOver(false);
      const found = images(event.dataTransfer?.files);
      if (found.length === 0) return;
      event.preventDefault();
      void latest.current(found);
    };
    area.addEventListener("paste", paste);
    const drags = ["dragenter", "dragover", "dragleave"] as const;
    drags.forEach((name) => box.addEventListener(name, drag));
    box.addEventListener("drop", drop);
    return () => {
      area.removeEventListener("paste", paste);
      drags.forEach((name) => box.removeEventListener(name, drag));
      box.removeEventListener("drop", drop);
    };
  }, [area, box]);

  return (
    <>
      <BarButton label={t("composer:image_add")} onClick={() => picker.current?.click()}>
        <PlusIcon />
      </BarButton>
      <input
        ref={picker}
        type="file"
        accept={IMAGE_TYPES.join(",")}
        multiple
        hidden
        onChange={(event) => {
          const files = [...(event.target.files ?? [])];
          event.target.value = "";
          void add(files);
        }}
      />
      {over &&
        box &&
        createPortal(
          <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center rounded-composer border-2 border-dashed border-border-strong bg-surface-composer/90 text-sm text-fg-secondary">
            {t("composer:image_drop")}
          </div>,
          box,
        )}
    </>
  );
}

const extension: ComposerExtension = { side: "left", order: 1, Component: Images };
export default extension;
