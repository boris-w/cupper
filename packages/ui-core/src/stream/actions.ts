import { createContext, use } from "react";

/** What a row can ask of the stream list. */
export const StreamActions = createContext<{ loadEarlier: () => void }>({
  loadEarlier: () => undefined,
});

export const useStreamActions = () => use(StreamActions);
