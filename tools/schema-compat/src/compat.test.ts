import { readFileSync } from "node:fs";

import { describe, expect, test } from "vitest";

import { compare, directions, unsafeIntegers, type Doc, type Node } from "./compat.ts";

const ref = (name: string) => ({ $ref: `#/$defs/${name}` });
const event = (type: string) => ({
  type: "object",
  properties: { type: { const: type, type: "string" }, data: ref("Data") },
  required: ["type", "data"],
});

/** A small protocol: one request and one event body, which goes to the client. */
function doc(): Doc {
  return {
    "x-methods": {
      requests: {
        "thing/get": {
          params: ref("Params"),
          result: { allOf: [ref("Thing"), ref("Replayed")] },
          "x-scope": "read",
          "x-idempotent": "command",
        },
      },
      host_notifications: {},
      client_notifications: {},
    },
    $defs: {
      Params: {
        type: "object",
        properties: { id: { type: "string" }, kind: ref("Kind"), limit: { type: "integer" } },
        required: ["id"],
      },
      Thing: {
        type: "object",
        properties: { id: { type: "string" }, note: { type: "string" }, state: ref("State") },
        required: ["id"],
      },
      Replayed: { type: "object", properties: { replayed: { const: true } } },
      // Open: anything else is a string; closed: these values only.
      Kind: { anyOf: [{ type: "string", enum: ["a", "b"] }, { type: "string" }] },
      State: { type: "string", enum: ["on", "off"] },
      Data: { type: "object" },
      EventBody: { anyOf: [event("thing.added"), ref("UnknownEvent")] },
      UnknownEvent: { type: "object", properties: { type: { type: "string" } } },
    },
  };
}

/** A definition of {@link doc}, as the changes below reach into it. */
interface Def {
  properties: Record<string, unknown>;
  required: string[];
  enum: string[];
  anyOf: { enum: string[] }[];
}
type Defs = Record<"Params" | "Thing" | "Kind" | "State" | "EventBody", Def>;

/** The breaking changes after `change` made to a copy of {@link doc}. */
function breaking(
  change: (d: Defs & { defs: Doc["$defs"]; requests: Record<string, Node> }) => void,
) {
  const now = structuredClone(doc());
  const defs = now.$defs as unknown as Defs;
  change({ ...defs, defs: now.$defs, requests: now["x-methods"].requests ?? {} });
  return compare(doc(), now)
    .filter((f) => f.breaking)
    .map((f) => `${f.at}: ${f.change}`);
}

describe("UNIT-schema-compat", () => {
  test("directions: params go in, results and the exported bodies go out", () => {
    const dirs = directions(doc());
    expect([...(dirs.get("Params") ?? [])]).toEqual(["in"]);
    expect([...(dirs.get("Kind") ?? [])]).toEqual(["in"]);
    expect([...(dirs.get("Thing") ?? [])]).toEqual(["out"]);
    expect([...(dirs.get("EventBody") ?? [])]).toEqual(["out"]);
  });

  test("the same schema, and additive changes, pass", () => {
    expect(breaking(() => undefined)).toEqual([]);
    expect(
      breaking((d) => {
        d.Thing.properties.extra = { type: "string" }; // out: a new field, even required
        d.Thing.required.push("extra");
        d.Params.properties.page = { type: "integer" }; // in: a new optional field
        d.Thing.required.push("note"); // out: optional made required
        d.Kind.anyOf[0]?.enum.push("c"); // an open enum's new value
        d.requests["thing/list"] = { params: ref("Params"), "x-scope": "read" };
      }),
    ).toEqual([]);
  });

  test("a new kind of event and a view's new optional member are additive", () => {
    expect(
      breaking((d) => {
        d.EventBody.anyOf.splice(1, 0, event("settings.updated") as never);
        d.Thing.properties.settings = ref("Data");
      }),
    ).toEqual([]);
  });

  test("removing a field, out a required field made optional, in a new required one: breaking", () => {
    expect(breaking((d) => delete d.Thing.properties.note)).toEqual([
      "$defs/Thing.note: field removed",
    ]);
    expect(breaking((d) => (d.Thing.required = []))).toEqual([
      "$defs/Thing.id: required made optional",
    ]);
    expect(
      breaking((d) => {
        d.Params.properties.token = { type: "string" };
        d.Params.required.push("token", "limit");
      }),
    ).toEqual([
      "$defs/Params.limit: optional made required",
      "$defs/Params.token: field added, required",
    ]);
  });

  test("types, closed values, removed branches and methods: breaking", () => {
    expect(breaking((d) => (d.Thing.properties.id = { type: "integer" }))).toEqual([
      "$defs/Thing.id: type string became integer",
    ]);
    expect(breaking((d) => (d.Params.properties.kind = ref("State")))).toEqual([
      "$defs/Params.kind: type Kind became State",
    ]);
    expect(breaking((d) => d.State.enum.push("broken"))).toEqual([
      '$defs/State "broken": value added',
    ]);
    expect(breaking((d) => d.Kind.anyOf[0]?.enum.pop())).toEqual(['$defs/Kind "b": value removed']);
    expect(breaking((d) => delete d.requests["thing/get"])).toEqual([
      "requests/thing/get: method removed",
    ]);
    expect(breaking((d) => ((d.requests["thing/get"] ?? {})["x-scope"] = "write"))).toEqual([
      "requests/thing/get x-scope: read became write",
    ]);
    expect(breaking((d) => delete d.defs.Data)).toEqual(["$defs/Data: definition removed"]);
  });

  test("the released schema against itself, and the exported schema's integers", () => {
    const v1 = JSON.parse(
      readFileSync(new URL("../../../protocol/released/v1-m1a.json", import.meta.url), "utf8"),
    ) as Doc;
    expect(compare(v1, v1)).toEqual([]);
    expect(unsafeIntegers(v1)).toEqual([]);
    const loose = {
      properties: {
        a: { type: "integer", format: "int64" },
        b: { type: "integer", minimum: 0, maximum: 2 ** 60 },
      },
    };
    expect(unsafeIntegers(loose)).toEqual(["#/properties/a", "#/properties/b"]);
  });
});
