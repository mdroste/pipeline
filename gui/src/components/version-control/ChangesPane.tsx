import { useMemo, useRef, useState } from "react";
import { Button, Menu, MenuItem } from "../../ui";
import { Icon } from "../../ui/icons";
import type {
  ChangedFile,
  ConflictChoice,
  RepositoryView,
  VersionControlAction,
} from "../../lib/versionControlClient";
import { confirmDialog } from "../DialogService";
import DiffView from "./DiffView";
import { useExcludedFiles } from "./excludedFiles";
import { FileName, StatusMark } from "./fileRows";

const megabytes = (bytes: number) =>
  `${Math.round(bytes / (1024 * 1024)).toLocaleString()} MB`;

function FileActions({
  file,
  disabled,
  onDiscard,
  onIgnore,
  onOpen,
}: {
  file: ChangedFile;
  disabled: boolean;
  onDiscard: () => void;
  onIgnore: () => void;
  onOpen: () => void;
}) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const isNew = file.status === "untracked" || file.status === "added";
  return (
    <>
      <button
        ref={button}
        type="button"
        className="vc-row-more"
        aria-label={`More actions for ${file.path}`}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        onClick={(event) => {
          event.stopPropagation();
          setOpen(!open);
        }}
      >
        <Icon name="more" className="h-4 w-4" />
      </button>
      <Menu
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={button}
        label={`Actions for ${file.path}`}
      >
        {!file.isDirectory &&
          !file.isSubmodule &&
          file.status !== "deleted" && (
            <MenuItem onSelect={onOpen}>Open in Files</MenuItem>
          )}
        {isNew && <MenuItem onSelect={onIgnore}>Add to .gitignore</MenuItem>}
        {!file.isSubmodule && (
          <MenuItem danger onSelect={onDiscard}>
            Discard changes
          </MenuItem>
        )}
      </Menu>
    </>
  );
}

function ConflictActions({
  path,
  disabled,
  onResolve,
  canChooseSide,
}: {
  path: string;
  disabled: boolean;
  onResolve: (path: string, choice: ConflictChoice) => void;
  canChooseSide: boolean;
}) {
  return (
    <div className="vc-conflict-actions">
      {canChooseSide && (
        <>
          <Button disabled={disabled} onClick={() => onResolve(path, "ours")}>
            Keep this branch&rsquo;s version
          </Button>
          <Button disabled={disabled} onClick={() => onResolve(path, "theirs")}>
            Keep incoming version
          </Button>
        </>
      )}
      <Button disabled={disabled} onClick={() => onResolve(path, "edited")}>
        I fixed this file
      </Button>
    </div>
  );
}

function IdentityForm({
  disabled,
  onSave,
}: {
  disabled: boolean;
  onSave: (name: string, email: string) => void;
}) {
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  return (
    <form
      className="vc-identity"
      onSubmit={(event) => {
        event.preventDefault();
        onSave(name, email);
      }}
    >
      <p>
        Commits record who made them. Add your name and email for this project.
      </p>
      <input
        className="vc-input"
        aria-label="Your name"
        placeholder="Your name"
        value={name}
        onChange={(event) => setName(event.target.value)}
      />
      <input
        className="vc-input"
        aria-label="Your email"
        placeholder="Your email"
        type="email"
        value={email}
        onChange={(event) => setEmail(event.target.value)}
      />
      <Button
        type="submit"
        disabled={disabled || !name.trim() || !email.trim()}
      >
        Save
      </Button>
    </form>
  );
}

export default function ChangesPane({
  workspaceId,
  view,
  busy,
  stamp,
  message,
  onMessage,
  onAct,
  onOpenFile,
}: {
  workspaceId: string;
  view: RepositoryView;
  busy: boolean;
  stamp: unknown;
  /** The commit box, kept for the project so it survives leaving this pane. */
  message: { summary: string; description: string };
  onMessage: (message: { summary: string; description: string }) => void;
  onAct: (action: VersionControlAction) => Promise<boolean>;
  onOpenFile: (path: string) => void;
}) {
  const { files, operation } = view;
  // Unchecked files, so anything newly changed starts out included.
  const [excluded, setExcluded] = useExcludedFiles(workspaceId);
  const [selected, setSelected] = useState<string | null>(null);
  const merging = operation !== null;
  const conflicts = files.filter((file) => file.status === "conflicted");
  const included = merging
    ? files
    : files.filter((file) => !excluded.has(file.path));
  const current =
    files.find((file) => file.path === selected) ?? files[0] ?? null;

  const { summary, description } = message;
  const allIncluded = included.length === files.length;
  const request = useMemo(
    () =>
      current && {
        kind: "working" as const,
        path: current.path,
        oldPath: current.oldPath,
      },
    [current],
  );

  const discard = async (file: ChangedFile) => {
    const isNew = file.status === "untracked" || file.status === "added";
    const confirmed = await confirmDialog(
      isNew
        ? `Move ${file.path} to the Trash?`
        : `Discard your changes to ${file.path}? Your current copy goes to the Trash, and the file returns to its last commit.`,
      { title: "Discard changes", confirmLabel: "Discard", destructive: true },
    );
    if (confirmed) await onAct({ kind: "discard", paths: [file.path] });
  };
  const commit = async () => {
    const done = await onAct({
      kind: "commit",
      paths: included.map((file) => file.path),
      summary,
      description: description.trim() || null,
    });
    if (done) onMessage({ summary: "", description: "" });
  };
  const resolve = async (path: string, choice: ConflictChoice) => {
    if (choice !== "edited") {
      const version =
        choice === "ours"
          ? "this branch\u2019s version"
          : "the incoming version";
      const confirmed = await confirmDialog(
        `Keep ${version} of ${path}? Your current copy goes to the Trash, with any edits you made to it.`,
        { title: "Keep one version", confirmLabel: "Keep", destructive: true },
      );
      if (!confirmed) return;
    }
    await onAct({ kind: "resolve", path, choice });
  };

  if (!files.length && !merging)
    return (
      <>
        <aside className="vc-list" aria-label="Changed files">
          <p className="vc-list-empty">
            Nothing to commit. Your files match the last commit.
          </p>
        </aside>
        <div className="vc-detail" />
      </>
    );

  const unresolved = conflicts.length + view.outsideConflicts;
  const commitLabel = merging
    ? `Commit ${operation}`
    : `Commit ${included.length} ${included.length === 1 ? "file" : "files"}${view.status.branch ? ` to ${view.status.branch}` : ""}`;
  return (
    <>
      <aside className="vc-list" aria-label="Changed files">
        {operation === "rebase" && (
          <p className="vc-banner">
            A rebase is unfinished. Finish it from a terminal, or abort it here.
          </p>
        )}
        {view.largeFiles.length > 0 && (
          <div className="vc-banner" role="status">
            <p>
              GitHub refuses files over 100 MB, and large data slows every copy
              of this repository. Leave these out of Git:
            </p>
            <ul>
              {view.largeFiles.map((file) => (
                <li key={file.path}>
                  <span title={file.path}>
                    {file.path} ({megabytes(file.size)})
                  </span>
                  <Button
                    variant="link"
                    disabled={busy}
                    onClick={() =>
                      void onAct({ kind: "ignore", path: file.path })
                    }
                  >
                    Add to .gitignore
                  </Button>
                </li>
              ))}
            </ul>
          </div>
        )}
        <label className="vc-list-head">
          {!merging && (
            <input
              type="checkbox"
              checked={allIncluded}
              aria-label="Include all files"
              onChange={() =>
                setExcluded(
                  allIncluded
                    ? new Set(files.map((file) => file.path))
                    : new Set(),
                )
              }
            />
          )}
          <span>
            {files.length}
            {view.filesTruncated ? "+" : ""} changed{" "}
            {files.length === 1 ? "file" : "files"}
          </span>
        </label>
        <ul className="vc-files">
          {files.map((file) => (
            <li
              key={file.path}
              className={`vc-file${file.path === current?.path ? " is-selected" : ""}`}
            >
              {!merging && (
                <input
                  type="checkbox"
                  checked={!excluded.has(file.path)}
                  aria-label={`Include ${file.path}`}
                  onChange={() => {
                    const next = new Set(excluded);
                    if (!next.delete(file.path)) next.add(file.path);
                    setExcluded(next);
                  }}
                />
              )}
              <button
                type="button"
                className="vc-file-open"
                aria-current={file.path === current?.path ? "true" : undefined}
                onClick={() => setSelected(file.path)}
              >
                <StatusMark status={file.status} />
                <FileName path={file.path} />
              </button>
              {file.status !== "conflicted" && (
                <FileActions
                  file={file}
                  disabled={busy}
                  onDiscard={() => void discard(file)}
                  onIgnore={() =>
                    void onAct({ kind: "ignore", path: file.path })
                  }
                  onOpen={() => onOpenFile(file.path)}
                />
              )}
            </li>
          ))}
        </ul>
        {view.filesTruncated && (
          <p className="vc-list-note">
            Showing the first {files.length.toLocaleString()} changed files. The
            rest appear after you commit these.
          </p>
        )}
        <div className="vc-commit">
          {view.identity ? (
            <>
              <input
                className="vc-input"
                aria-label="Commit summary"
                placeholder="Summary of this change"
                value={summary}
                onChange={(event) =>
                  onMessage({ summary: event.target.value, description })
                }
              />
              <textarea
                className="vc-input"
                aria-label="Commit description"
                placeholder="Description (optional)"
                rows={2}
                value={description}
                onChange={(event) =>
                  onMessage({ summary, description: event.target.value })
                }
              />
              <Button
                variant="primary"
                className="w-full"
                disabled={
                  busy ||
                  !summary.trim() ||
                  (!merging && !included.length) ||
                  unresolved > 0 ||
                  operation === "rebase"
                }
                onClick={() => void commit()}
              >
                {unresolved
                  ? `Resolve ${unresolved} ${unresolved === 1 ? "conflict" : "conflicts"} first`
                  : commitLabel}
              </Button>
            </>
          ) : (
            <IdentityForm
              disabled={busy}
              onSave={(name, email) =>
                void onAct({ kind: "setIdentity", name, email })
              }
            />
          )}
        </div>
      </aside>
      <section className="vc-detail" aria-label="File changes">
        {current && request && (
          <>
            <header className="vc-detail-head">
              <StatusMark status={current.status} />
              <div className="min-w-0 flex-1">
                <h2 title={current.path}>{current.path}</h2>
                {(current.oldPath || current.conflict) && (
                  <p>
                    {current.conflict
                      ? `Conflict: ${current.conflict}`
                      : `Renamed from ${current.oldPath}`}
                  </p>
                )}
              </div>
              {current.status !== "deleted" &&
                !current.isDirectory &&
                !current.isSubmodule && (
                  <Button onClick={() => onOpenFile(current.path)}>
                    Open in Files
                  </Button>
                )}
            </header>
            {current.status === "conflicted" && (
              <ConflictActions
                path={current.path}
                disabled={busy}
                onResolve={(path, choice) => void resolve(path, choice)}
                canChooseSide={operation !== "rebase"}
              />
            )}
            <DiffView
              workspaceId={workspaceId}
              request={request}
              stamp={stamp}
            />
          </>
        )}
      </section>
    </>
  );
}
