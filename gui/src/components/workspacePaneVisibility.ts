import { createContext, useContext } from "react";

export const WorkspacePaneVisibility = createContext(true);
export const useWorkspacePaneVisible = () =>
  useContext(WorkspacePaneVisibility);
