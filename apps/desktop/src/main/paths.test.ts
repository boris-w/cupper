import { describe, expect, it } from "vitest";

import { appDirs, e2eDirs } from "./paths";

const env = { appData: "/Users/u/Library/Application Support", home: "/Users/u" };

describe("app directories", () => {
  it("give each flavor its own data, userData and logs", () => {
    expect(appDirs("release", env)).toEqual({
      data: "/Users/u/Library/Application Support/cupper",
      userData: "/Users/u/Library/Application Support/cupper/ui",
      logs: "/Users/u/Library/Logs/cupper",
    });
    expect(appDirs("local", env).userData).toBe(
      "/Users/u/Library/Application Support/cupper-local/ui",
    );
    expect(appDirs("dev", env).logs).toBe("/Users/u/Library/Logs/cupper-dev");
  });

  it("keep an e2e build under its temporary root", () => {
    expect(e2eDirs("/tmp/case-1")).toEqual({
      data: "/tmp/case-1/data",
      userData: "/tmp/case-1/data/ui",
      logs: "/tmp/case-1/logs",
    });
    expect(() => e2eDirs(undefined)).toThrow(/CUPPER_E2E_DIR/);
    expect(() => e2eDirs("relative/dir")).toThrow(/absolute/);
  });
});
