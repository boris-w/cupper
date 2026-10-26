// js-yaml ships no types; the gate needs `loadAll` only.
declare module "js-yaml" {
  export function loadAll(input: string): unknown[];
}
