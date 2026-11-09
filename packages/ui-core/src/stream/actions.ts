import { createContext, use } from "react";

/** What a row can ask of the stream list. */
export const StreamActions = createContext<{
  sessionId?: string;
  loadEarlier: () => void;
  /** Folds or opens a completed turn. */
  toggleTurn: (turnId: string) => void;
  /** Scrolls to the row of an item, opening its turn first. */
  jumpTo: (itemId: string) => void;
}>({ loadEarlier: () => undefined, toggleTurn: () => undefined, jumpTo: () => undefined });

export const useStreamActions = () => use(StreamActions);
