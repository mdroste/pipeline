import { projectClient, type ProjectCapabilities } from "../lib/projectClient";
import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type {
  LoginStart,
  WorkbenchEvent,
  WorkspaceAccountState,
  WorkspaceModelCatalog,
  WorkspaceRateLimits,
} from "../lib/workbenchTypes";

import ConversationTitleSettings from "./settings/ConversationTitleSettings";

export default function WorkspaceConnectionSettings({
  embedded = false,
  connectionOnly = false,
}: {
  embedded?: boolean;
  connectionOnly?: boolean;
}) {
  const [capabilities, setCapabilities] = useState<ProjectCapabilities | null>(
    null,
  );
  useEffect(() => {
    let alive = true;
    if (connectionOnly) return;
    void projectClient
      .capabilities()
      .then((value) => {
        if (alive) setCapabilities(value);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [connectionOnly]);
  const [account, setAccount] = useState<WorkspaceAccountState | null>(null);
  const [catalog, setCatalog] = useState<WorkspaceModelCatalog | null>(null);
  const [limits, setLimits] = useState<WorkspaceRateLimits | null>(null);
  const [login, setLogin] = useState<LoginStart | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(async (refreshToken = false) => {
    setError(null);
    try {
      await workbenchClient.connectCodex();
      const nextAccount = await workbenchClient.accountState(refreshToken);
      setAccount(nextAccount);
      if (nextAccount.status === "chatgpt") {
        const [nextCatalog, nextLimits] = await Promise.all([
          workbenchClient.modelCatalog(),
          workbenchClient.rateLimits().catch(() => null),
        ]);
        setCatalog(nextCatalog);
        setLimits(nextLimits);
      } else {
        setCatalog(null);
        setLimits(null);
      }
    } catch (cause) {
      setError(workbenchErrorMessage(cause));
    } finally {
      setLoading(false);
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<WorkbenchEvent>("workbench:event", (event) => {
      if (disposed) return;
      if (
        event.payload.kind === "accountUpdated" ||
        event.payload.kind === "accountLoginCompleted"
      ) {
        setLogin(null);
        void refresh(true);
      }
      if (event.payload.kind === "accountRateLimitsUpdated") {
        void workbenchClient
          .rateLimits()
          .then(setLimits)
          .catch(() => undefined);
      }
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refresh]);

  const startLogin = async () => {
    setBusy(true);
    setError(null);
    try {
      setLogin(await workbenchClient.loginStart());
    } catch (cause) {
      setError(workbenchErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const cancelLogin = async () => {
    if (!login) return;
    setBusy(true);
    try {
      await workbenchClient.loginCancel(login.loginId);
      setLogin(null);
      await refresh();
    } catch (cause) {
      setError(workbenchErrorMessage(cause));
      setBusy(false);
    }
  };

  const logout = async () => {
    setBusy(true);
    try {
      await workbenchClient.logout();
      setLogin(null);
      await refresh();
    } catch (cause) {
      setError(workbenchErrorMessage(cause));
      setBusy(false);
    }
  };

  return (
    <div className="max-w-3xl space-y-8">
      <header>
        {!embedded && (
          <h1 className="text-xl font-semibold text-gray-900 dark:text-neutral-100">
            Workspace ChatGPT
          </h1>
        )}
        <p className="mt-2 text-sm leading-6 text-gray-500 dark:text-neutral-400">
          Conversations use a separate ChatGPT sign-in from Reviews.
        </p>
      </header>

      <section className="rounded-xl border border-gray-200 bg-white p-5 dark:border-neutral-700 dark:bg-neutral-900">
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 className="text-sm font-semibold text-gray-900 dark:text-neutral-100">
              Connection
            </h2>
            <p className="mt-1 text-sm text-gray-500 dark:text-neutral-400">
              {loading
                ? "Checking conversation connection…"
                : account?.status === "chatgpt"
                  ? `${account.email ?? "ChatGPT account"} · ${account.planType ?? "plan unavailable"}`
                  : account?.status === "unsupported"
                    ? `Unsupported authentication mode: ${account.unsupportedAccountType ?? "unknown"}`
                    : "Not signed in"}
            </p>
          </div>
          {account?.status === "chatgpt" ? (
            <button
              type="button"
              disabled={busy}
              onClick={() => void logout()}
              className="rounded-lg border border-gray-300 px-3 py-1.5 text-sm dark:border-neutral-600"
            >
              Sign out
            </button>
          ) : (
            <button
              type="button"
              disabled={busy || loading}
              onClick={() => void startLogin()}
              className="rounded-lg bg-gray-900 px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50 dark:bg-neutral-100 dark:text-neutral-900"
            >
              Sign in with ChatGPT
            </button>
          )}
        </div>
        <button
          type="button"
          className="settings-button mt-3"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            void refresh(true);
          }}
        >
          Check conversation connection
        </button>
        {login && (
          <div className="mt-4 rounded-lg bg-blue-50 p-3 text-sm text-blue-800 dark:bg-blue-950/40 dark:text-blue-200">
            Complete sign-in in your browser. This page will update
            automatically.
            <button
              type="button"
              onClick={() => void cancelLogin()}
              className="ml-3 underline"
            >
              Cancel
            </button>
          </div>
        )}
        {error && (
          <p
            role="alert"
            className="mt-4 text-sm text-red-600 dark:text-red-400"
          >
            {error}
          </p>
        )}
      </section>

      {!connectionOnly && <ConversationTitleSettings />}

      {!connectionOnly && (
        <section className="space-y-3 rounded-xl border p-5 text-sm">
          <h2 className="font-semibold">Research capabilities</h2>
          <p className="text-gray-500">
            {capabilities?.qualification ??
              "Project reading, notes and task history work without ChatGPT sign-in."}
          </p>
          {capabilities && (
            <>
              <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-xs">
                <dt>Platform</dt>
                <dd>
                  {capabilities.platform} · file acceptance{" "}
                  {capabilities.fileAcceptance
                    ? "available"
                    : "not yet supported on this platform"}
                </dd>
                <dt>Git</dt>
                <dd>
                  {capabilities.git ??
                    "Missing. Install Git or use local file copies."}
                </dd>
                <dt>LaTeX</dt>
                <dd>
                  {capabilities.latex ??
                    "Missing. Install a TeX distribution and configure an execution profile."}
                </dd>
                <dt>PDF pages</dt>
                <dd>
                  {capabilities.pdfPages ??
                    "No system pdftoppm found. Packaged resources may still supply it; try a page render, or install Poppler."}
                </dd>
                <dt>Stata</dt>
                <dd>{capabilities.stataPolicy}</dd>
              </dl>
              <p className="text-xs text-gray-500">
                Test each command profile before using it. Platform test
                details: {capabilities.record}
              </p>
            </>
          )}
        </section>
      )}

      {account?.status === "chatgpt" && (
        <details open={!connectionOnly} className="settings-disclosure">
          <summary>
            Models & usage{" "}
            <span>{catalog?.models.length ?? 0} available models</span>
          </summary>
          <section>
            <div className="flex items-center justify-between">
              <h2 className="text-sm font-semibold text-gray-900 dark:text-neutral-100">
                Available models
              </h2>
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  setBusy(true);
                  void refresh(true);
                }}
                className="text-xs text-gray-500 underline"
              >
                Refresh
              </button>
            </div>
            <div className="mt-3 grid gap-2 sm:grid-cols-2">
              {catalog?.models.map((model) => (
                <div
                  key={model.id}
                  className="rounded-lg border border-gray-200 p-3 dark:border-neutral-700"
                >
                  <div className="text-sm font-medium text-gray-900 dark:text-neutral-100">
                    {model.displayName}
                    {model.isDefault ? " · Default" : ""}
                  </div>
                  <div className="mt-1 text-xs text-gray-500 dark:text-neutral-400">
                    {model.supportedReasoningEfforts
                      .map((option) => option.reasoningEffort)
                      .join(" · ")}
                  </div>
                </div>
              ))}
            </div>
          </section>

          <section>
            <h2 className="text-sm font-semibold text-gray-900 dark:text-neutral-100">
              Usage limits
            </h2>
            <div className="mt-3 space-y-2">
              {limits?.buckets.map((bucket, index) => (
                <div
                  key={bucket.limitId ?? index}
                  className="rounded-lg border border-gray-200 p-3 text-sm dark:border-neutral-700"
                >
                  <span className="font-medium">
                    {bucket.limitName ?? bucket.limitId ?? "ChatGPT"}
                  </span>
                  {bucket.primary ? (
                    <span className="ml-2 text-gray-500">
                      {bucket.primary.usedPercent}% used ·{" "}
                      {bucket.primary.remainingPercent}% remaining
                    </span>
                  ) : (
                    <span className="ml-2 text-gray-500">
                      Current usage unavailable
                    </span>
                  )}
                </div>
              )) ?? (
                <p className="text-sm text-gray-500">
                  Usage data is unavailable.
                </p>
              )}
            </div>
          </section>
        </details>
      )}
    </div>
  );
}
