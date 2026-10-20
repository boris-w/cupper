// Agent content in full (design 5.3.6: what the user approves is never cut
// short). Text comes inline; a blob downloads on expanding, and shows only
// once it arrived whole and matched its hash.

import { escapeInvisible } from "@cupper/i18n";
import type { BlobRef, Content } from "@cupper/protocol";
import { useEffect, useState } from "react";

import { useT } from "../app/providers";
import { useBlobText } from "../state/blob";

/**
 * A box of agent content, collapsed to a few lines until expanded; a blob
 * downloads on expanding. Expanded, it scrolls within a cap so the options
 * stay in view. `onSeen` hears whether the full text is there now: expanded
 * and loaded, not scrolled to the end (security #6); collapsing takes it back.
 */
export function ContentBox(props: {
  label: string;
  content: Content;
  onSeen?: (seen: boolean) => void;
}) {
  const t = useT();
  const { content, onSeen } = props;
  const blob = content.type === "blob" && "blob" in content ? (content.blob as BlobRef) : undefined;
  const preview = blob && "preview" in content ? String(content.preview) : undefined;
  const full = useBlobText(blob);
  const inline = content.type === "text" && "text" in content ? String(content.text) : undefined;
  const [open, setOpen] = useState(false);
  const loaded = inline !== undefined || full.status === "loaded";
  useEffect(() => onSeen?.(open && loaded), [open, loaded, onSeen]);
  if (inline === undefined && !blob) return null;
  const toggle = () => {
    if (!open && !loaded) full.load();
    setOpen(!open);
  };
  return (
    <div className="mx-4 mb-2 overflow-hidden rounded-lg border border-border bg-surface-code text-code">
      <div className="flex items-center justify-between px-3 py-1 text-xs text-fg-tertiary">
        {props.label}
        <button type="button" onClick={toggle} className="rounded-md px-1 hover:bg-hover">
          {t(open ? "request:collapse" : "request:expand")}
        </button>
      </div>
      <pre
        className={`overflow-x-auto px-3 pb-2 font-mono whitespace-pre-wrap ${open && loaded ? "max-h-60 overflow-y-auto" : "max-h-24 overflow-y-hidden"}`}
      >
        {escapeInvisible(inline ?? full.text ?? preview ?? "")}
      </pre>
      {full.status === "loading" && (
        <p className="px-3 pb-2 text-xs text-fg-tertiary">{t("request:loading")}</p>
      )}
      {full.status === "failed" && (
        <p className="flex items-center gap-2 px-3 pb-2 text-xs text-danger">
          {t("request:load_failed")}
          <button
            type="button"
            onClick={full.load}
            className="rounded-md px-1 text-fg hover:bg-hover"
          >
            {t("request:retry")}
          </button>
        </p>
      )}
    </div>
  );
}
