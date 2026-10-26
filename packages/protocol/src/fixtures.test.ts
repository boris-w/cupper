import { readFileSync } from "node:fs";

import { describe, expect, test } from "vitest";
import type * as z from "zod";

import * as schemas from "./gen/zod.gen";
import {
  isKnownEvent,
  KNOWN_EVENT_TYPES,
  zEvent,
  zEventBody,
  type ItemStatus,
  type KnownEventType,
} from "./index";

/** The shared samples Rust checks too (UNIT-proto-fixtures); the file formats are described there. */
interface EventCase {
  name: string;
  expect: "known" | "unknown" | "degraded" | "invalid";
  event: { type?: unknown; data?: unknown };
  normalized?: unknown;
}

interface MethodCase {
  method: string;
  params: unknown;
  result?: unknown;
}

interface Ref {
  $ref: string;
}

interface CatalogEntry {
  params: Ref;
  /** A `command` method's is `allOf: [<result>, Replayed]` (design 5.2.5). */
  result?: Ref | { allOf: Ref[] };
}

const read = (path: string): unknown =>
  JSON.parse(readFileSync(new URL(`../../../protocol/${path}`, import.meta.url), "utf8"));

const events = read("fixtures/events.json") as EventCase[];
const methods = read("fixtures/methods.json") as MethodCase[];
const catalog = (
  read("schema/v1.json") as { "x-methods": Record<string, Record<string, CatalogEntry>> }
)["x-methods"];

function schemaOf(ref: string): z.ZodType {
  const name = `z${ref.replace("#/$defs/", "")}`;
  const schema = (schemas as Record<string, z.ZodType | undefined>)[name];
  if (schema === undefined) {
    throw new Error(`no ${name} for ${ref}`);
  }
  return schema;
}

describe("UNIT-ts-fixtures", () => {
  // Unknown types, unknown enum values and unknown union members parse, and
  // so does a known type with malformed data: as an unknown event, which
  // `isKnownEvent` tells apart (ADR-04, revision 4).
  test.each(events)("event: $name", ({ expect: kind, event, normalized }) => {
    const parsed = zEvent.safeParse(event);
    if (kind === "invalid") {
      expect(parsed.success).toBe(false);
      return;
    }
    if (!parsed.success) {
      throw parsed.error;
    }
    expect(parsed.data).toEqual(normalized ?? event);
    expect(isKnownEvent(parsed.data)).toBe(kind === "known");
    const { type, data } = event;
    const body = zEventBody.parse(data === undefined ? { type } : { type, data });
    expect(isKnownEvent(body)).toBe(kind === "known");
  });

  test.each(methods)("method: $method", ({ method, params, result }) => {
    const entry = Object.values(catalog).find((table) => method in table)?.[method];
    if (entry === undefined) {
      throw new Error(`${method} is not in the catalog`);
    }
    expect(schemaOf(entry.params.$ref).parse(params)).toEqual(params);
    if (entry.result !== undefined) {
      const [first, ...more] = "allOf" in entry.result ? entry.result.allOf : [entry.result];
      expect(first && schemaOf(first.$ref).parse(result)).toEqual(result);
      for (const { $ref } of more) {
        const replayed = { ...(result as object), replayed: true };
        expect(schemaOf($ref).safeParse(replayed).success).toBe(true);
      }
    }
  });

  test("every known event type has a sample", () => {
    const known = events.filter((c) => c.expect === "known").map((c) => c.event.type);
    expect(new Set(known)).toEqual(new Set(KNOWN_EVENT_TYPES));
  });

  test("a known event narrows by type, and open enums keep their known values", () => {
    const sample = events.find((c) => c.name === "item.delta");
    const event = zEvent.parse(sample?.event);
    if (!isKnownEvent(event)) {
      throw new Error("item.delta is a known type");
    }
    // On `Event` this does not compile: its fallback member has `type: string`.
    expect(event.type === "item.delta" ? event.data.text : undefined).toBe("ok\n");

    // `never`, and so a compile error, if `(string & {})` lost to `string`.
    const completed: Extract<ItemStatus, "completed"> = "completed";
    const future: ItemStatus = "paused";
    const known: KnownEventType = "item.delta";
    // @ts-expect-error: a type this version does not know is no `KnownEventType`.
    const unknown: KnownEventType = "project.pinned";
    expect([completed, future, known, unknown]).toHaveLength(4);
  });
});
