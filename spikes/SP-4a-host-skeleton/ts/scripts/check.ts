// Checks the generated TS types and zod validators against the shared
// fixtures (unknown types, enum values and union members) and shows the
// narrowing pattern that works with the Unknown fallback.
import { readFileSync } from 'node:fs';
import type { EventBody, InteractionRequest, ItemStatus } from '../src/gen/types.gen.ts';
import { zEvent, zEventBody, zUnknownEvent } from '../src/gen/zod.gen.ts';

type Fixture = { name: string; known: boolean; event: unknown };
const fixtures: Fixture[] = JSON.parse(readFileSync(new URL('../../protocol/fixtures/events.json', import.meta.url), 'utf8'));

// Narrowing on `type` alone does not exclude the fallback (its `type` is
// `string`), and `Exclude<EventBody, UnknownEvent>` is `never` (every known
// member is assignable to the fallback). What works: `Extract` on a literal
// list of the known types, plus a guard that validates against the known
// members, because a known type with malformed data is also "unknown".
const KNOWN = [
  'session.upsert', 'turn.started', 'turn.completed', 'item.started', 'item.delta',
  'item.updated', 'item.completed', 'interaction.requested', 'interaction.resolved', 'agent.notice',
] as const;
type Known = Extract<EventBody, { type: (typeof KNOWN)[number] }>;

function isKnown(e: EventBody): e is Known {
  return zEventBody.options.some((s) => s !== zUnknownEvent && s.safeParse(e).success);
}

function describe(e: EventBody): string {
  if (!isKnown(e)) return `unknown(${e.type})`;
  switch (e.type) {
    case 'item.delta':
      return `delta ${e.data.item_id} +${e.data.text.length}`;
    case 'item.completed': {
      const s: ItemStatus = e.data.status;
      return `completed ${e.data.item_id} ${s}`;
    }
    case 'interaction.requested': {
      const r: InteractionRequest = e.data.request;
      return `request ${r.type}`;
    }
    default:
      return e.type;
  }
}

let failures = 0;
for (const f of fixtures) {
  const parsed = zEvent.safeParse(f.event);
  if (!parsed.success) {
    failures++;
    console.log(`FAIL parse: ${f.name}: ${parsed.error.message}`);
    continue;
  }
  const known = isKnown(parsed.data as EventBody);
  const ok = known === f.known;
  if (!ok) failures++;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${f.name}: ${describe(parsed.data as EventBody)}`);
}

// What zod keeps of an unknown union member: the generated fallback is a plain
// z.object, which strips everything but `type`.
const req = zEvent.parse(fixtures[3].event) as { data: { request: Record<string, unknown> } };
console.log(`unknown union member after zod: ${JSON.stringify(req.data.request)}`);

if (failures > 0) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
console.log(`all ${fixtures.length} fixtures ok`);
