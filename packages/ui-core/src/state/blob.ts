// A blob's text on demand (host-client getBlob): it checks the bytes
// against the hash, so what shows is what the reference names.

import type { BlobRef } from "@cupper/protocol";
import { useCallback, useEffect, useRef, useState } from "react";

import { useHost } from "../app/providers";

export interface BlobText {
  status: "idle" | "loading" | "loaded" | "failed";
  text?: string;
  load(): void;
}

/** A blob's text, downloaded when `load` is called; dropped if the component goes first. */
export function useBlobText(blob: BlobRef | undefined): BlobText {
  const { client } = useHost();
  const [state, setState] = useState<Omit<BlobText, "load">>({ status: "idle" });
  // Only the latest download lands; unmounting (the request ended) drops it.
  const latest = useRef(0);
  useEffect(() => () => void latest.current++, []);
  const load = useCallback(() => {
    if (!blob) return;
    const mine = ++latest.current;
    setState({ status: "loading" });
    client.getBlob(blob).then(
      (bytes) =>
        mine === latest.current &&
        setState({ status: "loaded", text: new TextDecoder().decode(bytes) }),
      () => mine === latest.current && setState({ status: "failed" }),
    );
  }, [client, blob]);
  return { ...state, load };
}
