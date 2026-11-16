// The Build sheet: what to build, and the approval to run it. Saved settings
// lead with the run step, because a build is run far more often than set up.
import ManuscriptTexTools from "./ManuscriptTexTools";
import type { Dispatch, SetStateAction } from "react";
import { Fold } from "../../ui";
import {
  studioClient,
  type BuildConfig,
  type BuildRecord,
} from "../../lib/studioClient";
import { parseArgvText } from "../../lib/argvText";
import type { ProjectRecord } from "../../lib/projectClient";
import type { ExecutionProfile } from "../../lib/workbench/executionTypes";
import { button, input, Field, Select, Text, lines } from "./shared";
import ManuscriptBuildReview from "./ManuscriptBuildReview";

export const initialBuild: BuildConfig = {
  name: "Manuscript build",
  checkpointId: null,
  directory: ".",
  rootDocument: "main.tex",
  expectedPdf: "main.pdf",
  engine: "latexmk",
  automaticDependencies: true,
  executable: null,
  timeoutSeconds: 120,
  inputs: ["main.tex"],
  advancedArgv: null,
};
/** Start with the root only; the backend discovers dependencies before each build. */
export function suggestBuild(path: string): BuildConfig {
  const slash = path.lastIndexOf("/");
  const directory = slash < 0 ? "." : path.slice(0, slash);
  const prefix = slash < 0 ? "" : `${directory}/`;
  const rootDocument = path.slice(prefix.length);
  const expectedPdf = rootDocument.replace(/\.[^.]*$/, "") + ".pdf";
  return {
    ...initialBuild,
    directory,
    rootDocument,
    expectedPdf,
    inputs: [rootDocument],
  };
}

export default function ManuscriptBuildSheet({
  documentId,
  workspaceId,
  busy,
  run,
  configs,
  configId,
  setConfigId,
  config,
  setConfig,
  advanced,
  setAdvanced,
  workingCopies,
  profile,
  onSaved,
  onQueued,
  readyToBuild,
}: {
  readyToBuild: boolean;
  documentId?: string | null;
  workspaceId: string;
  busy: boolean;
  run: (work: () => Promise<unknown>) => Promise<void>;
  configs: ProjectRecord<BuildRecord>[];
  configId: string;
  setConfigId: (id: string) => void;
  config: BuildConfig;
  setConfig: Dispatch<SetStateAction<BuildConfig>>;
  advanced: string;
  setAdvanced: (value: string) => void;
  workingCopies: { value: string; label: string }[];
  profile: ExecutionProfile | null;
  onSaved: () => Promise<void>;
  onQueued: () => void;
}) {
  const chosen = configs.find((r) => r.id === configId);
  const settings = (
    <>
      <div className="grid grid-cols-2 gap-3">
        <Text
          label="Root TeX document"
          value={config.rootDocument}
          onChange={(rootDocument) =>
            setConfig((c) => ({ ...c, rootDocument }))
          }
        />
        <Select
          label="Engine"
          value={config.engine}
          onChange={(engine) =>
            setConfig((c) => ({ ...c, engine, executable: null }))
          }
          options={["pdflatex", "xelatex", "lualatex", "latexmk"].map((v) => ({
            value: v,
            label: v,
          }))}
        />
      </div>
      <ManuscriptTexTools
        workspaceId={workspaceId}
        config={config}
        setConfig={setConfig}
        saved={Boolean(chosen)}
      />
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={config.automaticDependencies ?? false}
          onChange={(e) =>
            setConfig((c) => ({
              ...c,
              automaticDependencies: e.target.checked,
            }))
          }
        />
        Track dependencies automatically
      </label>
      <p className="text-xs text-ink-muted">
        {config.automaticDependencies
          ? "Sources, bibliographies, and figures are tracked before each build."
          : "List every file used by this manuscript in More build settings."}
      </p>
      <Fold title="More build settings">
        <Text
          label="Additional inputs (one per line, relative to build directory)"
          multiline
          value={config.inputs.join("\n")}
          onChange={(s) => setConfig((c) => ({ ...c, inputs: s.split("\n") }))}
        />
        {configs.length > 0 && (
          <Select
            label="Saved configuration"
            value={configId}
            onChange={(id) => {
              setConfigId(id);
              const found = configs.find((c) => c.id === id);
              const selected = found?.body.config ?? initialBuild;
              setConfig(documentId ? { ...selected, documentId } : selected);
              setAdvanced(
                found?.body.config.advancedArgv
                  ? JSON.stringify(found.body.config.advancedArgv, null, 2)
                  : "",
              );
            }}
            placeholder="New build configuration"
            options={configs.map((c) => ({
              value: c.id,
              label: c.body.config.name,
            }))}
          />
        )}
        <Text
          label="Build name"
          value={config.name}
          onChange={(name) => setConfig((c) => ({ ...c, name }))}
        />
        {workingCopies.length > 0 && (
          <Select
            label="Build in"
            value={config.checkpointId ?? ""}
            onChange={(checkpointId) =>
              setConfig((c) => ({ ...c, checkpointId: checkpointId || null }))
            }
            placeholder="Project working copy"
            options={workingCopies}
          />
        )}
        <div className="grid grid-cols-2 gap-3">
          <Text
            label="Working directory (relative)"
            value={config.directory}
            onChange={(directory) => setConfig((c) => ({ ...c, directory }))}
          />
          <Text
            label="Expected PDF"
            value={config.expectedPdf}
            onChange={(expectedPdf) =>
              setConfig((c) => ({ ...c, expectedPdf }))
            }
          />
        </div>
        <Field label="Timeout (seconds)">
          <input
            className={input}
            type="number"
            min={1}
            max={7200}
            value={config.timeoutSeconds}
            onChange={(e) =>
              setConfig((c) => ({
                ...c,
                timeoutSeconds: Number(e.target.value),
              }))
            }
          />
        </Field>
        <Text
          label="Command arguments as JSON (optional)"
          multiline
          value={advanced}
          onChange={setAdvanced}
        />
      </Fold>
      <button
        className={button}
        disabled={busy}
        onClick={() =>
          void run(async () => {
            const saved = await studioClient.mutate<ProjectRecord<BuildRecord>>(
              workspaceId,
              {
                action: "saveBuild",
                id: configId || null,
                expectedRevision: chosen?.revision ?? 0,
                config: {
                  ...config,
                  ...(documentId ? { documentId } : {}),
                  inputs: lines(config.inputs.join("\n")),
                  advancedArgv: advanced.trim()
                    ? parseArgvText(advanced)
                    : null,
                },
              },
            );
            setConfigId(saved.id);
            setConfig(saved.body.config);
            setAdvanced(
              saved.body.config.advancedArgv
                ? JSON.stringify(saved.body.config.advancedArgv, null, 2)
                : "",
            );
            await onSaved();
          })
        }
      >
        Save build settings
      </button>
    </>
  );
  return (
    <div className="manuscript-sheet">
      {profile ? (
        <>
          <p className="text-xs text-ink-muted">
            {config.engine} · {config.rootDocument}
          </p>
          {readyToBuild &&
          JSON.stringify(config) === JSON.stringify(chosen?.body.config) &&
          advanced ===
            (config.advancedArgv
              ? JSON.stringify(config.advancedArgv, null, 2)
              : "") ? (
            <ManuscriptBuildReview
              key={`${profile.id}:${profile.revision}`}
              profile={profile}
              onQueued={onQueued}
            />
          ) : (
            <p className="text-sm">
              Save build settings, then close this panel and choose Build to
              save manuscript drafts before running.
            </p>
          )}
          <Fold title="Build settings">{settings}</Fold>
        </>
      ) : (
        settings
      )}
    </div>
  );
}
