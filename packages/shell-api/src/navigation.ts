import type { KnownNavigateTarget, NavigateTarget } from "./shell-api";

const KNOWN_KINDS: Record<KnownNavigateTarget["kind"], true> = { session: true, settings: true };

/** Whether this build knows the target's kind; the UI ignores the others. */
export function isKnownTarget(target: NavigateTarget): target is KnownNavigateTarget {
  return Object.hasOwn(KNOWN_KINDS, target.kind);
}
