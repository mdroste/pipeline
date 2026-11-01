import { useState, type FormEvent } from "react";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { Button, EmptyState } from "../../ui";
import type {
  VersionControlAction,
  VersionControlState,
} from "../../lib/versionControlClient";

/** The page before a repository exists: one step at a time, in order. */
export function RepositorySetup({
  state,
  busy,
  onAct,
  onAttachFolder,
}: {
  state: Exclude<VersionControlState, { state: "ready" }>;
  busy: boolean;
  onAct: (action: VersionControlAction) => Promise<boolean>;
  onAttachFolder: () => void;
}) {
  if (state.state === "noFolder")
    return (
      <EmptyState
        title="No folder attached yet."
        description="Version control tracks the files in this project's folder."
        action={
          <Button variant="primary" onClick={onAttachFolder}>
            Attach folder
          </Button>
        }
      />
    );
  if (state.state === "gitMissing")
    return (
      <EmptyState
        title="Git isn't installed on this computer."
        description="Install Git, then come back to this page."
        action={
          <Button
            variant="primary"
            onClick={() => void openExternal("https://git-scm.com/downloads")}
          >
            Get Git
          </Button>
        }
      />
    );
  return (
    <EmptyState
      title={
        state.problem
          ? "Git can't read this folder."
          : "This folder isn't tracked yet."
      }
      description={
        state.problem ??
        "Git keeps a history of your files, so you can see what changed, go back, and work with coauthors. Nothing leaves your computer until you connect a remote and push."
      }
      action={
        !state.problem && (
          <Button
            variant="primary"
            disabled={busy}
            onClick={() => void onAct({ kind: "initialize" })}
          >
            Start tracking this folder
          </Button>
        )
      }
    />
  );
}

const HOSTS = [
  { name: "GitHub", create: "https://github.com/new" },
  { name: "Bitbucket", create: "https://bitbucket.org/repo/create" },
];

/** Connects a repository made on a host's own site. Pipeline holds no account
 * with either host, so creating the repository happens there. */
export function ConnectRemote({
  busy,
  onAct,
  onDone,
}: {
  busy: boolean;
  onAct: (action: VersionControlAction) => Promise<boolean>;
  onDone: () => void;
}) {
  const [url, setUrl] = useState("");
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (await onAct({ kind: "setRemote", url })) onDone();
  };
  return (
    <form className="vc-connect" onSubmit={(event) => void submit(event)}>
      <p>
        A remote keeps a copy of this repository online, where coauthors can
        reach it. Create an empty repository on your host, with no README or
        license, then paste its address here.
      </p>
      <div className="vc-connect-hosts">
        {HOSTS.map((host) => (
          <Button
            key={host.name}
            onClick={() => void openExternal(host.create)}
          >
            Create on {host.name}
          </Button>
        ))}
      </div>
      <label>
        <span>Repository address</span>
        <input
          className="vc-input"
          placeholder="https://github.com/owner/repository.git"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          value={url}
          onChange={(event) => setUrl(event.target.value)}
        />
      </label>
      <p className="vc-connect-note">
        Pipeline uses your computer&rsquo;s Git sign-in and never sees your
        password or token.
      </p>
      <Button variant="primary" type="submit" disabled={busy || !url.trim()}>
        Connect
      </Button>
    </form>
  );
}
