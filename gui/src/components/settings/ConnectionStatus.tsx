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
  const [checked, setChecked] = useState<string | null>(null);
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
    setChecked(null);
    setError("");
    setAttempted(false);
    return () => {
      checkGeneration.current += 1;
    };
  }, [key, subscription, settings.local_base_url]);
  const status = error
    ? "Connection needs attention"
    : checked
      ? "Signed in"
      : subscription
        ? dependency?.cli_auth_status === "signed_in"
          ? "Signed in"
          : dependency?.cli_auth_status === "signed_out"
            ? "Sign-in required"
            : "Subscription · check sign-in"
        : provider !== "local" && !key
          ? "API key required"
          : loading
            ? "Checking model access…"
            : blocked
              ? "Waiting for saved connection"
              : catalog?.warning || catalog?.stale
                ? "Connection needs attention"
                : catalog?.models.length
                  ? "Model access available"
                  : "Not yet verified";
  return (
    <div className="settings-connection-status">
      <div>
        <span className="settings-status-badge">{status}</span>
        <span className="settings-row-description block">
          {subscription
            ? "Subscription"
            : provider === "local"
              ? "Compatible server"
              : "API · metered usage"}{" "}
          · Reviews
        </span>
      </div>
      <button
        type="button"
        className="settings-button"
        disabled={checking || loading || saved.pending || Boolean(saved.error)}
        onClick={async () => {
          const generation = ++checkGeneration.current;
          setChecking(true);
          setAttempted(true);
          setError("");
          setChecked(null);
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
                    "Sign-in could not be verified. Sign in using the provider’s command-line tool.",
                );
              setChecked("Subscription sign-in verified.");
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
      {checked && (
        <p role="status" className="settings-inline-status">
          {checked}
        </p>
      )}
      {!subscription &&
      catalog?.models.length &&
      !catalog.warning &&
      !catalog.stale &&
      !blocked ? (
        <p className="settings-row-description">
          Model access verified. This check does not run a model.
        </p>
      ) : null}
    </div>
  );
}
