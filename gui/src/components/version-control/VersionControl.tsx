// The project's Git page: a bar for the branch and remote, then Changes and
// History over a shared list-and-detail layout. Every change happens because the
// researcher chose it here; see workbench::project::version_control.
import { useCallback, useEffect, useRef, useState } from "react";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import {
  Button,
  Menu,
  MenuItem,
  MenuLabel,
  MenuSeparator,
  SegmentedControl,
  Sheet,
  Spinner,
} from "../../ui";
import { Icon } from "../../ui/icons";
import { appEvents } from "../../lib/appEvents";
import {
  pullRequestUrl,
  remoteLabel,
  remoteSummary,
} from "../../lib/repositoryClient";
import {
  versionControlClient,
  type RepositoryView,
  type VersionControlAction,
} from "../../lib/versionControlClient";
import { workspaceFileAdapter } from "../../lib/fileWorkspaceClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { confirmDialog, notify, promptDialog } from "../DialogService";
import { unsavedDraftPaths } from "../file-workspace/useFileBuffers";
import ChangesPane from "./ChangesPane";
import HistoryPane from "./HistoryPane";
import { ConnectRemote, RepositorySetup } from "./RepositorySetup";
import {
  isNewestRequest,
  NETWORK,
  nextRequest,
  readSession,
  usePageSession,
  type NetworkKind,
} from "./pageSession";
import "./versionControl.css";

/** Picks up edits made elsewhere while the page is open. */
const REFRESH_MS = 15_000;

/** The unsaved drafts an action would pass over: Git reads and replaces the saved
 * files, so it never sees a draft, and a draft may no longer match afterwards. */
function draftsIn(action: VersionControlAction, drafts: string[]) {
  switch (action.kind) {
    case "commit":
    case "discard":
      return drafts.filter((path) => action.paths.includes(path));
    case "resolve":
      return drafts.filter((path) => path === action.path);
    case "pull":
    case "switchBranch":
    case "abortOperation":
      return drafts;
    default:
      return [];
  }
}

function BranchMenu({
  view,
  busy,
  onAct,
}: {
  view: RepositoryView;
  busy: boolean;
  onAct: (action: VersionControlAction) => Promise<boolean>;
}) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const others = view.branches.filter((branch) => !branch.current);
  const create = async () => {
    const name = await promptDialog(
      "Your current files come with you to the new branch.",
      { title: "New branch", label: "Branch name", confirmLabel: "Create" },
    );
    if (name?.trim()) await onAct({ kind: "createBranch", name });
  };
  const change = async (name: string) => {
    const leaving =
      view.status.branch !== null ||
      (await confirmDialog(
        "You aren't on a branch, so any commits made here will be hard to find after you switch. To keep them, choose New branch first.",
        { title: `Switch to ${name}`, confirmLabel: "Switch" },
      ));
    if (leaving) await onAct({ kind: "switchBranch", name });
  };
  return (
    <>
      <button
        ref={button}
        type="button"
        className="vc-branch"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={busy}
        onClick={() => setOpen(!open)}
      >
        <span className="vc-branch-label">Branch</span>
        <span className="vc-branch-name">
          {view.status.branch ?? "No branch"}
        </span>
        <Icon name="chevron-down" className="h-3.5 w-3.5" />
      </button>
      <Menu
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={button}
        label="Branches"
        align="start"
        width={280}
      >
        <MenuItem onSelect={() => void create()}>New branch…</MenuItem>
        {others.length > 0 && <MenuSeparator />}
        {others.length > 0 && <MenuLabel>Switch to</MenuLabel>}
        <div className="vc-branch-list">
          {others.map((branch) => (
            <MenuItem
              key={branch.name}
              onSelect={() => void change(branch.name)}
            >
              {branch.name}
            </MenuItem>
          ))}
        </div>
      </Menu>
    </>
  );
}

function checkedLabel(fetchedAt: string | null) {
  if (!fetchedAt) return "Not checked yet";
  const minutes = Math.round((Date.now() - Date.parse(fetchedAt)) / 60_000);
  if (!Number.isFinite(minutes)) return "";
  if (minutes < 1) return "Checked just now";
  if (minutes < 60) return `Checked ${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `Checked ${hours} hr ago`;
  return `Checked ${Math.round(hours / 24)} d ago`;
}

function MoreMenu({
  view,
  busy,
  onAct,
  onRemote,
}: {
  view: RepositoryView;
  busy: boolean;
  onAct: (action: VersionControlAction) => Promise<boolean>;
  onRemote: () => void;
}) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const { remote } = view.status;
  const undo = async () => {
    const confirmed = await confirmDialog(
      "Your files stay as they are, and the changes return to your list.",
      { title: "Undo last commit", confirmLabel: "Undo commit" },
    );
    if (confirmed) await onAct({ kind: "undoCommit" });
  };
  return (
    <>
      <button
        ref={button}
        type="button"
        className="vc-icon-button"
        aria-label="More version control actions"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <Icon name="more" className="h-4 w-4" />
      </button>
      <Menu
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={button}
        label="More version control actions"
      >
        {remote && (
          <MenuItem
            disabled={busy}
            onSelect={() => void onAct({ kind: "fetch" })}
          >
            Check {remoteLabel(remote)} for updates
          </MenuItem>
        )}
        {remote?.webUrl && (
          <MenuItem onSelect={() => void openExternal(remote.webUrl as string)}>
            Open on {remoteLabel(remote)}
          </MenuItem>
        )}
        <MenuItem onSelect={onRemote}>
          {remote ? "Change remote…" : "Connect remote…"}
        </MenuItem>
        {view.canUndoCommit && <MenuSeparator />}
        {view.canUndoCommit && (
          <MenuItem disabled={busy} onSelect={() => void undo()}>
            Undo last commit
          </MenuItem>
        )}
      </Menu>
    </>
  );
}

function SyncButton({
  view,
  running,
  busy,
  onAct,
  onCancel,
  onConnect,
}: {
  view: RepositoryView;
  running: NetworkKind | null;
  busy: boolean;
  onAct: (action: VersionControlAction) => Promise<boolean>;
  onCancel: () => void;
  onConnect: () => void;
}) {
  const { status } = view;
  if (running)
    return (
      <>
        <Spinner inline label={`${NETWORK[running]}…`} />
        <Button onClick={onCancel}>Stop</Button>
      </>
    );
  if (!status.remote)
    return (
      <Button variant="primary" onClick={onConnect}>
        Connect remote
      </Button>
    );
  const blocked = busy || view.operation !== null;
  if (status.upstream === null)
    return (
      <Button
        variant="primary"
        disabled={blocked || !status.head || !status.branch}
        onClick={() => void onAct({ kind: "push" })}
      >
        Publish branch
      </Button>
    );
  if (status.behind)
    return (
      <Button
        variant="primary"
        disabled={blocked}
        onClick={() => void onAct({ kind: "pull" })}
      >
        Pull {status.behind}
      </Button>
    );
  if (status.ahead)
    return (
      <Button
        variant="primary"
        disabled={blocked}
        onClick={() => void onAct({ kind: "push" })}
      >
        Push {status.ahead}
      </Button>
    );
  return (
    <Button disabled={busy} onClick={() => void onAct({ kind: "fetch" })}>
      Check for updates
    </Button>
  );
}

function OperationBanner({
  view,
  busy,
  onAct,
}: {
  view: RepositoryView;
  busy: boolean;
  onAct: (action: VersionControlAction) => Promise<boolean>;
}) {
  const listed = view.files.filter(
    (file) => file.status === "conflicted",
  ).length;
  const conflicts = listed + view.outsideConflicts;
  const abort = async () => {
    const confirmed = await confirmDialog(
      `Stop this ${view.operation}? Your files return to how they were before it started, including any conflicts you resolved.`,
      {
        title: `Abort ${view.operation}`,
        confirmLabel: "Abort",
        destructive: true,
      },
    );
    if (confirmed) await onAct({ kind: "abortOperation" });
  };
  return (
    <div className="vc-operation" role="status">
      <Icon name="warning" className="h-4 w-4 shrink-0" />
      <p>
        {view.outsideConflicts
          ? `${view.outsideConflicts} conflicted ${view.outsideConflicts === 1 ? "file is" : "files are"} outside this project's folder. Resolve ${view.outsideConflicts === 1 ? "it" : "them"} from a terminal, then commit here.`
          : conflicts
            ? `${conflicts} ${conflicts === 1 ? "file needs" : "files need"} a decision: choose which version to keep. Then commit to finish.`
            : `All conflicts are resolved. Commit to finish the ${view.operation}.`}
      </p>
      <Button disabled={busy} onClick={() => void abort()}>
        Abort
      </Button>
    </div>
  );
}

export default function VersionControl({
  workspaceId,
  active,
  onAttachFolder,
  onFilesChanged,
}: {
  workspaceId: string;
  active: boolean;
  onAttachFolder: () => void;
  /** Called after an action that may have rewritten files in the folder. */
  onFilesChanged: () => void;
}) {
  const [session, update] = usePageSession(workspaceId);
  const { state, stamp, message, actionError, busy, running } = session;
  // A failed action stays on screen until the next action; a failed refresh
  // clears on the next good one.
  const [loadError, setLoadError] = useState("");
  const error = actionError || loadError;
  const [view, setView] = useState<"changes" | "history">("changes");
  const [connecting, setConnecting] = useState(false);
  const setMessage = useCallback(
    (next: { summary: string; description: string }) =>
      update({ message: next }),
    [update],
  );

  const load = useCallback(async () => {
    if (readSession(workspaceId).busy) return;
    const request = nextRequest(workspaceId);
    try {
      const next = await versionControlClient.state(workspaceId);
      if (!isNewestRequest(workspaceId, request)) return;
      update((current) => ({ state: next, stamp: current.stamp + 1 }));
      setLoadError("");
    } catch (cause) {
      if (isNewestRequest(workspaceId, request))
        setLoadError(workbenchErrorMessage(cause));
    }
  }, [workspaceId, update]);

  // Offers Git's merge message, and takes it back if the merge is abandoned.
  const merged = state?.state === "ready" ? (state.operationMessage ?? "") : "";
  useEffect(() => {
    if (merged === readSession(workspaceId).offered) return;
    update(({ offered: previous, message: current }) => ({
      offered: merged,
      message:
        merged && !current.summary
          ? { ...current, summary: merged }
          : !merged && current.summary === previous
            ? { ...current, summary: "" }
            : current,
    }));
  }, [merged, workspaceId, update]);

  useEffect(() => {
    if (!active) return;
    void load();
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void load();
    }, REFRESH_MS);
    const onFocus = () => void load();
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [active, load]);

  const act = useCallback(
    async (action: VersionControlAction) => {
      const drafts = draftsIn(
        action,
        unsavedDraftPaths(workspaceFileAdapter({ workspaceId }).id),
      );
      if (
        drafts.length &&
        !(await confirmDialog(
          `Git uses the saved copy of a file, not your unsaved draft of ${drafts.join(", ")}. Your draft stays in Files, where you can compare it with the file afterwards.`,
          { title: "Unsaved draft", confirmLabel: "Continue" },
        ))
      )
        return false;
      nextRequest(workspaceId);
      update({
        busy: true,
        actionError: "",
        running: action.kind in NETWORK ? (action.kind as NetworkKind) : null,
      });
      try {
        const result = await versionControlClient.act(workspaceId, action);
        update((current) => ({
          state: result.state,
          stamp: current.stamp + 1,
        }));
        if (result.undoneMessage !== null) {
          const [summary, ...rest] = result.undoneMessage.split("\n");
          update(({ message: current }) => ({
            message:
              current.summary.trim() || current.description.trim()
                ? current
                : { summary, description: rest.join("\n").trim() },
          }));
          setView("changes");
        }
        if (result.notice) notify(result.notice, "info");
        if (
          [
            "pull",
            "switchBranch",
            "discard",
            "abortOperation",
            "resolve",
          ].includes(action.kind)
        )
          onFilesChanged();
        return true;
      } catch (cause) {
        update({ actionError: workbenchErrorMessage(cause) });
        return false;
      } finally {
        update({ busy: false, running: null });
      }
    },
    [workspaceId, update, onFilesChanged],
  );

  const openFile = (path: string) =>
    appEvents.emit("open-file", { workspaceId, location: { path } });
  const reportError = useCallback(
    (nextError: string) => update({ actionError: nextError }),
    [update],
  );

  if (!state)
    return (
      <div className="vc-page">
        {error ? (
          <p className="vc-error" role="alert">
            {error}
          </p>
        ) : (
          <div className="vc-diff-message">
            <Spinner label="Loading" />
          </div>
        )}
      </div>
    );

  if (state.state !== "ready")
    return (
      <div className="vc-page">
        {error && (
          <p className="vc-error" role="alert">
            {error}
          </p>
        )}
        <RepositorySetup
          state={state}
          busy={busy}
          onAct={act}
          onAttachFolder={onAttachFolder}
        />
      </div>
    );

  const { status } = state;
  const { remote, branch } = status;
  const changeCount = state.files.length;
  const pullRequest =
    branch && status.upstream && !["main", "master"].includes(branch)
      ? pullRequestUrl(remote, branch)
      : null;
  return (
    <div className="vc-page">
      <div className="vc-bar">
        <BranchMenu view={state} busy={busy} onAct={act} />
        {remote && (
          <span
            className="vc-remote"
            title={
              remote.host
                ? `${remote.host}/${remote.owner}/${remote.repo}`
                : remote.name
            }
          >
            {remoteSummary(remote)}
          </span>
        )}
        <span className="flex-1" />
        {remote && status.upstream && !running && (
          <span className="vc-checked">{checkedLabel(status.fetchedAt)}</span>
        )}
        {pullRequest && (
          <Button onClick={() => void openExternal(pullRequest)}>
            Open pull request
          </Button>
        )}
        <SyncButton
          view={state}
          running={running}
          busy={busy}
          onAct={act}
          onCancel={() => void versionControlClient.cancel(workspaceId)}
          onConnect={() => setConnecting(true)}
        />
        <MoreMenu
          view={state}
          busy={busy}
          onAct={act}
          onRemote={() => setConnecting(true)}
        />
      </div>
      {error && (
        <p className="vc-error" role="alert">
          {error}
        </p>
      )}
      {state.operation && (
        <OperationBanner view={state} busy={busy} onAct={act} />
      )}
      <div className="vc-tabs">
        <SegmentedControl
          label="Version control view"
          value={view}
          onChange={setView}
          options={[
            {
              value: "changes",
              label: changeCount ? `Changes (${changeCount})` : "Changes",
            },
            { value: "history", label: "History" },
          ]}
        />
        {state.folderPrefix && (
          <span className="vc-checked">
            Showing only this project&rsquo;s folder in the repository
          </span>
        )}
      </div>
      <div className="vc-split">
        {view === "changes" ? (
          <ChangesPane
            workspaceId={workspaceId}
            view={state}
            busy={busy}
            stamp={stamp}
            message={message}
            onMessage={setMessage}
            onAct={act}
            onOpenFile={openFile}
          />
        ) : (
          <HistoryPane
            workspaceId={workspaceId}
            remote={remote}
            stamp={status.head}
            onError={reportError}
          />
        )}
      </div>
      <Sheet
        open={connecting}
        onClose={() => setConnecting(false)}
        title={remote ? "Change remote" : "Connect remote"}
        width={420}
      >
        <ConnectRemote
          busy={busy}
          onAct={act}
          onDone={() => setConnecting(false)}
        />
      </Sheet>
    </div>
  );
}
