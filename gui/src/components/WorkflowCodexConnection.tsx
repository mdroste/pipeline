import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Status {
  account: { status: "signedOut" | "chatgpt" | "unsupported"; email: string | null; planType: string | null };
  version: string;
  epoch: number;
  loginInProgress: boolean;
  unresolvedAttempts?: Array<{ id: string; label: string; state: string }>;
}

interface Limits { buckets: Array<{ limitId: string | null; limitName: string | null; primary: { remainingPercent: number } | null; secondary: { remainingPercent: number } | null }> }

export default function WorkflowCodexConnection({ onAccountChange, onStatusChange }: {
  onAccountChange?: () => void;
  onStatusChange?: () => void;
}) {
  const [status, setStatus] = useState<Status | null>(null);
  const [login, setLogin] = useState<{ loginId: string; epoch: number } | null>(null);
  const [reconciliation, setReconciliation] = useState<string | null>(null);
  const [limits, setLimits] = useState<Limits | null>(null);
  const accountIdentity = useRef<string | null>(null);
  const readinessIdentity = useRef<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!status) return;
    const next = JSON.stringify([status.epoch, status.account]);
    if (accountIdentity.current !== null && accountIdentity.current !== next) {
      setLimits(null);
      onAccountChange?.();
    }
    accountIdentity.current = next;
  }, [status, onAccountChange]);
  useEffect(() => {
    if (!status) return;
    const next = JSON.stringify([
      status.epoch, status.account, status.loginInProgress,
      (status.unresolvedAttempts ?? []).map((attempt) => attempt.id).sort(),
    ]);
    if (readinessIdentity.current !== next) {
      readinessIdentity.current = next;
      onStatusChange?.();
    }
  }, [status, onStatusChange]);
  const refresh = useCallback(async () => {
    const next = await invoke<Status>("workflow_codex_status");
    setStatus(next);
    if (!next.loginInProgress) setLogin(null);
    return next;
  }, []);
  useEffect(() => {
    let live = true;
    const poll = () => invoke<Status>("workflow_codex_status").then((next) => {
      if (live) {
        setStatus(next);
        if (!next.loginInProgress) setLogin(null);
      }
    }).catch((e) => { if (live) setError(String(e)); });
    void poll();
    const timer = window.setInterval(() => { void poll(); }, 5000);
    return () => { live = false; window.clearInterval(timer); };
  }, []);
  async function act(action: () => Promise<void>) {
    setBusy(true); setError(null);
    try { await action(); await refresh(); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }
  const button = "rounded border px-3 py-1 text-xs disabled:opacity-50";
  return (
    <div role="group" aria-label="Reviews ChatGPT connection" className="space-y-2 rounded border border-gray-200 dark:border-neutral-700 p-3">
      <p className="text-xs">{status?.account.status === "chatgpt"
        ? `Signed in${status.account.email ? ` as ${status.account.email}` : ""}${status.account.planType ? ` (${status.account.planType})` : ""}`
        : status ? "Reviews ChatGPT is not signed in." : "Checking Reviews connection…"}</p>
      <p className="text-xs text-gray-500 dark:text-neutral-400">This connection belongs to Reviews. Workspace keeps its own sign-in. Codex {status?.version ?? "App Server"}.</p>
      {status?.loginInProgress && <p className="text-xs">Complete ChatGPT sign-in in your browser. Workflow calls wait until sign-in finishes.</p>}
      <div className="flex gap-2">
        <button type="button" className={button} disabled={busy || !status || status.loginInProgress}
          onClick={() => void act(async () => {
            const started = await invoke<{ loginId: string; epoch: number }>("workflow_codex_login_start");
            setLogin(started);
          })}>{status?.account.status === "chatgpt" ? "Change account" : "Sign in to ChatGPT"}</button>
        {login && <button type="button" className={button} disabled={busy}
          onClick={() => void act(async () => { await invoke("workflow_codex_login_cancel", login); setLogin(null); })}>Cancel sign-in</button>}
        {status?.account.status === "chatgpt" && <button type="button" className={button} disabled={busy || status.loginInProgress}
          onClick={() => void act(async () => { await invoke("workflow_codex_logout"); })}>Sign out</button>}
        {status?.account.status === "chatgpt" && <button type="button" className={button} disabled={busy || status.loginInProgress}
          onClick={() => void act(async () => { setLimits(await invoke<Limits>("workflow_codex_rate_limits")); })}>Check usage</button>}
        <button type="button" className={button} disabled={busy} onClick={() => void act(async () => {})}>Refresh</button>
      </div>
      {limits?.buckets.map((bucket, index) => <p key={bucket.limitId ?? index} className="text-xs">
        {bucket.limitName ?? bucket.limitId ?? "Account"}: {bucket.primary ? `${bucket.primary.remainingPercent}% remaining` : "Primary usage unavailable"}{bucket.secondary ? `; second window ${bucket.secondary.remainingPercent}% remaining` : ""}
      </p>)}
      {!!status?.unresolvedAttempts?.length && <div className="space-y-2 text-xs">
        <p>An earlier attempt has no confirmed result. Check its saved result before allowing a new run. Previous usage still counts.</p>
        {status.unresolvedAttempts.map((attempt) => <div key={attempt.id} className="space-y-1">
          <p>{attempt.label} — {attempt.state} ({attempt.id.slice(0, 8)})</p>
          <div className="flex gap-2">
            <button type="button" className={button} disabled={busy} onClick={() => void act(async () => {
              const result = await invoke<{ status: string; preview: string; recordPath: string }>("workflow_codex_reconcile_attempt", { id: attempt.id, epoch: status.epoch });
              setReconciliation(`${result.status}\n${result.preview}\nSaved record: ${result.recordPath}`);
            })}>Check saved result</button>
            <button type="button" className={button} disabled={busy} onClick={() => void act(async () => {
              await invoke("workflow_codex_acknowledge_attempt", { id: attempt.id, epoch: status.epoch });
            })}>Acknowledge and allow a new run</button>
          </div>
        </div>)}
      </div>}
      {reconciliation && <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-xs">{reconciliation}</pre>}
      {error && <p role="alert" className="text-xs text-red-600 dark:text-red-400">{error}</p>}
    </div>
  );
}
