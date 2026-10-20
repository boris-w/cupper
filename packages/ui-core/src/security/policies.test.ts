import { describe, expect, test } from "vitest";

import { assetScript, workerPolicy, WORKER_POLICY, type TrustedTypesFactory } from "./policies";

describe("UNIT-ui-lint-rules: the Trusted Types policy", () => {
  test("only scripts under app://cupper/assets/ become script URLs", () => {
    expect(assetScript("app://cupper/assets/worker-a1.js")).toBe(
      "app://cupper/assets/worker-a1.js",
    );
    for (const url of [
      "app://cupper/assets/../index.js",
      "app://cupper/worker.js",
      "app://cupper-review/assets/worker.js",
      "https://cupper/assets/worker.js",
      "data:text/javascript,1",
      "blob:app://cupper/1",
      "assets/worker.js",
    ]) {
      expect(() => assetScript(url), url).toThrow(TypeError);
    }
  });

  test("one policy, cupper-worker, with that check; never default", () => {
    const made: string[] = [];
    const factory: TrustedTypesFactory = {
      createPolicy: (name, rules) => {
        made.push(name);
        return { createScriptURL: (url) => ({ trusted: rules.createScriptURL(url) }) };
      },
    };
    const policy = workerPolicy(factory);
    expect(made).toEqual([WORKER_POLICY]);
    expect(policy.createScriptURL("app://cupper/assets/w.js")).toEqual({
      trusted: "app://cupper/assets/w.js",
    });
    expect(() => policy.createScriptURL("app://cupper/index.js")).toThrow(TypeError);
    expect(workerPolicy(undefined).createScriptURL("app://cupper/assets/w.js")).toBe(
      "app://cupper/assets/w.js",
    );
  });
});
