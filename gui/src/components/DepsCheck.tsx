import { useId } from "react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import type { DepsReport } from "../lib/types";
import useModalDialog from "../hooks/useModalDialog";

interface Props {
  report: DepsReport;
  onDismiss: () => void;
  onOpenPdfSettings?: () => void;
}

type Dependency = DepsReport["deps"][number];
type DependencyGroup = "models" | "pdf";
type StatusTone = "success" | "warning" | "danger" | "neutral";

// Provider rows are named for the model family, whichever access mode the
// report checked; Settings → Providers uses the same names.
const PROVIDER_LABELS: Record<string, string> = {
  "Claude CLI": "Claude",
  "Codex CLI": "ChatGPT",
  "Workflow ChatGPT": "ChatGPT",
  "Google API": "Gemini",
  "Local LLM server": "Local model",
};

const BUNDLED_PDF_DEPENDENCIES = new Set(["pdftoppm", "pdftotext"]);

const HIDDEN_DEPENDENCIES = new Set([
  "Local LLM server",
  "PDF extractor configuration",
  ...BUNDLED_PDF_DEPENDENCIES,
]);

const TONE_STYLES: Record<
  StatusTone,
  { row: string; icon: string; badge: string; text: string }
> = {
  success: {
    row: "bg-emerald-50/70 dark:bg-emerald-950/20",
    icon: "bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-400",
    badge:
      "bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300",
    text: "text-emerald-700 dark:text-emerald-300",
  },
  warning: {
    row: "bg-amber-50/70 dark:bg-amber-950/20",
    icon: "bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-400",
    badge: "bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-300",
    text: "text-amber-700 dark:text-amber-300",
  },
  danger: {
    row: "bg-red-50/80 dark:bg-red-950/25",
    icon: "bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-400",
    badge: "bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-300",
    text: "text-red-700 dark:text-red-300",
  },
  neutral: {
    row: "bg-gray-50/80 dark:bg-gray-800/45",
    icon: "bg-gray-200/70 text-gray-500 dark:bg-gray-800 dark:text-gray-400",
    badge: "bg-gray-200/70 text-gray-600 dark:bg-gray-800 dark:text-gray-300",
    text: "text-gray-500 dark:text-gray-400",
  },
};

const isProviderDependency = (dep: Dependency) => dep.name in PROVIDER_LABELS;

// API-mode providers report "direct API" in place of a CLI version.
const usesApiKey = (dep: Dependency) => dep.version === "direct API";

const dependencyAvailable = (dep: Dependency) => {
  if (!dep.found) return false;
  if (dep.authenticated !== undefined) return dep.authenticated;
  if (dep.cli_auth_status === "signed_in") return true;
  if (dep.cli_auth_status === "signed_out") return false;
  if (dep.cli_auth_status === "unknown") return false;
  return true;
};

const openSetupGuide = (
  event: React.MouseEvent<HTMLAnchorElement>,
  url: string,
) => {
  event.preventDefault();
  void openUrl(url).catch((error) => {
    console.warn("Unable to open dependency setup guide:", error);
  });
};

const missingDependencyLabel = (dep: Dependency) => {
  if (dep.required && /paddleocr-vl/i.test(dep.name)) {
    return "highly recommended";
  }
  return dep.required ? "required" : "optional";
};

/** Version and path stay available as a tooltip for troubleshooting. */
const diagnostics = (dep: Dependency) =>
  [dep.version !== "direct API" && dep.version, dep.path]
    .filter(Boolean)
    .join(" · ") || undefined;

function dependencyTone(dep: Dependency, group: DependencyGroup): StatusTone {
  if (dependencyAvailable(dep)) return "success";
  if (group === "models") return dep.required ? "danger" : "neutral";
  return dep.required ? "danger" : "warning";
}

function StatusIcon({ tone }: { tone: StatusTone }) {
  const styles = TONE_STYLES[tone];
  return (
    <span
      className={`mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full ${styles.icon}`}
    >
      {tone === "success" ? (
        <svg
          aria-hidden="true"
          className="h-3 w-3"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2.4}
            d="M5 13l4 4L19 7"
          />
        </svg>
      ) : tone === "danger" ? (
        <svg
          aria-hidden="true"
          className="h-3 w-3"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2.4}
            d="M6 18L18 6M6 6l12 12"
          />
        </svg>
      ) : tone === "warning" ? (
        <svg
          aria-hidden="true"
          className="h-3 w-3"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2.2}
            d="M12 8v5m0 3h.01"
          />
        </svg>
      ) : (
        <svg
          aria-hidden="true"
          className="h-3 w-3"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path strokeLinecap="round" strokeWidth={2.2} d="M7 12h10" />
        </svg>
      )}
    </span>
  );
}

function providerStatus(dep: Dependency) {
  const local = dep.name === "Local LLM server";
  if (dependencyAvailable(dep)) {
    return local ? "Running" : usesApiKey(dep) ? "API key" : "Subscription";
  }
  if (usesApiKey(dep)) return "No API key";
  if (dep.cli_auth_status === "signed_out") return "Not signed in";
  if (dep.cli_auth_status === "unknown") return "Sign-in not verified";
  if (local) return "Not running";
  return dep.found ? "Not connected" : "Not set up";
}

const linkClass =
  "font-medium underline underline-offset-2 hover:text-gray-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:hover:text-gray-100";

function DependencyHint({
  dep,
  onOpenPdfSettings,
}: {
  dep: Dependency;
  onOpenPdfSettings?: () => void;
}) {
  const pdfSettingsLabel = "Settings → Reviews → PDF Extraction";
  // Older dependency reports use the previous Settings navigation labels.
  const hint = dep.hint
    .replace(/Settings → (?:Workflows → )?PDF Extraction/g, pdfSettingsLabel)
    .replaceAll("Workflow ChatGPT", "ChatGPT");
  const pdfSettingsIndex = hint.indexOf(pdfSettingsLabel);
  const linksToPdfSettings =
    /paddleocr-vl/i.test(dep.name) &&
    pdfSettingsIndex >= 0 &&
    Boolean(onOpenPdfSettings);

  return (
    <>
      {linksToPdfSettings ? (
        <>
          {hint.slice(0, pdfSettingsIndex)}
          <a
            href="#paddleocr-local-engine"
            onClick={(event) => {
              event.preventDefault();
              onOpenPdfSettings?.();
            }}
            className={linkClass}
          >
            {pdfSettingsLabel}
          </a>
          {hint.slice(pdfSettingsIndex + pdfSettingsLabel.length)}
        </>
      ) : (
        hint
      )}
      {dep.help_url && (
        <>
          {" "}
          <a
            href={dep.help_url}
            onClick={(event) => openSetupGuide(event, dep.help_url!)}
            className={linkClass}
          >
            Official installation guide
          </a>
          .
        </>
      )}
    </>
  );
}

function ProviderTile({ dep }: { dep: Dependency }) {
  const tone = dependencyTone(dep, "models");
  return (
    <li
      title={diagnostics(dep)}
      className={`flex items-center gap-2.5 rounded-lg px-3 py-2 ${TONE_STYLES[tone].row}`}
    >
      <StatusIcon tone={tone} />
      <div className="min-w-0">
        <p className="text-sm font-medium leading-5 text-gray-900 dark:text-gray-100">
          {PROVIDER_LABELS[dep.name]}
        </p>
        <p className={`truncate text-xs leading-4 ${TONE_STYLES[tone].text}`}>
          {providerStatus(dep)}
        </p>
      </div>
    </li>
  );
}

function DependencyRow({
  dep,
  onOpenPdfSettings,
}: {
  dep: Dependency;
  onOpenPdfSettings?: () => void;
}) {
  const available = dependencyAvailable(dep);
  const tone = dependencyTone(dep, "pdf");
  const styles = TONE_STYLES[tone];

  return (
    <div
      title={diagnostics(dep)}
      className={`flex items-start gap-3 px-3 py-2.5 text-sm ${styles.row}`}
    >
      <StatusIcon tone={tone} />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span className="font-medium text-gray-900 dark:text-gray-100">
            {dep.name}
          </span>
          <span
            className={`rounded-full px-2 py-0.5 text-[11px] font-medium leading-4 ${styles.badge}`}
          >
            {available ? "installed" : missingDependencyLabel(dep)}
          </span>
        </div>
        {dep.hint && !available && (
          <p className="mt-1 text-xs leading-5 text-gray-600 dark:text-gray-300">
            <DependencyHint dep={dep} onOpenPdfSettings={onOpenPdfSettings} />
          </p>
        )}
      </div>
    </div>
  );
}

function SectionStatus({ ready }: { ready: boolean }) {
  return (
    <span
      className={`shrink-0 rounded-full px-2.5 py-1 text-[11px] font-semibold ${
        ready
          ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300"
          : "bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-300"
      }`}
    >
      {ready ? "Ready" : "Needs attention"}
    </span>
  );
}

export default function DepsCheck({
  report,
  onDismiss,
  onOpenPdfSettings,
}: Props) {
  const titleId = useId();
  const descriptionId = useId();
  const modelsTitleId = useId();
  const pdfTitleId = useId();
  const dialogRef = useModalDialog<HTMLDivElement>(onDismiss);
  const providerDeps = report.deps.filter(isProviderDependency);
  const pdfDeps = report.deps.filter((dep) => !isProviderDependency(dep));
  const visible = (dep: Dependency) =>
    !HIDDEN_DEPENDENCIES.has(dep.name) ||
    (dep.required &&
      !dependencyAvailable(dep) &&
      !BUNDLED_PDF_DEPENDENCIES.has(dep.name));
  const visibleProviders = providerDeps.filter(visible);
  // Unused providers that are not set up need no remedy here.
  const providerProblems = visibleProviders.filter(
    (dep) => dep.required && !dependencyAvailable(dep) && dep.hint,
  );
  const modelAccessReady = providerDeps.every(
    (dep) => !dep.required || dependencyAvailable(dep),
  );
  const bundledPdfUnavailable = report.deps.some(
    (dep) =>
      BUNDLED_PDF_DEPENDENCIES.has(dep.name) &&
      dep.required &&
      !dependencyAvailable(dep),
  );
  const pdfReady =
    !bundledPdfUnavailable &&
    pdfDeps.every((dep) => !dep.required || dependencyAvailable(dep));
  const hasBlockers = !report.ready;
  const blockerMessage =
    !modelAccessReady && !pdfReady
      ? "Model access and PDF parsing need attention."
      : !modelAccessReady
        ? "Check the required model connections in Settings → Providers."
        : bundledPdfUnavailable
          ? "Bundled PDF tools are unavailable. Reinstall Pipeline."
          : !pdfReady
            ? "Required PDF parsing tools are missing."
            : "Workflow dependencies are not ready.";

  return (
    <div
      data-testid="dependencies-modal"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        className="max-h-[calc(100vh-2rem)] w-full max-w-xl overflow-y-auto rounded-2xl border border-gray-200/80 bg-white p-6 shadow-2xl dark:border-gray-800 dark:bg-gray-900"
      >
        <h2
          id={titleId}
          className="text-xl font-semibold tracking-tight text-gray-950 dark:text-gray-50"
        >
          Dependencies
        </h2>
        <p
          id={descriptionId}
          className="mt-1 text-sm text-gray-600 dark:text-gray-300"
        >
          {hasBlockers
            ? "Complete the highlighted setup before running this workflow."
            : "Pipeline has the model access and parsing tools it needs."}
        </p>

        <div className="mt-5 space-y-4">
          <section
            aria-labelledby={modelsTitleId}
            className="overflow-hidden rounded-xl border border-gray-200 dark:border-gray-700"
          >
            <div className="flex items-start justify-between gap-4 border-b border-gray-200 bg-gray-50/80 px-3.5 py-3 dark:border-gray-700 dark:bg-gray-800/60">
              <div>
                <h3
                  id={modelsTitleId}
                  className="text-sm font-semibold text-gray-900 dark:text-gray-100"
                >
                  Model access
                </h3>
                <p className="mt-0.5 text-xs leading-5 text-gray-600 dark:text-gray-300">
                  Each provider this workflow uses needs a subscription or API
                  key.
                </p>
              </div>
              <SectionStatus ready={modelAccessReady} />
            </div>
            <div className="p-3">
              <ul className="grid grid-cols-[repeat(auto-fit,minmax(8.5rem,1fr))] gap-2">
                {visibleProviders.map((dep) => (
                  <ProviderTile key={dep.name} dep={dep} />
                ))}
              </ul>
              {providerProblems.map((dep) => {
                const label = PROVIDER_LABELS[dep.name];
                return (
                  <p
                    key={dep.name}
                    className="mt-2 text-xs leading-5 text-gray-600 dark:text-gray-300"
                  >
                    {!dep.hint.includes(label) && (
                      <>
                        <span className="font-medium text-gray-900 dark:text-gray-100">
                          {label}:
                        </span>{" "}
                      </>
                    )}
                    <DependencyHint dep={dep} />
                  </p>
                );
              })}
            </div>
          </section>

          <section
            aria-labelledby={pdfTitleId}
            className="overflow-hidden rounded-xl border border-gray-200 dark:border-gray-700"
          >
            <div className="flex items-start justify-between gap-4 border-b border-gray-200 bg-gray-50/80 px-3.5 py-3 dark:border-gray-700 dark:bg-gray-800/60">
              <div>
                <h3
                  id={pdfTitleId}
                  className="text-sm font-semibold text-gray-900 dark:text-gray-100"
                >
                  PDF parsing
                </h3>
                <p className="mt-0.5 text-xs leading-5 text-gray-600 dark:text-gray-300">
                  Tools for reading PDF inputs.
                </p>
              </div>
              <SectionStatus ready={pdfReady} />
            </div>
            <div className="divide-y divide-gray-200/80 dark:divide-gray-700/80">
              {pdfDeps.filter(visible).map((dep) => (
                <DependencyRow
                  key={dep.name}
                  dep={dep}
                  onOpenPdfSettings={onOpenPdfSettings}
                />
              ))}
            </div>
          </section>
        </div>

        <div className="mt-5 flex items-center justify-end gap-2 border-t border-gray-200 pt-4 dark:border-gray-800">
          {hasBlockers && (
            <p className="mr-auto text-sm text-red-600 dark:text-red-400">
              {blockerMessage}
            </p>
          )}
          <button
            type="button"
            onClick={onDismiss}
            data-testid="dependencies-dismiss"
            data-autofocus
            className="rounded-lg bg-gray-900 px-4 py-1.5 text-sm text-white transition-colors hover:bg-gray-800 dark:bg-gray-100 dark:text-gray-900 dark:hover:bg-white"
          >
            {hasBlockers ? "Dismiss" : "Close"}
          </button>
        </div>
      </div>
    </div>
  );
}
