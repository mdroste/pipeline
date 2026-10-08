import { useState } from "react";
import type { WorkbenchEvent } from "../lib/workbenchTypes";

/** An approval, permission, or question the assistant is waiting on. */
export default function RequestCard({
  event,
  onResolve,
}: {
  event: WorkbenchEvent;
  onResolve: (event: WorkbenchEvent, result?: Record<string, unknown>) => void;
}) {
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
      {!isQuestion && (
        <p className="mt-2 whitespace-pre-wrap text-amber-900 dark:text-amber-200">
          {String(
            params.reason ??
              params.command ??
              "Review this request before continuing.",
          )}
        </p>
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
          onClick={() => {
            if (isQuestion) {
              onResolve(event, {
                answers: Object.fromEntries(
                  Object.entries(answers).map(([id, answer]) => [
                    id,
                    { answers: [answer] },
                  ]),
                ),
              });
            } else if (isPermission) {
              onResolve(event, {
                permissions: params.permissions as Record<string, unknown>,
                scope: "turn",
              });
            } else {
              onResolve(event, { decision: "accept" });
            }
          }}
          className="rounded bg-amber-900 px-3 py-1.5 font-medium text-white dark:bg-amber-100 dark:text-amber-950"
        >
          {isQuestion ? "Submit" : "Allow once"}
        </button>
        <button
          type="button"
          onClick={() => {
            if (isQuestion) onResolve(event);
            else if (isPermission)
              onResolve(event, { permissions: {}, scope: "turn" });
            else onResolve(event, { decision: "decline" });
          }}
          className="rounded border border-amber-400 px-3 py-1.5"
        >
          Decline
        </button>
      </div>
    </div>
  );
}
