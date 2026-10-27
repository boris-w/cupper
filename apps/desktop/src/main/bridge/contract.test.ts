import { hostPortContract } from "@cupper/host-client/contract";
import { MockHost } from "@cupper/host-client/mock";
import { describe } from "vitest";

import { fixtures, wired } from "./testing";

describe("UNIT-hostport-contract-bridge", () => {
  hostPortContract(async () => {
    const host = new MockHost(fixtures);
    return { shell: wired(host).shell, host };
  });
});
