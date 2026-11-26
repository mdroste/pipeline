import { createContext, useContext } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { appClient } from "../lib/appClient";
import { settingsClient } from "../lib/settingsClient";
import { workflowProfileClient } from "../lib/workflowProfileClient";
import { onFileDrop } from "../lib/fileDrop";

/** Scoped services let the real setup run on sample data without patching clients. */
const liveEnvironment = {
  loadSettings: () => settingsClient.load(),
  modelCatalog: (...args: Parameters<typeof settingsClient.modelCatalog>) =>
    settingsClient.modelCatalog(...args),
  modelRoster: (...args: Parameters<typeof appClient.modelRoster>) =>
    appClient.modelRoster(...args),
  pdfOptions: (...args: Parameters<typeof appClient.reviewPdfOptions>) =>
    appClient.reviewPdfOptions(...args),
  switchWorkflow: (...args: Parameters<typeof workflowProfileClient.switch>) =>
    workflowProfileClient.switch(...args),
  chooseFiles: open,
  inputFiles: (...args: Parameters<typeof appClient.inputFiles>) =>
    appClient.inputFiles(...args),
  onFileDrop,
};
export type ReviewSetupEnvironmentValue = typeof liveEnvironment;
export const ReviewSetupEnvironment = createContext(liveEnvironment);
export const useReviewSetupEnvironment = () =>
  useContext(ReviewSetupEnvironment);
