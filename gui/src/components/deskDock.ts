// Where the conversation pane sits beside a project, and the drag that moves
// it. The desk provides the context; the conversation's tab strip drives it.

import { createContext, useContext } from "react";

export type AssistantDock = "left" | "right" | "bottom";

export function assistantDock(value: unknown): AssistantDock | null {
  return value === "left" || value === "right" || value === "bottom"
    ? value
    : null;
}

export interface DeskDock {
  dock: AssistantDock;
  /** A tab dragged off its strip is over (x, y); preview where it would land. */
  move: (x: number, y: number) => void;
  /** The drag ended; on a drop, the pane moves to the previewed side. */
  end: (drop: boolean) => void;
}

export const DeskDockContext = createContext<DeskDock | null>(null);

/** Null where the conversation has no project beside it to dock against. */
export const useDeskDock = () => useContext(DeskDockContext);
