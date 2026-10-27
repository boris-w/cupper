import { describe } from "vitest";

import { hostPortContract } from "../contract";
import { fixtures } from "../testing";
import { MockHost } from "./host";
import { createMockShellHost } from "./port";

describe("UNIT-hostport-contract-mock", () => {
  hostPortContract(async () => {
    const host = new MockHost(fixtures);
    return { shell: createMockShellHost(host), host };
  });
});
