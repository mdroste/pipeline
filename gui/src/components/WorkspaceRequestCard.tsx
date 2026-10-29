import { useState } from "react";
import type { WorkbenchEvent } from "../lib/workbenchTypes";

const NETWORK_PROTOCOLS: Record<string, string> = {
  http: "HTTP",
  https: "HTTPS",
  socks5Tcp: "SOCKS5 (TCP)",
  socks5Udp: "SOCKS5 (UDP)",
};
/** Long diffs stay inspectable without letting one approval flood the page. */
const DIFF_PREVIEW_CHARACTERS = 20_000;
const FILE_LIST_LIMIT = 50;

const text = (value: unknown) =>
  typeof value === "string" && value.trim() ? value : null;

function networkAccess(value: unknown) {
  if (!value || typeof value !== "object") return null;
  const { host, protocol } = value as Record<string, unknown>;
  if (typeof host !== "string" || !host) return null;
  const label =
    typeof protocol === "string"
      ? (NETWORK_PROTOCOLS[protocol] ?? protocol)
      : null;
  return label ? `${host} over ${label}` : host;
}

interface FileChange {
  path: string;
  action: string;
  diff: string | null;
}
function fileChanges(item: unknown): FileChange[] | null {
  const changes =
    item && typeof item === "object"
      ? (item as Record<string, unknown>).changes
      : null;
  if (!Array.isArray(changes)) return null;
  return changes.flatMap((change) => {
    if (!change || typeof change !== "object") return [];
    const { path, kind, diff } = change as Record<string, unknown>;
    if (typeof path !== "string") return [];
    const type =
      kind && typeof kind === "object"
        ? (kind as Record<string, unknown>).type
        : kind;
    const moveTo =
      kind && typeof kind === "object"
        ? text((kind as Record<string, unknown>).move_path)
        : null;
    const action =
      type === "add"
        ? "Create"
        : type === "delete"
          ? "Delete"
          : moveTo
            ? `Move to ${moveTo} and edit`
            : "Edit";
    return [{ path, action, diff: text(diff) }];
  });
}

/** What a command or file-change approval would allow, shown before any decision. */
function ApprovalDetails({
  params,
  method,
  item,
}: {
  params: Record<string, unknown>;
  method: string | undefined;
  item: unknown;
}) {
  const reason = text(params.reason);
  const command = text(params.command);
  const cwd = text(params.cwd);
  const network = networkAccess(params.networkApprovalContext);
  const amendment = Array.isArray(params.proposedExecpolicyAmendment)
    ? params.proposedExecpolicyAmendment.filter(
        (part): part is string => typeof part === "string",
      )
    : [];
  const grantRoot = text(params.grantRoot);
  const isFileChange = method === "item/fileChange/requestApproval";
  const changes = isFileChange ? fileChanges(item) : null;
  const pre =
    "mt-1 max-h-64 overflow-auto whitespace-pre-wrap break-all rounded border border-amber-300 bg-white/70 p-2 font-mono text-xs text-amber-950 dark:border-amber-800 dark:bg-neutral-950/60 dark:text-amber-100";
  if (
    !reason &&
    !command &&
    !cwd &&
    !network &&
    !amendment.length &&
    !grantRoot &&
    !isFileChange
  )
    return (
      <p className="mt-2 text-amber-900 dark:text-amber-200">
        Review this request before continuing.
      </p>
    );
  return (
    <div className="mt-2 space-y-2 text-amber-900 dark:text-amber-200">
      {reason && (
        <p className="whitespace-pre-wrap">
          <span className="font-medium">Assistant’s explanation:</span> {reason}
        </p>
      )}
      {command && (
        <div>
          <span className="font-medium">Command</span>
          <pre className={pre}>{command}</pre>
        </div>
      )}
      {cwd && (
        <p className="break-all">
          <span className="font-medium">Working directory:</span> {cwd}
        </p>
      )}
      {network && (
        <p className="break-all">
          <span className="font-medium">Network access:</span> {network}
        </p>
      )}
      {amendment.length > 0 && (
        <div>
          <span className="font-medium">
            Suggested rule to run similar commands without asking (Allow once
            does not apply it)
          </span>
          <pre className={pre}>{amendment.join(" ")}</pre>
        </div>
      )}
      {isFileChange &&
        (changes?.length ? (
          <div>
            <span className="font-medium">Files to change</span>
            <ul className="mt-1 space-y-0.5">
              {changes.slice(0, FILE_LIST_LIMIT).map((change, index) => (
                <li key={`${change.path}-${index}`} className="break-all">
                  {change.action}: <code>{change.path}</code>
                </li>
              ))}
            </ul>
            {changes.length > FILE_LIST_LIMIT && (
              <p>and {changes.length - FILE_LIST_LIMIT} more files</p>
            )}
            {changes.some((change) => change.diff) && (
              <details className="mt-1">
                <summary className="cursor-pointer">Show changes</summary>
                <pre className={pre}>
                  {(() => {
                    const diff = changes
                      .filter((change) => change.diff)
                      .map((change) => `${change.path}\n${change.diff}`)
                      .join("\n\n");
                    return diff.length > DIFF_PREVIEW_CHARACTERS
                      ? `${diff.slice(0, DIFF_PREVIEW_CHARACTERS)}\n… (shortened)`
                      : diff;
                  })()}
                </pre>
              </details>
            )}
          </div>
        ) : (
          <p>Pipeline cannot show which files this change would edit.</p>
        ))}
      {grantRoot && (
        <div>
          <span className="font-medium">
            Also asks to allow writes anywhere in this folder for the rest of
            the session
          </span>
          <pre className={pre}>{grantRoot}</pre>
        </div>
      )}
    </div>
  );
}

/** An approval, permission, or question the assistant is waiting on. */
export default function RequestCard({
  event,
  item = null,
  onResolve,
}: {
  event: WorkbenchEvent;
  /** The live or saved item the request refers to, such as a file change. */
  item?: unknown;
  onResolve: (
    event: WorkbenchEvent,
    result?: Record<string, unknown>,
  ) => void | Promise<void>;
}) {
  const [resolving, setResolving] = useState(false);
  const resolve = async (result?: Record<string, unknown>) => {
    setResolving(true);
    try {
      await onResolve(event, result);
    } finally {
      setResolving(false);
    }
  };
  const params = event.params ?? {};
  const questions = Array.isArray(params.questions)
    ? (params.questions as Array<Record<string, unknown>>)
    : [];
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const isQuestion = event.method === "item/tool/requestUserInput";
  const isPermission = event.method === "item/permissions/requestApproval";
  return (
    <div className="rounded-xl border border-amber-300 bg-amber-50 p-4 text-sm dark:border-amber-800 dark:bg-amber-950/30">
      <div className="font-semibold text-amber-950 dark:text-amber-100">
        {isQuestion
          ? "ChatGPT has a question"
          : isPermission
            ? "Permission requested"
            : "Approval requested"}
      </div>
      {!isQuestion && !isPermission && (
        <ApprovalDetails params={params} method={event.method} item={item} />
      )}
      {isPermission && (
        <div className="mt-2 space-y-2">
          {typeof params.reason === "string" && params.reason.trim() && (
            <p className="whitespace-pre-wrap">
              <span className="font-medium">Assistant’s explanation:</span>{" "}
              {params.reason}
            </p>
          )}
          <p>Duration: this turn</p>
          <p>Working directory: {String(params.cwd ?? "Not specified")}</p>
          <p>Requested permissions (including all paths and network access):</p>
          <pre className="whitespace-pre-wrap break-all rounded border p-2">
            {JSON.stringify(params.permissions ?? {}, null, 2)}
          </pre>
        </div>
      )}
      {questions.map((question) => {
        const id = String(question.id ?? "question");
        const options = Array.isArray(question.options)
          ? (question.options as Array<Record<string, unknown>>)
          : [];
        return (
          <label key={id} className="mt-3 block">
            <span className="block font-medium">
              {String(question.question ?? "Response")}
            </span>
            {options.length ? (
              <select
                value={answers[id] ?? ""}
                onChange={(e) =>
                  setAnswers((old) => ({ ...old, [id]: e.target.value }))
                }
                className="mt-1 w-full rounded border bg-white p-2 dark:bg-neutral-900"
              >
                <option value="">Choose…</option>
                {options.map((option) => (
                  <option
                    key={String(option.label)}
                    value={String(option.label)}
                  >
                    {String(option.label)}
                  </option>
                ))}
              </select>
            ) : (
              <input
                type={question.isSecret ? "password" : "text"}
                value={answers[id] ?? ""}
                onChange={(e) =>
                  setAnswers((old) => ({ ...old, [id]: e.target.value }))
                }
                className="mt-1 w-full rounded border bg-white p-2 dark:bg-neutral-900"
              />
            )}
          </label>
        );
      })}
      <div className="mt-3 flex gap-2">
        <button
          type="button"
          disabled={resolving}
          onClick={() => {
            if (isQuestion) {
              void resolve({
                answers: Object.fromEntries(
                  Object.entries(answers).map(([id, answer]) => [
                    id,
                    { answers: [answer] },
                  ]),
                ),
              });
            } else if (isPermission) {
              void resolve({
                permissions: params.permissions as Record<string, unknown>,
                scope: "turn",
              });
            } else {
              void resolve({ decision: "accept" });
            }
          }}
          className="rounded bg-amber-900 px-3 py-1.5 font-medium text-white dark:bg-amber-100 dark:text-amber-950"
        >
          {isQuestion
            ? "Submit"
            : isPermission
              ? "Allow for this turn"
              : "Allow once"}
        </button>
        <button
          type="button"
          disabled={resolving}
          onClick={() => {
            if (isQuestion) void resolve();
            else if (isPermission)
              void resolve({ permissions: {}, scope: "turn" });
            else void resolve({ decision: "decline" });
          }}
          className="rounded border border-amber-400 px-3 py-1.5"
        >
          Decline
        </button>
      </div>
    </div>
  );
}
