import type { IndexView as IndexViewJson, KnownEventBody } from "@cupper/protocol";

import { missing, otherStream, StreamView } from "./view";

/**
 * The index stream's view: projects, session summaries, agents, the host,
 * connected clients and pending elevations. A later `host`, `agents` or
 * `clients` update replaces the earlier one; removing what is not there is
 * an error; `notification.intent` is a one-off signal, never in the view.
 */
export class IndexView extends StreamView<IndexViewJson> {
  protected empty(): IndexViewJson {
    return { projects: {}, sessions: {}, elevations: {} };
  }

  protected fold(body: KnownEventBody): void {
    switch (body.type) {
      case "host.updated":
        return this.#set("host", body.data);
      case "agents.updated":
        return this.#set("agents", body.data);
      case "clients.updated":
        return this.#set("clients", body.data);
      case "project.upserted":
        return this.#upsert("projects", "project", body.data.project_id, body.data);
      case "session.upserted":
        return this.#upsert("sessions", "session", body.data.session_id, body.data);
      case "elevation.requested":
        return this.#upsert("elevations", "elevation", body.data.elevation_id, body.data);
      case "project.removed":
        return this.#remove("projects", "project", body.data.project_id);
      case "session.removed":
        return this.#remove("sessions", "session", body.data.session_id);
      case "elevation.resolved":
        return this.#remove("elevations", "elevation", body.data.elevation_id);
      case "notification.intent":
        return;
      default:
        throw otherStream(body.type);
    }
  }

  #set<K extends "host" | "agents" | "clients">(key: K, value: IndexViewJson[K]): void {
    this.state[key] = value;
    this.versions.touch(key);
  }

  #upsert(map: Collection, key: string, id: string, value: unknown): void {
    const added = !Object.hasOwn(this.state[map], id);
    (this.state[map] as Record<string, unknown>)[id] = value;
    this.versions.touch(`${key}:${id}`, ...(added ? [map] : []));
  }

  #remove(map: Collection, key: string, id: string): void {
    if (!Object.hasOwn(this.state[map], id)) {
      throw missing(id);
    }
    Reflect.deleteProperty(this.state[map], id);
    this.versions.touch(`${key}:${id}`, map);
  }
}

type Collection = "projects" | "sessions" | "elevations";
