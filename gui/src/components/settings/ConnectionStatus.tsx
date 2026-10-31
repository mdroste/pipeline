import { useContext, useEffect, useRef, useState } from "react";
import { appClient } from "../../lib/appClient";
import type { DepsReport, ModelCatalog, Settings } from "../../lib/types";
import { ReviewSaveState } from "./SaveState";

export default function ConnectionStatus({
  provider,
  settings,
  catalog,
  loading,
  blocked,
  onCheck,
  dependencies,
}: {
  provider: string;
  settings: Settings;
  catalog?: ModelCatalog;
  loading?: boolean;
  blocked?: boolean;
  onCheck: () => Promise<void>;
  dependencies?: DepsReport | null;
}) {
  const [checking, setChecking] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const [error, setError] = useState("");
  const [checked, setChecked] = useState(false);
  const checkGeneration = useRef(0);
  const saved = useContext(ReviewSaveState);
  const subscription =
    provider === "claude"
      ? settings.claude_access_mode !== "api"
      : provider === "codex" && settings.codex_access_mode !== "api";
  const dependency = dependencies?.deps.find(
    (dep) => dep.name === (provider === "claude" ? "Claude CLI" : "Codex CLI"),
  );
  const key =
    provider === "claude"
      ? settings.anthropic_api_key
      : provider === "codex"
        ? settings.openai_api_key
        : provider === "antigravity"
          ? settings.google_api_key
          : settings.local_api_key;
  useEffect(() => {
    checkGeneration.current += 1;
    setChecking(false);
    setChecked(false);
    setError("");
    setAttempted(false);
    return () => {
      checkGeneration.current += 1;
    };
  }, [key, subscription, settings.local_base_url]);
  const status = error
    ? "Connection problem"
    : checked
      ? "Signed in"
      : subscription
        ? dependency?.cli_auth_status === "signed_in"
          ? "Signed in"
          : dependency?.cli_auth_status === "signed_out"
            ? "Sign-in required"
            : "Sign-in not checked"
        : provider !== "local" && !key
          ? "API key required"
          : loading
            ? "Checking…"
            : blocked
              ? "Saving…"
              : catalog?.warning || catalog?.stale
                ? "Connection problem"
                : catalog?.models.length
                  ? "Connected"
                  : "Not checked";
  return (
    <div className="settings-connection-status">
      <span role="status" className="settings-status-badge">
        {status}
      </span>
      <button
        type="button"
        className="settings-button"
        title={subscription ? undefined : "This check is free."}
        disabled={checking || loading || saved.pending || Boolean(saved.error)}
        onClick={async () => {
          const generation = ++checkGeneration.current;
          setChecking(true);
          setAttempted(true);
          setError("");
          setChecked(false);
          try {
            if (subscription) {
              const { readiness: report } = await appClient.executionPlan({
                variables: null,
                extraInputs: null,
                expectedProfileConfigSnapshotId: null,
                diff: false,
                paperPath: null,
                inputInterpretation: null,
              });
              if (generation !== checkGeneration.current) return;
              const dep = report.deps.find(
                (item) =>
                  item.name ===
                  (provider === "claude" ? "Claude CLI" : "Codex CLI"),
              );
              if (!dep?.found)
                throw new Error(
                  dep?.hint || "The provider command is not installed.",
                );
              if (dep.cli_auth_status !== "signed_in")
                throw new Error(
                  dep.hint ||
                    "Could not confirm sign-in. Sign in with the provider’s command-line tool.",
                );
              setChecked(true);
            }
            await onCheck();
          } catch (cause) {
            if (generation === checkGeneration.current) setError(String(cause));
          } finally {
            if (generation === checkGeneration.current) setChecking(false);
          }
        }}
      >
        {checking ? "Checking…" : "Check connection"}
      </button>
      {(error ||
        (!subscription &&
          catalog?.warning &&
          !blocked &&
          (attempted ||
            Boolean(key) ||
            (provider === "local" && Boolean(settings.local_model))))) && (
        <p role="alert" className="settings-error">
          {error || catalog?.warning}
        </p>
      )}
    </div>
  );
}
