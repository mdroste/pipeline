import type { DepsReport } from "../lib/types";

interface Props {
  report: DepsReport;
  onDismiss: () => void;
}

export default function DepsCheck({ report, onDismiss }: Props) {
  const missing = report.deps.filter((d) => !d.found);
  const unauthenticated = report.deps.filter((d) => d.found && d.required && d.authenticated === false);
  const missingRequired = missing.filter((d) => d.required);
  const allGood = missing.length === 0 && unauthenticated.length === 0;
  const hasBlockers = missingRequired.length > 0 || unauthenticated.length > 0;

  return (
    <div data-testid="dependencies-modal" className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div className="bg-white rounded-xl shadow-2xl max-w-lg w-full p-6">
        <h2 className="text-lg font-bold text-gray-900 mb-1">
          Dependencies
        </h2>
        <p className="text-sm text-gray-500 mb-4">
          {allGood
            ? "All dependencies found."
            : "Pipeline requires a few tools to function."}
        </p>

        <div className="space-y-2 mb-4">
          {report.deps.map((dep) => (
            <div
              key={dep.name}
              className={`flex items-start gap-3 p-2.5 rounded-lg text-sm ${
                dep.found && dep.authenticated !== false
                  ? "bg-green-50"
                  : dep.found && dep.authenticated === false
                    ? "bg-orange-50"
                    : dep.required
                      ? "bg-red-50"
                      : "bg-yellow-50"
              }`}
            >
              <span className="mt-0.5 shrink-0">
                {dep.found && dep.authenticated !== false ? (
                  <svg className="w-4 h-4 text-green-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                  </svg>
                ) : dep.found && dep.authenticated === false ? (
                  <svg className="w-4 h-4 text-orange-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                ) : dep.required ? (
                  <svg className="w-4 h-4 text-red-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                ) : (
                  <svg className="w-4 h-4 text-yellow-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                )}
              </span>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="font-medium text-gray-900">{dep.name}</span>
                  {dep.found && dep.authenticated !== false ? (
                    <span className="text-xs text-gray-500 truncate">
                      {dep.version}
                    </span>
                  ) : dep.found && dep.authenticated === false ? (
                    <span className="text-xs text-orange-600">not signed in</span>
                  ) : (
                    <span className={`text-xs ${dep.required ? "text-red-600" : "text-yellow-600"}`}>
                      {dep.required ? "required" : "optional"}
                    </span>
                  )}
                </div>
                {(!dep.found || dep.authenticated === false) && (
                  <p className="text-xs text-gray-600 mt-0.5 break-words">
                    {dep.hint}
                  </p>
                )}
                {dep.found && dep.authenticated !== false && dep.path && (
                  <p className="text-xs text-gray-400 truncate">{dep.path}</p>
                )}
              </div>
            </div>
          ))}
        </div>

        <div className="flex items-center justify-end">
          {hasBlockers && (
            <p className="text-sm text-red-600 flex-1">
              {missingRequired.length > 0 && unauthenticated.length > 0
                ? "Required dependencies missing or not signed in."
                : missingRequired.length > 0
                  ? "Required dependencies missing."
                  : "Required CLI not signed in."}
            </p>
          )}
          <button
            onClick={onDismiss}
            data-testid="dependencies-dismiss"
            className="py-1.5 px-4 bg-gray-900 text-white rounded-lg text-sm hover:bg-gray-800"
          >
            {allGood ? "Close" : hasBlockers ? "Dismiss" : "Continue"}
          </button>
        </div>
      </div>
    </div>
  );
}
