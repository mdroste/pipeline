import { useEffect, useId, useRef, useState } from "react";
import { useProjectEnvironment } from "../../lib/projectEnvironment";
import type { WorkspaceConfig } from "../../lib/workbench/harnessTypes";
import { fieldError } from "../../ui/classes";
import AssistantAccessChoice, {
  isAssistantAccess,
  type AssistantAccess,
} from "../AssistantAccessChoice";
import { button, Card, input, muted } from "./shared";

/** The project setting sent with every conversation's instructions. */
const FIELD_CONVENTIONS_KEY = "fieldConventions";
/** Matches the limit Pipeline enforces when the setting is saved. */
export const FIELD_CONVENTIONS_MAX_BYTES = 4 * 1024;
/**
 * Whether conversations receive the background from Settings → About you.
 * Unset means on, so a project stores it only to differ from the default.
 */
const ABOUT_YOU_KEY = "aboutYou";

const savedConventions = (config: WorkspaceConfig | null) => {
  const value = config?.body[FIELD_CONVENTIONS_KEY];
  return typeof value === "string" ? value : "";
};

/**
 * The project's assistant settings, which conversations in it inherit: the
 * access mode, the field and conventions every conversation follows, and
 * whether they also receive About you.
 */
export default function ProjectAssistantAccess({
  workspaceId,
  reportError,
}: {
  workspaceId: string;
  reportError: (cause: unknown) => void;
}) {
  const { harnessClient } = useProjectEnvironment();
  const report = useRef(reportError);
  report.current = reportError;
  const hintId = useId();
  const aboutYouHintId = useId();
  const [config, setConfig] = useState<WorkspaceConfig | null>(null);
  const [inherited, setInherited] = useState<AssistantAccess>("inspect");
  const [conventions, setConventions] = useState("");
  const [aboutYouDefault, setAboutYouDefault] = useState(true);
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    let stale = false;
    setConfig(null);
    setConventions("");
    void Promise.all([
      harnessClient.getWorkspaceConfig(workspaceId),
      harnessClient.getWorkspaceConfig(null),
    ])
      .then(([project, global]) => {
        if (stale) return;
        setConfig(project);
        setConventions(savedConventions(project));
        setInherited(
          isAssistantAccess(global.body.mode) ? global.body.mode : "inspect",
        );
        setAboutYouDefault(global.body[ABOUT_YOU_KEY] !== false);
      })
      .catch((cause) => {
        if (!stale) report.current(cause);
      });
    return () => {
      stale = true;
    };
  }, [harnessClient, workspaceId]);
  const saved = config?.body.mode;
  const value = isAssistantAccess(saved) ? saved : inherited;
  const save = async (
    change: (body: Record<string, unknown>) => Record<string, unknown>,
    operation: string,
  ) => {
    setSaving(true);
    try {
      // The full profile editor writes the same record; start from its latest.
      const latest = await harnessClient.getWorkspaceConfig(workspaceId);
      const next = await harnessClient.saveWorkspaceConfig({
        workspaceId,
        expectedRevision: latest.revision,
        body: change({ ...latest.body }),
        operationId: `${operation}-${crypto.randomUUID()}`,
      });
      setConfig(next);
    } catch (cause) {
      report.current(cause);
    } finally {
      setSaving(false);
    }
  };
  const trimmed = conventions.trim();
  const tooLong =
    new TextEncoder().encode(trimmed).length > FIELD_CONVENTIONS_MAX_BYTES;
  const changed = trimmed !== savedConventions(config).trim();
  // An empty setting is removed, so a project that never sets one, and one
  // that clears it, send the same instructions.
  const saveConventions = () =>
    save((body) => {
      if (trimmed) body[FIELD_CONVENTIONS_KEY] = trimmed;
      else delete body[FIELD_CONVENTIONS_KEY];
      return body;
    }, "project-conventions");
  const ownAboutYou = config?.body[ABOUT_YOU_KEY];
  const aboutYou =
    typeof ownAboutYou === "boolean" ? ownAboutYou : aboutYouDefault;
  const saveAboutYou = (use: boolean) =>
    save((body) => {
      if (use === aboutYouDefault) delete body[ABOUT_YOU_KEY];
      else body[ABOUT_YOU_KEY] = use;
      return body;
    }, "project-about-you");
  return (
    <Card title="Assistants">
      <AssistantAccessChoice
        value={value}
        onChange={(mode) =>
          void save((body) => ({ ...body, mode }), "project-access")
        }
        disabled={!config || saving}
        hint="Conversations in this project use this from their next message. A conversation that sets its own access in Assistant settings keeps that choice."
      />
      <label className="mt-4 block text-xs">
        Field and conventions
        <textarea
          className={`${input} mt-1`}
          rows={4}
          value={conventions}
          disabled={!config}
          aria-describedby={hintId}
          aria-invalid={tooLong || undefined}
          placeholder="Economics. Use the field's notation; report standard errors clustered as in the paper."
          onChange={(event) => setConventions(event.target.value)}
        />
      </label>
      {tooLong ? (
        <p id={hintId} role="alert" className={fieldError}>
          Shorten this to 4 KiB or less, about 4,000 characters.
        </p>
      ) : (
        <p id={hintId} className={muted}>
          Your field and the conventions assistants should follow. Every
          conversation in this project follows it from its next message, on
          every connection and with every agent profile.
        </p>
      )}
      <button
        type="button"
        className={`${button} mt-2`}
        disabled={!config || saving || !changed || tooLong}
        onClick={() => void saveConventions()}
      >
        Save field and conventions
      </button>
      <label className="mt-4 flex items-start gap-2 text-xs">
        <input
          type="checkbox"
          className="mt-0.5"
          checked={aboutYou}
          disabled={!config || saving}
          aria-describedby={aboutYouHintId}
          onChange={(event) => void saveAboutYou(event.target.checked)}
        />
        <span>
          Use About you in this project
          <span id={aboutYouHintId} className={`${muted} block`}>
            Sends your background from Settings → About you with every
            conversation here, before this project’s field and conventions. Turn
            it off for work written in someone else’s voice.
          </span>
        </span>
      </label>
    </Card>
  );
}
