// The views of design 5.3.3 and 5.7.6 as T087's StreamSink: the state of a
// stream, folded from its event bodies by the same rules as the Rust views
// (`cupper-protocol::view`, T014's rulings in 5.3.11). Their JSON is the
// schema's `IndexView` and `SessionView`, equal to Rust's.

import {
  isKnownEvent,
  type Event,
  type EventBody,
  type KnownEventBody,
  type RpcError,
} from "@cupper/protocol";

import type { StreamSink } from "../sync";
import { Versions } from "./versions";

/** A body that does not fit the view; the stream starts over (T087). */
export class ViewError extends Error {
  override name = "ViewError";
}

export const missing = (id: string) => new ViewError(`${id} is not in the view`);
export const duplicate = (id: string) => new ViewError(`${id} is already in the view`);
export const otherStream = (type: string) => new ViewError(`${type} belongs to the other stream`);

/** A body this version cannot show, with its event when it came live. */
export interface Placeholder {
  body: EventBody;
  event?: Event;
}

export abstract class StreamView<T> implements StreamSink {
  readonly versions = new Versions();
  /** From `reset` until `live` the view is being rebuilt, and incomplete. */
  complete = false;
  /** Why the latest subscription failed, until the next `live`. */
  error: RpcError | undefined;
  /**
   * Bodies of unknown types, or of known ones with malformed data, for the
   * UI's placeholders (key `unknown`). Like Rust's views, the view ignores them.
   */
  unknown: Placeholder[] = [];
  protected state: T;

  constructor() {
    this.state = this.empty();
  }

  /** The view as JSON, equal to the Rust view's. */
  toJSON(): T {
    return this.state;
  }

  reset(): void {
    this.state = this.empty();
    this.unknown = [];
    this.complete = false;
    this.versions.touchAll();
  }

  /** Throws a `ViewError`, and then has changed nothing. */
  apply(body: EventBody, event?: Event): void {
    if (isKnownEvent(body)) {
      this.fold(body);
    } else {
      this.unknown.push(event ? { body, event } : { body });
      this.versions.touch("unknown");
    }
  }

  live(): void {
    this.complete = true;
    this.error = undefined;
    this.versions.touch();
  }

  failed(error: RpcError): void {
    this.error = error;
    this.versions.touch();
  }

  protected abstract empty(): T;

  protected abstract fold(body: KnownEventBody): void;
}
