/**
 * The build flavors of design 5.7.9, picked by `vite build --mode <flavor>`.
 * Vite defines `__CUPPER_FLAVOR__` as a compile-time constant, so code that
 * only test builds may contain sits behind a check of it and is gone from
 * release and local builds.
 */
export type Flavor = "release" | "local" | "e2e" | "dev";

/** Release and local builds share the release hardening (design 5.7.9). */
export function isReleaseLike(flavor: Flavor): boolean {
  return flavor === "release" || flavor === "local";
}
