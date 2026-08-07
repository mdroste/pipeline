import { useId } from "react";
import type { DepsReport } from "../lib/types";
import useModalDialog from "../hooks/useModalDialog";

interface Props {
  report: DepsReport;
  onDismiss: () => void;
  onRefresh?: () => void;
  refreshing?: boolean;
}

export default function DepsCheck({
  report,
  onDismiss,
  onRefresh,
  refreshing = false,
}: Props) {
  const titleId = useId();
  const descriptionId = useId();
  const dialogRef = useModalDialog<HTMLDivElement>(onDismiss);
  const missing = report.deps.filter((d) => !d.found);
  const unauthenticated = report.deps.filter((d) => d.found && d.required && d.authenticated === false);
  const unverifiedRequired = report.deps.filter(
    (d) =>
      d.found &&
      d.required &&
      d.authenticated !== true &&
      d.cli_auth_status === "unknown" &&
      !/gemini/i.test(d.name),
  );
  const missingRequired = missing.filter((d) => d.required);
  const allGood =
    report.ready &&
    missing.length === 0 &&
    unauthenticated.length === 0 &&
    unverifiedRequired.length === 0;
  const hasBlockers = !report.ready;

  const authLabel = (dep: DepsReport["deps"][number]) => {
    if (dep.cli_auth_status === "signed_in") return "signed in";
    if (dep.cli_auth_status === "signed_out") return "not signed in";
    if (dep.cli_auth_status === "unknown") return "sign-in not verified";
    return null;
  };

  const needsAuthAttention = (dep: DepsReport["deps"][number]) =>
    dep.cli_auth_status === "signed_out" || dep.cli_auth_status === "unknown";

  return (
    <div data-testid="dependencies-modal" className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        className="bg-white dark:bg-gray-900 rounded-xl shadow-2xl max-w-lg w-full p-6"
      >
        <h2 id={titleId} className="text-lg font-bold text-gray-900 dark:text-gray-100 mb-1">
          Dependencies
        </h2>
        <p id={descriptionId} className="text-sm text-gray-600 dark:text-gray-300 mb-4">
          {allGood
            ? "All dependencies found."
            : hasBlockers
              ? "Some required dependencies or sign-ins need attention."
              : "Some optional tools are unavailable."}
        </p>

        <div className="space-y-2 mb-4">
          {report.deps.map((dep) => (
            <div
              key={dep.name}
              className={`flex items-start gap-3 p-2.5 rounded-lg text-sm ${
                dep.found && !needsAuthAttention(dep)
                  ? "bg-green-50 dark:bg-green-950/35"
                  : dep.found
                    ? "bg-orange-50 dark:bg-orange-950/35"
                    : dep.required
                      ? "bg-red-50 dark:bg-red-950/35"
                      : "bg-yellow-50 dark:bg-yellow-950/35"
              }`}
            >
              <span className="mt-0.5 shrink-0">
                {dep.found && !needsAuthAttention(dep) ? (
                  <svg className="w-4 h-4 text-green-700 dark:text-green-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                  </svg>
                ) : dep.found ? (
                  <svg className="w-4 h-4 text-orange-700 dark:text-orange-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                ) : dep.required ? (
                  <svg className="w-4 h-4 text-red-600 dark:text-red-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                ) : (
                  <svg className="w-4 h-4 text-yellow-700 dark:text-yellow-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                )}
              </span>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="font-medium text-gray-900 dark:text-gray-100">{dep.name}</span>
                  {dep.found ? (
                    <span className="text-xs text-gray-600 dark:text-gray-300 truncate">
                      {dep.version}
                    </span>
                  ) : (
                    <span className={`text-xs ${
                      dep.required
                        ? "text-red-600 dark:text-red-400"
                        : "text-yellow-700 dark:text-yellow-400"
                    }`}>
                      {dep.required ? "required" : "optional"}
                    </span>
                  )}
                  {authLabel(dep) && (
                    <span
                      className={`text-xs whitespace-nowrap ${
                        dep.cli_auth_status === "signed_in"
                          ? "text-green-700 dark:text-green-400"
                          : "text-orange-700 dark:text-orange-400"
                      }`}
                    >
                      {authLabel(dep)}
                    </span>
                  )}
                </div>
                {(!dep.found || dep.authenticated === false || (
                  dep.cli_auth_status === "unknown" && dep.authenticated !== true
                )) && (
                  <p className="text-xs text-gray-700 dark:text-gray-300 mt-0.5 break-words">
                    {dep.hint}
                  </p>
                )}
                {dep.found && dep.path && (
                  <p className="text-xs text-gray-600 dark:text-gray-400 truncate">{dep.path}</p>
                )}
              </div>
            </div>
          ))}
        </div>

        <div className="flex items-center justify-end gap-2">
          {hasBlockers && (
            <p className="text-sm text-red-600 dark:text-red-400 flex-1">
              {missingRequired.length > 0 && unauthenticated.length > 0
                ? "Required dependencies missing or not signed in."
                : missingRequired.length > 0
                  ? "Required dependencies missing."
                  : unauthenticated.length > 0
                    ? "Required CLI not signed in."
                    : unverifiedRequired.length > 0
                      ? "Required CLI sign-in could not be verified."
                      : "Workflow dependencies are not ready."}
            </p>
          )}
          {onRefresh && (
            <button
              type="button"
              onClick={onRefresh}
              disabled={refreshing}
              className="py-1.5 px-4 border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-200 rounded-lg text-sm hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-50"
            >
              {refreshing ? "Checking…" : "Refresh"}
            </button>
          )}
          <button
            type="button"
            onClick={onDismiss}
            data-testid="dependencies-dismiss"
            data-autofocus
            className="py-1.5 px-4 bg-gray-900 text-white rounded-lg text-sm hover:bg-gray-800"
          >
            {allGood ? "Close" : hasBlockers ? "Dismiss" : "Continue"}
          </button>
        </div>
      </div>
    </div>
  );
}
