// The message stream's row types (design 5.7.5): turn headers, user and agent
// messages, activity groups, tool rows and so on. T082, T083 and later tasks
// register their components; a kind nobody registered shows a placeholder.

import type { ComponentType } from "react";

import { useT } from "../app/providers";

/** A row of the stream; each kind carries its own data. */
export interface Row {
  kind: string;
}

export type RowComponent<R extends Row = Row> = ComponentType<{ row: R }>;

export class RowRegistry<R extends Row = Row> {
  readonly #components = new Map<string, RowComponent<R>>();

  /** Registers the component of `kind`; a second registration of a kind is a bug and throws. */
  register(kind: R["kind"], component: RowComponent<R>): void {
    if (this.#components.has(kind)) {
      throw new Error(`the row kind ${kind} is registered already`);
    }
    this.#components.set(kind, component);
  }

  /** The component of `kind`, or the placeholder for a kind this build does not know. */
  resolve(kind: string): RowComponent<R> {
    return this.#components.get(kind) ?? UnsupportedRow;
  }
}

/** Says this version cannot show the row; its data never reaches the screen. */
export function UnsupportedRow() {
  const t = useT();
  return <div className="text-sm text-fg-tertiary">{t("stream:unsupported_row")}</div>;
}

/** Draws `row` with the component its kind is registered with. */
export function RowView<R extends Row>({ registry, row }: { registry: RowRegistry<R>; row: R }) {
  const Component = registry.resolve(row.kind);
  return <Component row={row} />;
}
