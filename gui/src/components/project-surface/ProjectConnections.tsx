import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import {
  studioClient,
  type ProjectConnection,
  type ConnectionSettings,
} from "../../lib/studioClient";
import type { ProjectRecord } from "../../lib/projectClient";
import { button, input, muted, type SurfaceApi } from "./shared";
import type { RepositoryStatus } from "../../lib/repositoryClient";

export default function ProjectConnections({
  workspaceId,
  workspace,
  reportError,
  run,
  setTab,
  repository,
}: SurfaceApi & { repository: RepositoryStatus | null }) {
  const [connection, setConnection] =
    useState<ProjectRecord<ProjectConnection> | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [provider, setProvider] =
    useState<ConnectionSettings["provider"]>("git");
  const [webUrl, setWebUrl] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let live = true;
    setLoaded(false);
    void studioClient
      .records(workspaceId, "connection")
      .then((records) => {
        if (!live) return;
        const saved = records[0] ?? null;
        setConnection(saved);
        setProvider(saved?.body.provider ?? "git");
        setWebUrl(saved?.body.webUrl ?? "");
        setLoaded(true);
      })
      .catch(reportError);
    return () => {
      live = false;
    };
  }, [workspaceId]);
  const save = (active: boolean) =>
    run(async () => {
      setBusy(true);
      try {
        const saved = await studioClient.mutate(workspaceId, {
          action: "saveConnection",
          expectedRevision: connection?.revision ?? 0,
          settings: active
            ? {
                provider,
                remoteName:
                  provider === "dropbox"
                    ? null
                    : (repository?.remote?.name ?? null),
                webUrl: webUrl.trim() || null,
                active,
              }
            : {
                provider: connection!.body.provider,
                remoteName: connection!.body.remoteName,
                webUrl: connection!.body.webUrl,
                active: false,
              },
        });
        setConnection(saved);
      } finally {
        setBusy(false);
      }
    });
  const active = connection?.body.active;
  return (
    <section className="space-y-3" aria-label="Project connections">
      <h3 className="font-semibold">Connections</h3>
      <p className={muted}>
        {workspace?.root ?? "Attach your existing research folder first."}
      </p>
      {active ? (
        <>
          <p>
            {connection.body.provider === "dropbox"
              ? "Dropbox local folder"
              : `${connection.body.provider} · ${connection.body.remoteName}`}
          </p>
          <p className={muted}>{connection.body.basis}</p>
          <p className={muted}>
            Supported actions: {connection.body.capabilities.join(", ")}. Access
            to files does not add them to assistant context.
          </p>
          {connection.body.webUrl && (
            <button
              className={button}
              onClick={() =>
                void open(connection.body.webUrl!).catch(reportError)
              }
            >
              Open connected project
            </button>
          )}
          <button
            className={button}
            disabled={busy}
            onClick={() => void save(false)}
          >
            Disconnect
          </button>
          <p className={muted}>
            Disconnecting preserves your files, saved versions, and research
            history.
          </p>
        </>
      ) : (
        <>
          <label className="block text-xs">
            Connect this folder through
            <select
              className={input}
              value={provider}
              onChange={(event) =>
                setProvider(
                  event.target.value as ConnectionSettings["provider"],
                )
              }
            >
              <option value="git">Git</option>
              <option value="overleaf">Overleaf Git</option>
              <option value="github">GitHub</option>
              <option value="dropbox">Dropbox local folder</option>
            </select>
          </label>
          {provider !== "git" && (
            <label className="block text-xs">
              Project link (optional)
              <input
                type="url"
                className={input}
                value={webUrl}
                onChange={(event) => setWebUrl(event.target.value)}
                placeholder={
                  provider === "dropbox"
                    ? "https://www.dropbox.com/…"
                    : provider === "overleaf"
                      ? "https://www.overleaf.com/project/…"
                      : "https://github.com/owner/repository"
                }
              />
            </label>
          )}
          <p className={muted}>
            {provider === "dropbox"
              ? "Choose a folder already managed by the Dropbox desktop app. Pipeline observes local files; upload status remains unknown. Keep online-only files available offline before editing."
              : `Use an existing clone and select its remote in Version control. ${repository?.remote ? `Selected: ${repository.remote.name}.` : "No remote selected."}`}
          </p>
          <button
            className={button}
            disabled={busy || !loaded || !workspace?.root}
            onClick={() => void save(true)}
          >
            Connect existing folder
          </button>
        </>
      )}
      {provider !== "dropbox" && (
        <button className={button} onClick={() => setTab("version-control")}>
          Open version control
        </button>
      )}
      {provider === "overleaf" && (
        <p className={muted}>
          Overleaf Git requires an eligible plan. Use your native Git credential
          manager with username “git” and an Overleaf token. Overleaf does not
          support branches, tags, or Git LFS; Git exchange does not preserve
          every tracked change or comment. Keep one transport active for this
          folder.
        </p>
      )}
    </section>
  );
}
