import { useEffect } from "react";
import { vi, type Mock } from "vitest";
import { queuedRun } from "./queuedRunFixture";

export interface AppTestMocks {
  saveCollection: Mock;
  invoke: Mock;
  enqueue: Mock;
  enqueueBatch: Mock;
  onCloseRequested: Mock;
  setWindowTheme: Mock;
  listen: Mock;
  confirmDialog: Mock;
  notify: Mock;
  runQueue: { limit: number; runs: ReturnType<typeof queuedRun>[] };
  workflowInputMode: { value: string };
}
export interface AppTestAppearance {
  systemIsDark: boolean;
  systemThemeListener: ((event: MediaQueryListEvent) => void) | undefined;
}
export function createAppTestFixture(
  mocks: AppTestMocks,
  appearance: AppTestAppearance,
) {
  const {
    saveCollection,
    invoke,
    runQueue,
    enqueue,
    enqueueBatch,
    onCloseRequested,
    setWindowTheme,
    listen,
    confirmDialog,
    notify,
    workflowInputMode,
  } = mocks;
  const previewConfig = {
    steps: [],
    merge: { enabled: false, prompt: "", agents: [] },
    context_cache: { enabled: false },
    use_orientation: true,
    orientation_prompt: "",
    extraction: { method: "", input_mode: "document", extra_inputs: [] },
    parallel_context_template: "{step_prompt}",
    variables: [],
  };

  function runSetup(
    profileId = "deep-review",
    profileConfigSnapshotId = "config-test",
  ) {
    return {
      profileId,
      profileConfigSnapshotId,
      inputMode: workflowInputMode.value,
      variables: [],
      inputSlots: [],
    };
  }

  const reset = () => {
    saveCollection.mockReset().mockResolvedValue(true);
    invoke.mockReset();
    listen.mockReset();
    confirmDialog.mockReset();
    notify.mockReset();
    listen.mockResolvedValue(vi.fn());
    runQueue.runs = [];
    const add = (path: string) => {
      const key = (path.split("/").pop() ?? path).replace(/\.\w+$/, "");
      return runQueue.runs[runQueue.runs.push(queuedRun(key)) - 1];
    };
    enqueue.mockReset().mockImplementation(async (r) => add(r.paperPath));
    enqueueBatch.mockReset().mockImplementation(async (r) => r.paths.map(add));
    onCloseRequested.mockReset();
    onCloseRequested.mockResolvedValue(vi.fn());
    setWindowTheme.mockReset();
    setWindowTheme.mockResolvedValue(undefined);
    workflowInputMode.value = "document";
    appearance.systemIsDark = false;
    appearance.systemThemeListener = undefined;
    localStorage.clear();
    localStorage.setItem("pipeline.ui.page", "main");
    document.documentElement.classList.remove("dark", "theme-switching");
    document.documentElement.style.colorScheme = "";
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: vi.fn(() => ({
        matches: appearance.systemIsDark,
        addEventListener: (
          _event: string,
          listener: (event: MediaQueryListEvent) => void,
        ) => {
          appearance.systemThemeListener = listener;
        },
        removeEventListener: (
          _event: string,
          listener: (event: MediaQueryListEvent) => void,
        ) => {
          if (appearance.systemThemeListener === listener)
            appearance.systemThemeListener = undefined;
        },
      })),
    });
    invoke.mockImplementation((command: string) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_pipeline_config")
        return Promise.resolve(previewConfig);
      if (command === "get_run_setup") return Promise.resolve(runSetup());
      if (command === "get_execution_plan") {
        return Promise.resolve({
          profileId: "deep-review",
          profileConfigSnapshotId: "config-test",
          profileSnapshotId: "snapshot-test",
          inputMode: workflowInputMode.value,
          variables: [],
          inputSlots: [],
          readiness: { deps: [], ready: true },
          stages: [
            {
              id: "extracting",
              kind: "extracting",
              label: "Extract input",
              stepIds: [],
            },
            { id: "done", kind: "done", label: "Complete", stepIds: [] },
          ],
        });
      }
      if (command === "start_batch") return Promise.resolve();
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
  };
  return { previewConfig, runSetup, reset };
}

export function createAppComponentMocks({
  saveCollection,
  workflowInputMode,
}: Pick<AppTestMocks, "saveCollection" | "workflowInputMode">) {
  return {
    PaperSelector: {
      default: ({
        inputMode,
        onPathChange,
        onSelectionChange,
      }: {
        inputMode: string;
        onPathChange: (path: string) => void;
        onSelectionChange?: (selection: {
          paths: string[];
          interpretation: "document" | "batch";
          selectionKind: "file";
        }) => void;
      }) => (
        <>
          <span>Primary mode: {inputMode}</span>
          <button
            onClick={() => {
              onPathChange("/tmp/test-paper.pdf");
              onSelectionChange?.({
                paths: ["/tmp/test-paper.pdf"],
                interpretation: "document",
                selectionKind: "file",
              });
            }}
          >
            Choose test paper
          </button>
          <button
            onClick={() => {
              onPathChange("/tmp/test-paper.docx");
              onSelectionChange?.({
                paths: ["/tmp/test-paper.docx"],
                interpretation: "document",
                selectionKind: "file",
              });
            }}
          >
            Choose DOCX
          </button>
          <button
            onClick={() => {
              onPathChange("/tmp/a.pdf");
              onSelectionChange?.({
                paths: ["/tmp/a.pdf", "/tmp/b.pdf"],
                interpretation: "batch",
                selectionKind: "file",
              });
            }}
          >
            Choose two papers
          </button>
        </>
      ),
    },
    WorkflowPanel: {
      default: () => <div>Test workflow</div>,
    },
    RunParallelAgents: {
      default: ({
        onChange,
      }: {
        onChange: (value: {
          agents: string[];
          model_overrides: Record<string, never>;
          effort_overrides: Record<string, never>;
        }) => void;
      }) => (
        <button
          onClick={() =>
            onChange({
              agents: ["codex"],
              model_overrides: {},
              effort_overrides: {},
            })
          }
        >
          Change report agents
        </button>
      ),
    },
    ProjectsPage: {
      default: ({
        onSaveHandlerChange,
        onDirtyChange,
      }: {
        onSaveHandlerChange: (save: (() => Promise<boolean>) | null) => void;
        onDirtyChange: (dirty: boolean) => void;
      }) => {
        useEffect(() => {
          onSaveHandlerChange(saveCollection);
          return () => {
            onSaveHandlerChange(null);
            onDirtyChange(false);
          };
        }, [onSaveHandlerChange, onDirtyChange]);
        return (
          <button onClick={() => onDirtyChange(true)}>Edit collection</button>
        );
      },
    },
    HistoryPage: {
      default: () => <div>History workspace</div>,
    },
    BatchPanel: {
      default: ({
        preloadedSetup,
      }: {
        preloadedSetup?: { profileConfigSnapshotId?: string } | null;
      }) => (
        <div>
          Batch workspace ·{" "}
          {preloadedSetup?.profileConfigSnapshotId ?? "no setup"}
        </div>
      ),
    },
    PipelinePage: {
      default: ({
        onClose,
        onDirtyChange,
        onOpenGallery,
        onProfileChange,
      }: {
        onClose: () => void;
        onDirtyChange?: (dirty: boolean) => void;
        onOpenGallery?: () => void;
        onProfileChange?: (config?: {
          extraction: { input_mode: string };
        }) => void;
      }) => (
        <div>
          Workflow workspace
          <button onClick={() => onDirtyChange?.(true)}>
            Make workflow dirty
          </button>
          <button
            onClick={() => {
              workflowInputMode.value = "document";
              onProfileChange?.({ extraction: { input_mode: "document" } });
            }}
          >
            Save document workflow
          </button>
          <button
            onClick={() => {
              workflowInputMode.value = "folder";
              onProfileChange?.({ extraction: { input_mode: "folder" } });
            }}
          >
            Switch to folder workflow
          </button>
          <button onClick={onOpenGallery}>Gallery</button>
          <button onClick={onClose}>Close workflow editor</button>
        </div>
      ),
    },
    WorkflowGalleryPage: {
      default: () => <div>Gallery workspace</div>,
    },
    SettingsPage: {
      default: ({
        onDirtyChange,
        onSystemChange,
        theme,
        onThemeChange,
        initialSection,
        targetId,
      }: {
        onDirtyChange?: (dirty: boolean) => void;
        onSystemChange?: () => void;
        theme?: "light" | "dark" | "system";
        onThemeChange?: (theme: "light" | "dark" | "system") => void;
        initialSection?: string;
        targetId?: string;
      }) => (
        <div>
          <div>Settings workspace</div>
          <div>Initial settings section: {initialSection}</div>
          <div>Settings target: {targetId ?? "none"}</div>
          <div>Current theme: {theme}</div>
          <button onClick={() => onThemeChange?.("light")}>
            Use light theme
          </button>
          <button onClick={() => onThemeChange?.("dark")}>
            Use dark theme
          </button>
          <button onClick={() => onThemeChange?.("system")}>
            Use system theme
          </button>
          <button onClick={() => onDirtyChange?.(true)}>
            Make settings dirty
          </button>
          <button onClick={() => onSystemChange?.()}>
            Save mocked settings
          </button>
        </div>
      ),
    },
    ResearchActivity: {
      default: ({ onProject }: { onProject: (id: string) => void }) => (
        <div role="dialog" aria-label="Research activity">
          <button onClick={() => onProject("activity-project")}>
            Open activity project
          </button>
        </div>
      ),
    },
    UpdateBanner: { default: () => null },
  };
}
