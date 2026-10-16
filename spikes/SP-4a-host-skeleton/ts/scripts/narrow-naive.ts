// Deliberately naive narrowing, kept out of tsconfig. `npx tsc --ignoreConfig --noEmit
// --strict --allowImportingTsExtensions scripts/narrow-naive.ts` shows that
// `e.type === 'item.delta'` does not exclude the Unknown fallback.
import type { EventBody } from '../src/gen/types.gen.ts';

export function text(e: EventBody): string | undefined {
  if (e.type === 'item.delta') {
    return e.data.text;
  }
  return undefined;
}
