import type { ComponentProps } from "react";
import type { ConversationSnapshot } from "../../lib/workbench/sessionTypes";
import type { ChatApprovalMode } from "../../lib/workbench/runtimeTypes";
import { runtimeClient } from "../../lib/workbench/runtimeClient";
import WorkspaceComposerControls, {
  loadAccountSummary,
  loadClaudeUsage,
} from "../WorkspaceComposerControls";

type Controls = ComponentProps<typeof WorkspaceComposerControls>;

interface Props extends Pick<
  Controls,
  "models" | "model" | "effort" | "runtime" | "disabled"
> {
  snapshot: ConversationSnapshot | null;
  setRuntime: (runtime: NonNullable<Controls["runtime"]>) => void;
  setModel: (model: string) => void;
  setEffort: (effort: string) => void;
  saveSelection: (
    model: string,
    effort: string,
    runtime?: Controls["runtime"],
    approvalMode?: ChatApprovalMode,
  ) => void;
}

export default function WorkspaceComposerToolbar({
  models,
  model,
  effort,
  snapshot,
  disabled,
  runtime,
  setRuntime,
  setModel,
  setEffort,
  saveSelection,
}: Props) {
  return (
    <WorkspaceComposerControls
      models={models}
      model={model}
      effort={effort}
      disabled={disabled}
      runtime={runtime}
      approvalMode={
        snapshot?.session.overrides.approvalMode === "ask" ||
        snapshot?.session.overrides.approvalMode === "auto" ||
        snapshot?.session.overrides.approvalMode === "sandbox"
          ? snapshot.session.overrides.approvalMode
          : runtime === "claude"
            ? "sandbox"
            : "ask"
      }
      onApprovalModeChange={(mode) =>
        saveSelection(model, effort, runtime, mode)
      }
      onClearApprovals={
        snapshot
          ? () => runtimeClient.clearChatApprovals(snapshot.session.id)
          : undefined
      }
      loadAccount={runtime === "codex" ? loadAccountSummary : undefined}
      loadClaudeUsage={runtime === "claude" ? loadClaudeUsage : undefined}
      onRuntimeChange={(next) => {
        setRuntime(next);
        setModel("");
        setEffort("");
        saveSelection("", "", next);
      }}
      onChange={(nextModel, nextEffort) => {
        setModel(nextModel);
        setEffort(nextEffort);
        saveSelection(nextModel, nextEffort);
      }}
    />
  );
}
