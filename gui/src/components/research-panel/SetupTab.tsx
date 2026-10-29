import { harnessDiagnostic } from "../harness-editor/harnessCopy";
import WorkspaceAssistantPresetForm from "../WorkspaceAssistantPresetForm";
import {
  compactAccessSummary,
  compactModuleSummary,
  describePreset,
} from "../harness-editor/harnessHelpers";
import { conversationClient } from "../../lib/workbench/conversationClient";
import { harnessClient } from "../../lib/workbench/harnessClient";
import { operation } from "./useResearchPanelController";
import type { useResearchPanelController } from "./useResearchPanelController";
import type { ConversationSnapshot } from "../../lib/workbenchTypes";

type Panel = ReturnType<typeof useResearchPanelController>;
type Props = Pick<
  Panel,
  "workspaceId" | "busy" | "act" | "patchSession" | "successor"
> & {
  snapshot: ConversationSnapshot;
  effective: NonNullable<Panel["effective"]>;
  catalog: NonNullable<Panel["catalog"]>;
  settingsDisabled: boolean;
  onEditHarness?: () => void;
};

export default function SetupTab({
  workspaceId,
  busy,
  act,
  patchSession,
  successor,
  snapshot,
  effective,
  catalog,
  settingsDisabled,
  onEditHarness,
}: Props) {
  return (
    <div className="space-y-4">
      <label className="block">
        <span className="text-xs font-medium">
          Agent profile · This conversation
        </span>
        <select
          value={snapshot.session.presetId ?? "plain"}
          disabled={busy || settingsDisabled}
          onChange={(event) =>
            void act(async () => {
              await patchSession({ presetId: event.target.value });
            })
          }
          className="mt-1 w-full rounded border bg-white p-2 dark:bg-neutral-950"
        >
          {catalog.presets.map((preset) => (
            <option key={preset.id} value={preset.id}>
              {preset.name}
              {preset.builtIn ? "" : " · custom"}
            </option>
          ))}
        </select>
      </label>
      <p
        data-testid="harness-summary"
        className="text-xs text-gray-600 dark:text-neutral-300"
      >
        {describePreset(effective.preset, catalog.modules)}
      </p>
      <label className="block">
        <span className="text-xs font-medium">Access mode</span>
        <select
          aria-label="Access mode"
          value={
            typeof snapshot.session.overrides.mode === "string"
              ? snapshot.session.overrides.mode
              : ""
          }
          disabled={busy || settingsDisabled}
          onChange={(event) =>
            void act(async () => {
              const latest = await conversationClient.conversationSnapshot(
                snapshot.session.id,
              );
              const overrides = { ...latest.session.overrides };
              if (event.target.value) overrides.mode = event.target.value;
              else delete overrides.mode;
              await patchSession({ overrides });
            })
          }
          className="mt-1 w-full rounded border bg-white p-2 dark:bg-neutral-950"
        >
          <option value="">
            Use inherited ·{" "}
            {effective.mode === "inspect" ? "Read only" : "Allow edits"}
          </option>
          <option value="inspect">Read only</option>
          <option value="edit">Allow edits</option>
        </select>
      </label>
      <p className="text-[11px] text-gray-500">
        {compactAccessSummary(effective)} · {compactModuleSummary(effective)}
      </p>
      {successor && (
        <p className="rounded border border-blue-200 bg-blue-50 p-2 text-xs text-blue-800 dark:border-blue-900 dark:bg-blue-950/30 dark:text-blue-200">
          Your next message starts a new assistant session with these settings
          and the saved project context. Earlier messages remain visible here.
        </p>
      )}
      {effective.diagnostics.map((item) => (
        <p
          key={item}
          className="rounded border border-amber-200 bg-amber-50 p-2 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/20 dark:text-amber-200"
        >
          {harnessDiagnostic(item)}
        </p>
      ))}
      <WorkspaceAssistantPresetForm
        key={`${effective.preset.id}:${effective.preset.revision}`}
        effective={effective}
        catalog={catalog}
        disabled={busy || settingsDisabled}
        onSave={(name, instructions, modules) =>
          void act(async () => {
            const copy = await harnessClient.clonePreset({
              workspaceId,
              sourcePresetId: effective.preset.id,
              name,
              operationId: operation("assistant-copy"),
            });
            await harnessClient.updatePreset({
              presetId: copy.id,
              expectedRevision: copy.revision,
              name,
              description: effective.preset.description,
              instructions,
              modules,
              operationId: operation("assistant-customize"),
            });
            await patchSession({ presetId: copy.id });
          })
        }
      />
      {onEditHarness && (
        <div className="border-t border-gray-200 pt-3 dark:border-neutral-800">
          <button
            type="button"
            disabled={busy || settingsDisabled}
            onClick={onEditHarness}
            className="rounded border px-3 py-2 text-xs font-medium"
          >
            Manage agent profiles…
          </button>
          <p className="mt-1 text-[11px] text-gray-500">
            Create profiles, view system prompts, and choose allowed tools.
          </p>
        </div>
      )}
    </div>
  );
}
