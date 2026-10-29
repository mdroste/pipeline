import { invoke } from "@tauri-apps/api/core";
import type {
  HarnessCatalog,
  BasePromptUpdate,
  NativePromptCatalog,
  EffectiveHarness,
  WorkspaceConfig,
  HarnessPreset,
} from "../workbenchTypes";

export const harnessClient = {
  harnessCatalog(workspaceId: string | null) {
    return invoke<HarnessCatalog>("workbench_harness_catalog", { workspaceId });
  },
  effectiveHarness(sessionId: string) {
    return invoke<EffectiveHarness>("workbench_effective_harness", {
      sessionId,
    });
  },
  getWorkspaceConfig(workspaceId: string | null) {
    return invoke<WorkspaceConfig>("workbench_get_workspace_config", {
      workspaceId,
    });
  },
  saveWorkspaceConfig(request: {
    workspaceId: string | null;
    expectedRevision: number;
    body: Record<string, unknown>;
    operationId: string;
  }) {
    return invoke<WorkspaceConfig>("workbench_save_workspace_config", {
      request,
    });
  },
  clonePreset(request: {
    workspaceId: string | null;
    sourceWorkspaceId?: string | null;
    sourcePresetId: string;
    name: string;
    operationId: string;
  }) {
    return invoke<HarnessPreset>("workbench_clone_preset", { request });
  },
  nativePromptCatalog() {
    return invoke<NativePromptCatalog>("workbench_native_prompt_catalog");
  },
  updatePreset(request: {
    basePrompt?: BasePromptUpdate;
    presetId: string;
    expectedRevision: number;
    name: string;
    description: string;
    instructions: string;
    modules: string[];
    operationId: string;
  }) {
    return invoke<HarnessPreset>("workbench_update_preset", { request });
  },
};
