// The Build sheet: what to build, and the approval to run it. Saved settings
// lead with the run step, because a build is run far more often than set up.
import type { Dispatch, SetStateAction } from "react";
import { Fold } from "../../ui";
import {
  studioClient,
  type BuildConfig,
  type BuildRecord,
} from "../../lib/studioClient";
import type { ProjectRecord } from "../../lib/projectClient";
import type { ExecutionProfile } from "../../lib/workbenchTypes";
import { button, input, Field, Select, Text, lines } from "./shared";
import { JobLauncher } from "./Jobs";

export const initialBuild: BuildConfig = {
  name: "Manuscript build",
  checkpointId: null,
  directory: ".",
  rootDocument: "main.tex",
  expectedPdf: "main.pdf",
  engine: "pdflatex",
  timeoutSeconds: 120,
  inputs: ["main.tex"],
  advancedArgv: null,
};
const BUILD_FILES = /\.(tex|bib|bst|sty|cls|png|jpe?g|pdf|eps)$/i;

/** A first build for this file: its folder, its PDF, and the source and figure
 * files beside it, for the researcher to check before saving. */
export function suggestBuild(path: string, files: string[]): BuildConfig {
  const slash = path.lastIndexOf("/");
  const directory = slash < 0 ? "." : path.slice(0, slash);
  const prefix = slash < 0 ? "" : `${directory}/`;
  const rootDocument = path.slice(prefix.length);
  const expectedPdf = rootDocument.replace(/\.[^.]*$/, "") + ".pdf";
  const nearby = files
    .filter((f) => f.startsWith(prefix) && BUILD_FILES.test(f))
    .map((f) => f.slice(prefix.length))
    .filter((f) => f !== expectedPdf);
  return {
    ...initialBuild,
    directory,
    rootDocument,
    expectedPdf,
    inputs: [...new Set([rootDocument, ...nearby])],
  };
}

export default function ManuscriptBuildSheet({
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
}: {
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
          onChange={(engine) => setConfig((c) => ({ ...c, engine }))}
          options={["pdflatex", "xelatex", "lualatex", "latexmk"].map((v) => ({
            value: v,
            label: v,
          }))}
        />
      </div>
      <Text
        label="Declared inputs (one per line, relative to build directory)"
        multiline
        value={config.inputs.join("\n")}
        onChange={(s) => setConfig((c) => ({ ...c, inputs: s.split("\n") }))}
      />
      <p className="text-xs text-ink-muted">
        List all sections, bibliography files, tables, and figures used in the
        build. Pipeline tracks changes only in the files you list.
      </p>
      <Fold title="More build settings">
        {configs.length > 0 && (
          <Select
            label="Saved configuration"
            value={configId}
            onChange={(id) => {
              setConfigId(id);
              const found = configs.find((c) => c.id === id);
              setConfig(found?.body.config ?? initialBuild);
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
                  inputs: lines(config.inputs.join("\n")),
                  advancedArgv: advanced.trim() ? JSON.parse(advanced) : null,
                },
              },
            );
            setConfigId(saved.id);
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
          <JobLauncher profile={profile} onQueued={onQueued} />
          <Fold title="Build settings">{settings}</Fold>
        </>
      ) : (
        settings
      )}
    </div>
  );
}
