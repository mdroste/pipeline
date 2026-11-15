/** App-owned Workspace records. */

export interface HarnessModule {
  id: string;
  kind:
    "instruction_pack" | "context_provider" | "tool" | "inspector" | "recipe";
  version: number;
  name: string;
  description: string;
  requiresWorkspace: boolean;
  capability: string;
}

export type BasePromptUpdate =
  { mode: "codexDefault" } | { mode: "replace"; text: string };

export interface NativePromptCatalog {
  installedVersion: string | null;
  sources: Array<{
    path: string;
    origin: "pipeline" | "codex";
    clientVersion: string | null;
    fetchedAt: string | null;
    models: Array<{
      model: string;
      template: string;
      templateField: string;
      sections: InstructionSection[];
    }>;
  }>;
  diagnostics: string[];
}

export interface HarnessPreset {
  id: string;
  workspaceId: string | null;
  name: string;
  description: string;
  instructions: string;
  /** Absent/null inherits the native default; a string replaces it. */
  baseInstructions?: string | null;
  modules: string[];
  builtIn: boolean;
  sourcePresetId: string | null;
  revision: number;
}

export interface HarnessCatalog {
  schemaVersion: number;
  toolCatalogVersion: number;
  modules: HarnessModule[];
  presets: HarnessPreset[];
  /** Pipeline-owned prompt layers; Codex base instructions are runtime-managed. */
  promptLayers?: InstructionSection[];
}

export interface EffectiveHarness {
  schemaVersion: number;
  sessionId: string;
  workspaceId: string | null;
  preset: HarnessPreset;
  mode: "inspect" | "edit";
  fileMode?: "review" | "direct";
  webSearch: boolean;
  commandNetwork: boolean;
  capabilities?: {
    commandDomains: string[];
    shellWrite: boolean;
    privateFolder?: boolean;
  };
  permissionProfile: string;
  contextBudgetBytes: number;
  enabledModules: string[];
  unavailableModules: string[];
  diagnostics: string[];
  developerInstructions: string;
  contextPreview: string;
  contextTruncated: boolean;
  dynamicTools: Array<Record<string, unknown>>;
  fingerprint: string;
  valueSources: Record<string, unknown>;
  /** Derived per-catalog-module availability for this session; absent from older backends. */
  moduleAvailability?: ModuleAvailability[];
  /** Host-owned labelled bands of `developerInstructions`; absent from older backends. */
  instructionSections?: InstructionSection[];
}

export interface ModuleAvailability {
  id: string;
  available: boolean;
  reasons: string[];
}

export interface InstructionSection {
  id: string;
  label: string;
  text: string;
}

export interface WorkspaceConfig {
  scopeKey: string;
  workspaceId: string | null;
  schemaVersion: number;
  body: Record<string, unknown>;
  revision: number;
  updatedAt: string;
}
