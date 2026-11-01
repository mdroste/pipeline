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

/** The select's value for the free-text choice. Options use their position. */
const OTHER_CHOICE = "other";

/** How a question was answered: a listed option, or the researcher's own words. */
interface Answer {
  choice: string;
  written: string;
}

function questionOptions(question: Record<string, unknown>) {
  return Array.isArray(question.options)
    ? (question.options as Array<Record<string, unknown>>)
    : [];
}

/** What a question's answer says, or null while it has none. */
function answerText(
  question: Record<string, unknown>,
  answer: Answer | undefined,
) {
  const options = questionOptions(question);
  if (!options.length || answer?.choice === OTHER_CHOICE)
    return text(answer?.written);
  const chosen = options[Number(answer?.choice || NaN)];
  return chosen ? String(chosen.label) : null;
}

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

/** The grants a command approval may carry, by the pinned protocol. */
const PERMISSION_FIELDS = new Map([
  ["fileSystem", ["entries", "globScanMaxDepth", "read", "write"]],
  ["network", ["enabled"]],
]);

/** Whether a command approval would grant something Pipeline can't describe. */
function hasUnknownPermission(permissions: unknown) {
  if (permissions === null || permissions === undefined) return false;
  const isRecord = (value: unknown): value is Record<string, unknown> =>
    typeof value === "object" && value !== null && !Array.isArray(value);
  if (!isRecord(permissions)) return true;
  return Object.entries(permissions).some(([field, value]) => {
    const known = PERMISSION_FIELDS.get(field);
    if (!known) return true;
    if (value === null) return false;
    return (
      !isRecord(value) ||
      Object.keys(value).some((name) => !known.includes(name))
    );
  });
}

function networkRules(value: unknown) {
  if (!Array.isArray(value)) return [];
  return value.flatMap((rule) => {
    const { action, host } = (rule ?? {}) as Record<string, unknown>;
    return typeof action === "string" && typeof host === "string"
      ? [`${action} ${host}`]
      : [];
  });
}

/** Whether the request lets Pipeline answer with `decision`; older requests don't list any. */
function offers(params: Record<string, unknown>, decision: string) {
  const { availableDecisions } = params;
  return !Array.isArray(availableDecisions)
    ? true
    : availableDecisions.includes(decision);
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
  const permissions = params.additionalPermissions ?? null;
  const rules = networkRules(params.proposedNetworkPolicyAmendments);
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
    permissions === null &&
    !rules.length &&
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
      {permissions !== null && (
        <div>
          <span className="font-medium">
            Allow once also grants this command these permissions
          </span>
          <pre className={pre}>{JSON.stringify(permissions, null, 2)}</pre>
        </div>
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
      {rules.length > 0 && (
        <div>
          <span className="font-medium">
            Suggested network rules (Allow once does not apply them)
          </span>
          <pre className={pre}>{rules.join("\n")}</pre>
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
  const [answers, setAnswers] = useState<Record<string, Answer>>({});
  const questionId = (question: Record<string, unknown>) =>
    String(question.id ?? "question");
  const updateAnswer = (id: string, change: Partial<Answer>) =>
    setAnswers((old) => ({
      ...old,
      [id]: { ...(old[id] ?? { choice: "", written: "" }), ...change },
    }));
  const allAnswered =
    questions.length > 0 &&
    questions.every(
      (question) =>
        answerText(question, answers[questionId(question)]) !== null,
    );
  const isQuestion = event.method === "item/tool/requestUserInput";
  const isPermission = event.method === "item/permissions/requestApproval";
  const isApproval = !isQuestion && !isPermission;
  const unknownGrant =
    isApproval && hasUnknownPermission(params.additionalPermissions);
  const withheld = isApproval && (unknownGrant || !offers(params, "accept"));
  const declining =
    !offers(params, "decline") && offers(params, "cancel")
      ? "cancel"
      : "decline";
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
        const id = questionId(question);
        const prompt = String(question.question ?? "Response");
        const options = questionOptions(question);
        const answer = answers[id];
        const field =
          "mt-1 w-full rounded border bg-white p-2 dark:bg-neutral-900";
        const written = (label?: string) => (
          <input
            type={question.isSecret ? "password" : "text"}
            aria-label={label}
            value={answer?.written ?? ""}
            onChange={(e) => updateAnswer(id, { written: e.target.value })}
            className={field}
          />
        );
        return (
          <div key={id} className="mt-3">
            <label className="block">
              <span className="block font-medium">{prompt}</span>
              {options.length ? (
                <select
                  value={answer?.choice ?? ""}
                  onChange={(e) => updateAnswer(id, { choice: e.target.value })}
                  className={field}
                >
                  <option value="">Choose…</option>
                  {options.map((option, index) => (
                    <option key={index} value={String(index)}>
                      {String(option.label)}
                    </option>
                  ))}
                  {question.isOther === true && (
                    <option value={OTHER_CHOICE}>Other…</option>
                  )}
                </select>
              ) : (
                written()
              )}
            </label>
            {options.length > 0 &&
              answer?.choice === OTHER_CHOICE &&
              written(`Your answer to: ${prompt}`)}
          </div>
        );
      })}
      {withheld && (
        <p className="mt-2 text-amber-900 dark:text-amber-200">
          {unknownGrant
            ? "Allow once is not available because Pipeline cannot describe everything this request would grant."
            : "Allow once is not available for this request."}
        </p>
      )}
      <div className="mt-3 flex gap-2">
        {!withheld && (
          <button
            type="button"
            disabled={resolving || (isQuestion && !allAnswered)}
            onClick={() => {
              if (isQuestion) {
                void resolve({
                  answers: Object.fromEntries(
                    questions.map((question) => {
                      const id = questionId(question);
                      return [
                        id,
                        { answers: [answerText(question, answers[id])] },
                      ];
                    }),
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
        )}
        <button
          type="button"
          disabled={resolving}
          onClick={() => {
            if (isQuestion) void resolve();
            else if (isPermission)
              void resolve({ permissions: {}, scope: "turn" });
            else void resolve({ decision: declining });
          }}
          className="rounded border border-amber-400 px-3 py-1.5"
        >
          Decline
        </button>
      </div>
    </div>
  );
}
