import { useId, useState } from "react";
import type {
  ApprovalDetails as Approval,
  RequestQuestion,
  WorkbenchEvent,
} from "../lib/workbenchTypes";

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

/** What a question's answer says, or null while it has none. */
function answerText(question: RequestQuestion, answer: Answer | undefined) {
  if (!question.options.length || answer?.choice === OTHER_CHOICE)
    return text(answer?.written);
  return question.options[Number(answer?.choice || NaN)]?.label ?? null;
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
  approval,
  item,
}: {
  approval: Approval | null;
  item: unknown;
}) {
  const reason = approval?.reason;
  const command = approval?.command;
  const cwd = approval?.cwd;
  const network = approval?.network;
  const amendment = approval?.suggestedRule ?? [];
  const grantRoot = approval?.grantRoot;
  const permissions = approval?.extraPermissions ?? null;
  const rules = approval?.suggestedNetworkRules ?? [];
  const isFileChange = approval?.type === "fileChange";
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
          <pre className={pre}>{permissions}</pre>
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
  const group = useId();
  const [resolving, setResolving] = useState(false);
  const resolve = async (result?: Record<string, unknown>) => {
    setResolving(true);
    try {
      await onResolve(event, result);
    } finally {
      setResolving(false);
    }
  };
  const details = event.details ?? null;
  const questions = details?.type === "question" ? details.questions : [];
  const permission = details?.type === "permission" ? details : null;
  const approval =
    details?.type === "command" || details?.type === "fileChange"
      ? details
      : null;
  const [answers, setAnswers] = useState<Record<string, Answer>>({});
  const updateAnswer = (id: string, change: Partial<Answer>) =>
    setAnswers((old) => ({
      ...old,
      [id]: { ...(old[id] ?? { choice: "", written: "" }), ...change },
    }));
  const allAnswered =
    questions.length > 0 &&
    questions.every(
      (question) => answerText(question, answers[question.id]) !== null,
    );
  const isQuestion = event.method === "item/tool/requestUserInput";
  const isPermission = event.method === "item/permissions/requestApproval";
  const isApproval = !isQuestion && !isPermission;
  const unknownGrant = approval?.unknownGrant ?? false;
  // A request without Pipeline's record has nothing to show, so nothing to allow.
  const withheld =
    (isApproval && (unknownGrant || !approval?.allowOffered)) ||
    (isPermission && !permission);
  return (
    <div className="rounded-xl border border-amber-300 bg-amber-50 p-4 text-sm dark:border-amber-800 dark:bg-amber-950/30">
      <div className="font-semibold text-amber-950 dark:text-amber-100">
        {isQuestion
          ? "ChatGPT has a question"
          : isPermission
            ? "Permission requested"
            : "Approval requested"}
      </div>
      {isApproval && <ApprovalDetails approval={approval} item={item} />}
      {permission && (
        <div className="mt-2 space-y-2">
          {permission.reason && (
            <p className="whitespace-pre-wrap">
              <span className="font-medium">Assistant’s explanation:</span>{" "}
              {permission.reason}
            </p>
          )}
          <p>Duration: this turn</p>
          <p>Working directory: {permission.cwd ?? "Not specified"}</p>
          <p>Requested permissions (including all paths and network access):</p>
          <pre className="whitespace-pre-wrap break-all rounded border p-2">
            {JSON.stringify(permission.permissions ?? {}, null, 2)}
          </pre>
        </div>
      )}
      {questions.map((question) => {
        const { id, header, options, question: prompt } = question;
        const answer = answers[id];
        const field =
          "mt-1 w-full rounded border bg-white p-2 dark:bg-neutral-900";
        const written = (label?: string) => (
          <input
            type={question.secret ? "password" : "text"}
            aria-label={label}
            value={answer?.written ?? ""}
            onChange={(e) => updateAnswer(id, { written: e.target.value })}
            className={field}
          />
        );
        const row =
          "flex items-start gap-2 rounded border border-amber-200 bg-white p-2 dark:border-neutral-700 dark:bg-neutral-900";
        const radio = "mt-1 accent-gray-900 dark:accent-gray-300";
        return (
          <div key={id} className="mt-3">
            {header && header !== prompt && (
              <div className="text-xs font-semibold uppercase tracking-wide text-amber-900 dark:text-amber-200">
                {header}
              </div>
            )}
            {options.length ? (
              <fieldset>
                <legend className="font-medium">{prompt}</legend>
                <div className="mt-1 space-y-1">
                  {options.map((option, index) => {
                    const description = option.description?.trim();
                    return (
                      <label key={index} className={row}>
                        <input
                          type="radio"
                          name={`${group}-${id}`}
                          checked={answer?.choice === String(index)}
                          onChange={() =>
                            updateAnswer(id, { choice: String(index) })
                          }
                          className={radio}
                        />
                        <span className="min-w-0">
                          <span className="block">{option.label}</span>
                          {description && (
                            <span className="block whitespace-pre-wrap text-xs text-gray-600 dark:text-neutral-400">
                              {description}
                            </span>
                          )}
                        </span>
                      </label>
                    );
                  })}
                  {question.allowsOther && (
                    <label className={row}>
                      <input
                        type="radio"
                        name={`${group}-${id}`}
                        checked={answer?.choice === OTHER_CHOICE}
                        onChange={() =>
                          updateAnswer(id, { choice: OTHER_CHOICE })
                        }
                        className={radio}
                      />
                      <span>Other…</span>
                    </label>
                  )}
                </div>
                {answer?.choice === OTHER_CHOICE &&
                  written(`Your answer to: ${prompt}`)}
              </fieldset>
            ) : (
              <label className="block">
                <span className="block font-medium">{prompt}</span>
                {written()}
              </label>
            )}
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
                    questions.map((question) => [
                      question.id,
                      { answers: [answerText(question, answers[question.id])] },
                    ]),
                  ),
                });
              } else if (isPermission) {
                void resolve({
                  permissions: permission?.permissions,
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
            else
              void resolve({
                decision: approval?.declineDecision ?? "decline",
              });
          }}
          className="rounded border border-amber-400 px-3 py-1.5"
        >
          Decline
        </button>
      </div>
    </div>
  );
}
