import { describe } from "vitest";

import { hostPortContract } from "../contract";
import { fixtures } from "../testing";
import { MockHost } from "./host";
import { createMockShellHost } from "./port";

describe("UNIT-hostport-contract-mock", () => {
  hostPortContract(async () => {
    const host = new MockHost(fixtures);
    const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
    return { shell: createMockShellHost(host), host, settle };
  });
});
