// The main process's HostPort (shell-api): its end of the native extension's
// socketpair (T073) in a `net.Socket`. Frames go out whole; inbound bytes go
// on in the chunks they came in, each its own ArrayBuffer.

import type { Socket } from "node:net";

import { encodeBlobChunk, encodeFrame, FRAME_JSON } from "@cupper/host-client";
import type { HostPort } from "@cupper/shell-api";

export function socketPort(socket: Socket): HostPort {
  return {
    send: (json) => socket.write(encodeFrame(FRAME_JSON, new TextEncoder().encode(json))),
    sendBlob: (hash, offset, last, bytes) =>
      socket.write(encodeBlobChunk({ hash, offset, last, bytes: new Uint8Array(bytes) })),
    onData: (cb) => {
      const data = (chunk: Buffer) => cb(new Uint8Array(chunk).slice().buffer);
      socket.on("data", data);
      return () => socket.off("data", data);
    },
    // Flow control on this end is the window bridge's (T076).
    ack: () => undefined,
    close: () => socket.destroy(),
  };
}
