import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  WorkspaceAccountState,
  WorkspaceModelCatalog,
  WorkspaceRateLimits,
} from "../lib/workbenchTypes";

interface AccountStatus {
  account: WorkspaceAccountState;
  epoch: number;
  version: string;
  loginInProgress: boolean;
  pendingLogin?: { loginId: string; epoch: number } | null;
  existingAccounts?: Array<{
    source: string;
    label: string;
    email: string | null;
  }>;
  error?: string | null;
}

export default function ChatgptConnection({
  onAccountChange,
  onStatusChange,
}: {
  onAccountChange?: () => void;
  onStatusChange?: () => void;
}) {
  const [status, setStatus] = useState<AccountStatus | null>(null);
  const [catalog, setCatalog] = useState<WorkspaceModelCatalog | null>(null);
  const [limits, setLimits] = useState<WorkspaceRateLimits | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const identity = useRef<string | null>(null);
  const readiness = useRef<string | null>(null);
  const mounted = useRef(true);
  const refresh = useCallback(async () => {
    const next = await invoke<AccountStatus>("chatgpt_account_status");
    if (mounted.current) {
      setStatus(next);
      setError(null);
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    const poll = () =>
      void refresh().catch((e) => {
        if (mounted.current) setError(typeof e === "string" ? e : String(e));
      });
    poll();
    const timer = window.setInterval(poll, 5000);
    let dispose: (() => void) | undefined;
    void listen("chatgpt:account-changed", poll).then((stop) => {
      if (mounted.current) dispose = stop;
      else stop();
    });
    return () => {
      mounted.current = false;
      window.clearInterval(timer);
      dispose?.();
    };
  }, [refresh]);
  useEffect(() => {
    if (!status) return;
    const next = JSON.stringify([status.epoch, status.account]);
    if (identity.current !== next) {
      setCatalog(null);
      setLimits(null);
      if (identity.current !== null) onAccountChange?.();
      identity.current = next;
    }
    const ready = JSON.stringify([
      next,
      status.loginInProgress,
      status.existingAccounts,
    ]);
    if (readiness.current !== ready) {
      readiness.current = ready;
      onStatusChange?.();
    }
  }, [status, onAccountChange, onStatusChange]);
  async function act(action: () => Promise<unknown>) {
    setBusy(true);
    setError(null);
    try {
      await action();
      await refresh();
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setBusy(false);
    }
  }
  const signedIn = status?.account.status === "chatgpt";
  const disabled = busy || !status || status.loginInProgress;
  return (
    <div
      role="group"
      aria-label="ChatGPT account"
      className="space-y-3 rounded border border-gray-200 p-4 dark:border-neutral-700"
    >
      <p className="text-sm">
        {signedIn
          ? `Signed in${status.account.email ? ` as ${status.account.email}` : ""}${status.account.planType ? ` (${status.account.planType})` : ""}`
          : status
            ? "Not signed in to ChatGPT."
            : "Checking ChatGPT account…"}
      </p>
      {!!status?.existingAccounts?.length && (
        <div className="space-y-2 text-sm">
          <p>
            Your earlier Conversation and Review sign-ins use different
            accounts. Choose the account to use throughout Pipeline, or sign in
            again.
          </p>
          {status.existingAccounts.map((account) => (
            <button
              key={account.source}
              className="settings-button mr-2"
              disabled={disabled}
              onClick={() =>
                void act(() =>
                  invoke("chatgpt_select_existing_account", {
                    source: account.source,
                  }),
                )
              }
            >
              Use {account.label} account
              {account.email ? ` (${account.email})` : ""}
            </button>
          ))}
        </div>
      )}
      {status?.loginInProgress && (
        <p className="text-sm">
          Complete ChatGPT sign-in in your browser. Conversations and Reviews
          can start when sign-in finishes.
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <button
          className="settings-button"
          disabled={disabled}
          onClick={() => void act(() => invoke("chatgpt_login_start"))}
        >
          {signedIn ? "Change account" : "Sign in to ChatGPT"}
        </button>
        {status?.pendingLogin && (
          <button
            className="settings-button"
            disabled={busy}
            onClick={() =>
              void act(() =>
                invoke("chatgpt_login_cancel", { ...status.pendingLogin }),
              )
            }
          >
            Cancel sign-in
          </button>
        )}
        {signedIn && (
          <button
            className="settings-button"
            disabled={disabled}
            onClick={() => void act(() => invoke("chatgpt_logout"))}
          >
            Sign out
          </button>
        )}
        <button
          className="settings-button"
          disabled={busy}
          onClick={() => void act(async () => {})}
        >
          Refresh
        </button>
        {signedIn && (
          <button
            className="settings-button"
            disabled={disabled}
            onClick={() =>
              void act(async () => {
                const [models, usage] = await Promise.all([
                  invoke<WorkspaceModelCatalog>("chatgpt_model_catalog"),
                  invoke<WorkspaceRateLimits>("chatgpt_rate_limits"),
                ]);
                setCatalog(models);
                setLimits(usage);
              })
            }
          >
            Models & usage
          </button>
        )}
      </div>
      {catalog && (
        <details className="settings-disclosure">
          <summary>
            Available models <span>{catalog.models.length} models</span>
          </summary>
          <div className="space-y-2">
            {catalog.models.map((model) => (
              <p className="text-sm" key={model.id}>
                {model.displayName}
                {model.isDefault ? " · Default" : ""}
                <span className="ml-2 text-xs text-gray-500">
                  {model.supportedReasoningEfforts
                    .map((effort) => effort.reasoningEffort)
                    .join(" · ")}
                </span>
              </p>
            ))}
          </div>
        </details>
      )}
      {limits?.buckets.map((bucket, i) => (
        <p className="text-xs" key={bucket.limitId ?? i}>
          {bucket.limitName ?? bucket.limitId ?? "Account"}:{" "}
          {bucket.primary
            ? `${bucket.primary.remainingPercent}% remaining`
            : "Current usage unavailable"}
          {bucket.secondary
            ? `; second window ${bucket.secondary.remainingPercent}% remaining`
            : ""}
        </p>
      ))}
      {(error || status?.error) && (
        <p role="alert" className="text-sm text-red-600 dark:text-red-400">
          {error || status?.error}
        </p>
      )}
    </div>
  );
}
