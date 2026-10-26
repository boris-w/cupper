// The ShellAPI frozen in M1a (design 5.12.5): `../api-m1a.d.ts` is its
// snapshot. Members and union branches may be added; nothing is removed or
// changed. Every frozen member is still there, at every depth, and takes
// what it took; every frozen union's branches are still values of today's.
// A type error here is a breaking change, which takes a new major version
// (ADR-04).

import type * as M1a from "../api-m1a";
import type * as Now from "./shell-api";

type Holds<T extends true> = T;
type Fits<A, B> = [A] extends [B] ? true : false;
type Fn = (...args: never[]) => unknown;

/** A function's arguments, its callbacks left out: what may be passed to it. */
type Args<F> = F extends (...args: infer A) => unknown
  ? { [I in keyof A]: A[I] extends Fn ? unknown : A[I] }
  : never;

/**
 * Every member of `Old` still in `Now`, at every depth; old calls still
 * type. Arrays and the protocol's types (`Handshake`) are the protocol
 * freeze's to check.
 */
type Kept<Old, Now> = [Old] extends [Fn]
  ? [Now] extends [Fn]
    ? Fits<Args<Old>, Args<Now>> extends true
      ? Kept<Awaited<ReturnType<Old>>, Awaited<ReturnType<Now>>>
      : false
    : false
  : [Old] extends [readonly unknown[]]
    ? true
    : [Old] extends [object]
      ? [Exclude<keyof Old, keyof Now>] extends [never]
        ? false extends { [K in keyof Old & keyof Now]: Kept<Old[K], Now[K]> }[keyof Old &
            keyof Now]
          ? false
          : true
        : false
      : true;

export type FrozenInM1a = [
  Holds<Kept<M1a.ShellAPI, Now.ShellAPI>>,
  Holds<Kept<M1a.ShellInfo, Now.ShellInfo>>,
  Holds<Kept<M1a.HostPort, Now.HostPort>>,
  Holds<Kept<M1a.ShellCommand, Now.ShellCommand>>,
  Holds<Fits<M1a.Theme, Now.Theme>>,
  Holds<Fits<M1a.KnownCommandId, Now.KnownCommandId>>,
  Holds<Fits<M1a.KnownNavigateTarget, Now.KnownNavigateTarget>>,
  Holds<Fits<M1a.UpdateState, Now.UpdateState>>,
  Holds<Fits<M1a.KnownConnectionFailure, Now.KnownConnectionFailure>>,
  Holds<Fits<M1a.ConnectionState, Now.ConnectionState>>,
  Holds<Fits<M1a.PrivilegedAction, Now.PrivilegedAction>>,
  Holds<Fits<M1a.PrivilegedResult, Now.PrivilegedResult>>,
];
