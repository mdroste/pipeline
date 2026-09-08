import AutoReviewCatalogDialog from "./AutoReviewCatalogDialog";

import PromptDialog from "./pipeline-editor/PromptDialog";
import AddStepDialog from "./pipeline-editor/AddStepDialog";
import {
  MergeEditorPanel,
  PipelineSettingsEditorPanel,
  StepEditorPanel,
} from "./pipeline-editor/EditorPanels";
import {
  MemoizedExtractionEditor as ExtractionEditor,
  MemoizedOrientationEditor as OrientationEditor,
} from "./pipeline-editor/PreprocessingEditors";

import {
  AdaptiveAgentsEditorPanel,
  type AdaptiveSlotKind,
} from "./pipeline-editor/AdaptiveReviewEditors";
import {
  adaptiveAgentCount as getAdaptiveAgentCount,
  adaptiveAgentRange as getAdaptiveAgentRange,
  isAutoReview,
  withAdaptiveAgentCount,
} from "../lib/autoReview";

import SchemaEditorPanel from "./pipeline-editor/SchemaEditorPanel";

interface Props {
  onClose: () => void;
  onDirtyChange?: (dirty: boolean) => void;
  onOpenGallery?: () => void;
  onProfileChange?: () => void;
  showBack?: boolean;
}

import { DEFAULT_EXTRACTION } from "./pipeline-editor/editorState";
import { useWorkflowEditor } from "./pipeline-editor/useWorkflowEditor";
import { useProfileOperations } from "./pipeline-editor/useProfileOperations";
import { useStepActions } from "./pipeline-editor/useStepActions";
import { useWorkflowCatalogs } from "./pipeline-editor/useWorkflowCatalogs";
import { useWorkflowShortcuts } from "./pipeline-editor/useWorkflowShortcuts";
import { useWorkflowView } from "./pipeline-editor/useWorkflowView";
import { WorkflowNavigator } from "./pipeline-editor/WorkflowNavigator";
export default function PipelinePage({
  onClose,
  onDirtyChange,
  onOpenGallery,
  onProfileChange,
  showBack = true,
}: Props) {
  const editor = useWorkflowEditor(onDirtyChange);
  const { settings, catalogs } = useWorkflowCatalogs();
  const operations = useProfileOperations(editor, settings, onProfileChange);
  const view = useWorkflowView();
  const {
    addStepOpen,
    setAddStepOpen,
    navigatorView,
    setNavigatorView,
    editorMode,
    catalogTab,
    setCatalogTab,
  } = view;
  const actions = useStepActions(editor, () => {
    setAddStepOpen(false);
    setNavigatorView("steps");
  });
  useWorkflowShortcuts(editor, operations);
  const {
    config,
    activeProfile,
    schemaEditorEpoch,
    editing,
    editingStep,
    conditionStepIds,
    setConfig,
    setDirty,
    setSchemaDraftValid,
  } = editor;
  const { promptResetRequestRef } = editor.identity;
  const { loading, loadError, retryLoad, promptDialog, setPromptDialog } =
    operations;
  const {
    updateStep,
    updateStepPhase,
    updateOutputRole,
    updateMerge,
    updateExtraction,
    resetParallelTemplate,
    addStep,
  } = actions;
  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-gray-400">
        <div className="animate-spin w-6 h-6 border-2 border-gray-300 border-t-gray-600 rounded-full" />
      </div>
    );
  }
  if (loadError || !config) {
    return (
      <div className="flex items-center justify-center h-full p-8">
        <div className="max-w-md text-center space-y-4">
          <h2 className="text-base font-semibold text-gray-800 dark:text-gray-100">
            Pipeline editor could not be loaded
          </h2>
          <p className="text-sm text-red-600 dark:text-red-400 break-words">
            {loadError ?? "The profile configuration was unavailable."}
          </p>
          <div className="flex justify-center gap-2">
            <button
              type="button"
              onClick={retryLoad}
              className="px-3 py-1.5 rounded bg-blue-600 text-white text-sm hover:bg-blue-700"
            >
              Retry
            </button>
            <button
              type="button"
              onClick={onClose}
              className="px-3 py-1.5 rounded border border-gray-300 dark:border-gray-700 text-sm"
            >
              Close
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Group steps for display: find boundaries between parallel and sequential
  const parallelSteps = config.steps.filter((s) => s.phase === "parallel");
  const autoReview = isAutoReview(config);
  const adaptiveAgentCount = getAdaptiveAgentCount(config);
  const adaptiveAgentRange = getAdaptiveAgentRange(config);
  const browseSpecialists = (kind: AdaptiveSlotKind) => {
    setCatalogTab(kind === "subject" ? "subjects" : "methods");
  };

  return (
    <div className="flex h-full relative">
      {/* Prompt dialog */}
      {promptDialog && (
        <PromptDialog
          title={promptDialog.title}
          defaultValue={promptDialog.defaultValue}
          onSubmit={(value) => {
            promptDialog.onSubmit(value);
            setPromptDialog(null);
          }}
          onCancel={() => setPromptDialog(null)}
        />
      )}
      {addStepOpen && (
        <AddStepDialog
          onCreate={addStep}
          onCancel={() => setAddStepOpen(false)}
        />
      )}
      {catalogTab && (
        <AutoReviewCatalogDialog
          initialTab={catalogTab}
          onClose={() => setCatalogTab(null)}
        />
      )}

      {/* Left panel — list */}
      <WorkflowNavigator
        editor={editor}
        operations={operations}
        actions={actions}
        view={view}
        onClose={onClose}
        onOpenGallery={onOpenGallery}
        showBack={showBack}
      />
      {/* Right panel — independently memoizable editors */}
      <div className="flex-1 flex flex-col min-h-0">
        {navigatorView === "schemas" ? (
          <SchemaEditorPanel
            scopeKey={`${activeProfile}:${schemaEditorEpoch}`}
            target={
              editingStep
                ? {
                    kind: "step",
                    step: editingStep,
                    publishesFindings:
                      config.outputs?.findings_step === editingStep.id,
                  }
                : {
                    kind: "orientation",
                    schema: config.orientation_schema,
                    autoReview,
                  }
            }
            onOrientationSchemaChange={(orientation_schema) => {
              setConfig((current) =>
                current ? { ...current, orientation_schema } : current,
              );
              setDirty(true);
            }}
            onStepSchemaChange={(id, output_schema) =>
              updateStep(id, { output_schema })
            }
            onDraftValidityChange={setSchemaDraftValid}
            onBrowseCatalog={setCatalogTab}
          />
        ) : editing === "merge" ? (
          <MergeEditorPanel merge={config.merge} onChange={updateMerge} />
        ) : editing === "extraction" ? (
          <ExtractionEditor
            extraction={config.extraction ?? DEFAULT_EXTRACTION}
            onChange={updateExtraction}
          />
        ) : editing === "orientation" ? (
          <OrientationEditor
            prompt={config.orientation_prompt}
            schema={config.orientation_schema}
            inputMode={config.extraction?.input_mode ?? "document"}
            autoReview={autoReview}
            onPromptChange={(orientation_prompt) => {
              setConfig((current) =>
                current ? { ...current, orientation_prompt } : current,
              );
              setDirty(true);
            }}
            onSchemaChange={(orientation_schema) => {
              setConfig((current) =>
                current ? { ...current, orientation_schema } : current,
              );
              setDirty(true);
            }}
          />
        ) : editing === "auto_adaptive_agents" ? (
          <AdaptiveAgentsEditorPanel
            count={adaptiveAgentCount}
            range={adaptiveAgentRange}
            coreStepLabels={parallelSteps
              .filter((step) => step.enabled)
              .map((step) => step.label)}
            onCountChange={(count) => {
              setConfig(withAdaptiveAgentCount(config, count));
              setDirty(true);
            }}
            onBrowse={browseSpecialists}
          />
        ) : editing === "pipeline_settings" ? (
          <PipelineSettingsEditorPanel
            config={config}
            activeProfile={activeProfile}
            onContextCacheChange={(enabled) => {
              setConfig({ ...config, context_cache: { enabled } });
              setDirty(true);
            }}
            onVariablesChange={(variables) => {
              setConfig({ ...config, variables });
              setDirty(true);
            }}
            onCalibrationAppend={(stepId, text) => {
              setConfig({
                ...config,
                steps: config.steps.map((step) =>
                  step.id === stepId
                    ? { ...step, prompt: `${step.prompt}\n\n${text}` }
                    : step,
                ),
              });
              setDirty(true);
            }}
            onResetParallelTemplate={(source) =>
              void resetParallelTemplate(source)
            }
            onParallelTemplateChange={(parallel_context_template) => {
              promptResetRequestRef.current += 1;
              setConfig({ ...config, parallel_context_template });
              setDirty(true);
            }}
          />
        ) : editingStep ? (
          <StepEditorPanel
            step={editingStep}
            config={config}
            settings={settings}
            catalogs={catalogs}
            conditionStepIds={conditionStepIds}
            advanced={editorMode === "advanced"}
            onUpdate={updateStep}
            onPhaseChange={updateStepPhase}
            onOutputRoleChange={updateOutputRole}
          />
        ) : (
          <div className="flex items-center justify-center h-full text-gray-500 dark:text-gray-400">
            <div className="text-center">
              <p className="text-lg">Select a step to edit</p>
              <p className="text-sm mt-1">
                Click a step on the left to view and edit its prompt
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
