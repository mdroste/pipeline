import type { FileStatus } from "../../lib/versionControlClient";

const STATUS: Record<FileStatus, { letter: string; label: string }> = {
  modified: { letter: "M", label: "Changed" },
  added: { letter: "A", label: "New" },
  untracked: { letter: "A", label: "New" },
  deleted: { letter: "D", label: "Deleted" },
  renamed: { letter: "R", label: "Renamed" },
  copied: { letter: "C", label: "Copied" },
  conflicted: { letter: "!", label: "Conflict" },
};

export function StatusMark({ status }: { status: FileStatus }) {
  const { letter, label } = STATUS[status];
  return (
    <span className={`vc-status is-${status}`} title={label}>
      <span aria-hidden="true">{letter}</span>
      <span className="sr-only">{label}</span>
    </span>
  );
}

/** The file name first, with its folder quieter beside it. */
export function FileName({ path }: { path: string }) {
  const slash = path.lastIndexOf("/");
  return (
    <span className="vc-file-name" title={path}>
      <span>{slash < 0 ? path : path.slice(slash + 1)}</span>
      {slash >= 0 && (
        <span className="vc-file-folder">{path.slice(0, slash)}</span>
      )}
    </span>
  );
}
