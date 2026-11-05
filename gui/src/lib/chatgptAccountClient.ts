import { invoke } from "@tauri-apps/api/core";
import type {
  WorkspaceAccountState,
  WorkspaceModelCatalog,
  WorkspaceRateLimits,
} from "./workbenchTypes";
export interface AccountStatus {
  account: WorkspaceAccountState;
  epoch: number;
  version: string;
  loginInProgress: boolean;
  pendingLogin?: { loginId: string; epoch: number } | null;
  existingAccounts?: Array<{
    source: string;
    label: string;
    email: string | null;
  }>;
  error?: string | null;
}

export const chatgptAccountClient = {
  status: () => invoke<AccountStatus>("chatgpt_account_status"),
  selectExisting: (args: { source: string }) =>
    invoke<unknown>("chatgpt_select_existing_account", args),
  loginStart: () => invoke<unknown>("chatgpt_login_start"),
  loginCancel: (args: { loginId?: string; epoch?: number }) =>
    invoke<unknown>("chatgpt_login_cancel", args),
  logout: () => invoke<unknown>("chatgpt_logout"),
  models: () => invoke<WorkspaceModelCatalog>("chatgpt_model_catalog"),
  limits: () => invoke<WorkspaceRateLimits>("chatgpt_rate_limits"),
};
