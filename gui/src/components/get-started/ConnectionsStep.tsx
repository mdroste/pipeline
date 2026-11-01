import { useCallback, useEffect, useRef, useState } from "react";
import {
  chatgptAccountClient,
  type AccountStatus,
} from "../../lib/chatgptAccountClient";
import { settingsClient } from "../../lib/settingsClient";
import type { DepsReport, Settings } from "../../lib/types";
import { Badge, Button } from "../../ui";
import { connectionRows, connectionSummary } from "./guide";

const message = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

/** Read-only connection overview; every change happens in Settings. */
export default function ConnectionsStep({
  dependencies,
  dependenciesLoading,
  onCheckDependencies,
  onOpenConnections,
  headingId,
}: {
  dependencies: DepsReport | null;
  dependenciesLoading: boolean;
  onCheckDependencies: () => void;
  onOpenConnections: (target?: string) => void;
  headingId: string;
}) {
  const [account, setAccount] = useState<AccountStatus | null>();
  const [accountError, setAccountError] = useState<string | null>(null);
  const [settings, setSettings] = useState<Settings | null>();
  const generation = useRef(0);

  const load = useCallback(() => {
    const request = ++generation.current;
    setAccountError(null);
    chatgptAccountClient
      .status()
      .then((next) => {
        if (request === generation.current) setAccount(next ?? null);
      })
      .catch((error: unknown) => {
        if (request === generation.current) setAccountError(message(error));
      });
    settingsClient
      .load()
      .then((response) => {
        if (request === generation.current)
          setSettings(response?.settings ?? null);
      })
      .catch(() => {
        // Only the API-key row depends on settings.
        if (request === generation.current) setSettings(null);
      });
  }, []);

  useEffect(() => {
    load();
    return () => {
      generation.current += 1;
    };
  }, [load]);

  const rows = connectionRows({
    dependencies,
    dependenciesLoading,
    account,
    accountError,
    settings,
  });

  return (
    <section aria-labelledby={headingId} className="space-y-4">
      <div>
        <h3
          id={headingId}
          tabIndex={-1}
          className="text-ui-body font-semibold outline-none"
        >
          Connections
        </h3>
        <p className="mt-1 text-ui-label text-ink-muted">
          Pipeline uses your own model accounts. Conversations and Reviews can
          use ChatGPT, Claude Code, an API key, or a local model server.
        </p>
      </div>
      <p
        role="status"
        className="rounded-ui-md bg-sunken px-3 py-2 text-ui-label"
      >
        {connectionSummary(rows)}
      </p>
      <ul aria-label="Model connections" className="space-y-2">
        {rows.map((row) => (
          <li
            key={row.id}
            className="flex flex-wrap items-start gap-x-3 gap-y-2 rounded-ui-md border border-line px-3 py-2.5"
          >
            <span className="mt-2">
              <Badge kind={row.tone} label={row.status} />
            </span>
            <div className="min-w-[12rem] flex-1">
              <p className="text-ui-label font-medium">{row.name}</p>
              <p className="text-ui-meta text-ink-muted">{row.use}</p>
              <p className="mt-0.5 text-ui-meta">
                <span className="font-medium">{row.status}</span>
                {row.detail && (
                  <span className="text-ink-muted"> · {row.detail}</span>
                )}
              </p>
            </div>
            <Button
              aria-label={`Open ${row.name} in Settings`}
              onClick={() => onOpenConnections(row.target)}
            >
              Open in Settings
            </Button>
          </li>
        ))}
      </ul>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          disabled={dependenciesLoading}
          onClick={() => {
            onCheckDependencies();
            load();
          }}
        >
          {dependenciesLoading ? "Checking…" : "Check again"}
        </Button>
        <Button variant="link" onClick={() => onOpenConnections()}>
          Open Settings → Connections
        </Button>
      </div>
    </section>
  );
}
