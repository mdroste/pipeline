import { chatgptAccountClient } from "../lib/chatgptAccountClient";
import type { AccountStatus } from "../lib/chatgptAccountClient";
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type {
  WorkspaceModelCatalog,
  WorkspaceRateLimits,
} from "../lib/workbenchTypes";

/** Names a rate-limit window by its length, such as "7-day limit". */
function windowLimitLabel(minutes: number | null) {
  if (!minutes) return "Second limit";
  if (minutes % 1440 === 0) return `${minutes / 1440}-day limit`;
  if (minutes % 60 === 0) return `${minutes / 60}-hour limit`;
  return `${minutes}-minute limit`;
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
    const next = await chatgptAccountClient.status();
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
    <div role="group" aria-label="ChatGPT account" className="space-y-2">
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1.5">
        <p className="text-sm">
          {signedIn
            ? `Signed in${status.account.email ? ` as ${status.account.email}` : ""}${status.account.planType ? ` (${status.account.planType})` : ""}`
            : status
              ? "Not signed in to ChatGPT."
              : "Checking ChatGPT account…"}
        </p>
        <div className="flex flex-wrap items-center gap-1.5">
          <button
            className="settings-button"
            disabled={disabled}
            onClick={() => void act(() => chatgptAccountClient.loginStart())}
          >
            {signedIn ? "Change account" : "Sign in to ChatGPT"}
          </button>
          {status?.pendingLogin && (
            <button
              className="settings-button"
              disabled={busy}
              onClick={() =>
                void act(() =>
                  chatgptAccountClient.loginCancel({ ...status.pendingLogin }),
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
              onClick={() => void act(() => chatgptAccountClient.logout())}
            >
              Sign out
            </button>
          )}
          <button
            className="settings-button-quiet"
            disabled={busy}
            onClick={() => void act(async () => {})}
          >
            Refresh
          </button>
          {signedIn && (
            <button
              className="settings-button-quiet"
              disabled={disabled}
              onClick={() =>
                void act(async () => {
                  const [models, usage] = await Promise.all([
                    chatgptAccountClient.models(),
                    chatgptAccountClient.limits(),
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
      </div>
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
                  chatgptAccountClient.selectExisting({
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
        <p className="text-sm">Finish signing in to ChatGPT in your browser.</p>
      )}
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
            ? `; ${windowLimitLabel(bucket.secondary.windowDurationMins)}: ${bucket.secondary.remainingPercent}% remaining`
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
