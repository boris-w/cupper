import { EventEmitter } from "node:events";

import { describe, expect, it, vi } from "vitest";

import { catchUncaught } from "./errors";

describe("errors nothing caught", () => {
  it("go to main.log with their stack and cause", () => {
    const proc = new EventEmitter();
    const log = { error: vi.fn() };
    catchUncaught(proc, log);
    const error = new Error("socket gone", { cause: new Error("EPIPE") });
    proc.emit("uncaughtException", error, "uncaughtException");
    proc.emit("unhandledRejection", "a string", Promise.resolve());
    expect(log.error).toHaveBeenNthCalledWith(1, "uncaught exception", {
      origin: "uncaughtException",
      error: expect.stringMatching(/Error: socket gone\n {4}at [^]*\[cause\]: Error: EPIPE/),
    });
    expect(log.error).toHaveBeenNthCalledWith(2, "unhandled rejection", { error: "'a string'" });
  });
});
