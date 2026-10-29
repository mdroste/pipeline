import { useState } from "react";
import type {
  LlmRequestDetails,
  TokenTotals,
} from "../../lib/reviewRuntimeTypes";
import {
  fmtTokens,
  fmtCount,
  fmtDuration,
  hasTokenUsage,
  requestText,
} from "../../lib/consolePresentation";
import TokenUsage from "./TokenUsage";

function TruncatedPreviewBadge({ truncated }: { truncated?: boolean }) {
  if (!truncated) return null;
  return (
    <span className="rounded bg-amber-100 px-1.5 py-px text-[10px] font-medium uppercase tracking-wide text-amber-700 dark:bg-amber-900/50 dark:text-amber-300">
      Preview truncated
    </span>
  );
}

function Fact({
  label,
  title,
  children,
}: {
  label: string;
  title?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="min-w-0 max-w-full">
      <dt className="text-gray-500 dark:text-gray-400">{label}</dt>
      <dd className="truncate text-gray-800 dark:text-gray-200" title={title}>
        {children}
      </dd>
    </div>
  );
}

type RequestPanel = "prompt" | "paths" | null;

function PanelToggle({
  open,
  onClick,
  children,
}: {
  open: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      aria-expanded={open}
      className={`flex items-center gap-1.5 rounded-md px-2 py-1 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 ${
        open
          ? "bg-white text-gray-900 shadow-sm ring-1 ring-gray-200 dark:bg-gray-800 dark:text-gray-100 dark:ring-gray-700"
          : "text-gray-600 hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-200"
      }`}
    >
      <svg
        aria-hidden
        viewBox="0 0 12 12"
        className={`h-2.5 w-2.5 transition-transform ${open ? "rotate-90" : ""}`}
      >
        <path
          d="M4 2.5 7.5 6 4 9.5"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
        />
      </svg>
      {children}
    </button>
  );
}

function PromptSection({
  title,
  truncated,
  text,
}: {
  title: string;
  truncated?: boolean;
  text: string;
}) {
  return (
    <section>
      <div className="mb-1 flex items-center gap-2 text-gray-500 dark:text-gray-400">
        <span>{title}</span>
        <TruncatedPreviewBadge truncated={truncated} />
      </div>
      <pre className="max-h-56 overflow-auto whitespace-pre-wrap rounded-md bg-white px-2.5 py-2 font-mono leading-relaxed text-gray-700 ring-1 ring-gray-200 dark:bg-gray-950 dark:text-gray-300 dark:ring-gray-800">
        {text}
      </pre>
    </section>
  );
}

export default function RequestDetails({
  request,
  usage,
  copyText,
}: {
  request: LlmRequestDetails;
  usage?: TokenTotals;
  copyText: (text: string) => Promise<boolean>;
}) {
  const [panel, setPanel] = useState<RequestPanel>(null);
  const toggle = (next: Exclude<RequestPanel, null>) =>
    setPanel((current) => (current === next ? null : next));
  const transport = request.transport === "api" ? "API" : "CLI";
  const tools = request.tools.length > 0 ? request.tools.join(", ") : "None";
  const context = [
    request.pdf_attached ? "PDF" : null,
    request.shared_context
      ? `${fmtTokens(request.shared_context_chars)} chars shared`
      : null,
    request.write_enabled ? "Writes artifacts" : "Read-only",
  ]
    .filter(Boolean)
    .join(" · ");
  const paths = [
    request.working_directory
      ? { label: "Working directory", path: request.working_directory }
      : null,
    ...request.read_directories.map((path) => ({ label: "Read", path })),
    request.write_directory
      ? { label: "Write", path: request.write_directory }
      : null,
  ].filter((entry): entry is { label: string; path: string } => entry != null);
  const hasTruncatedPreview = Boolean(
    request.prompt_truncated ||
    request.system_prompt_truncated ||
    request.shared_context_truncated,
  );

  return (
    <section
      aria-label="Request details"
      className="mb-2 overflow-hidden rounded-lg border border-gray-200 bg-gray-50 font-sans text-xs text-gray-700 dark:border-gray-800 dark:bg-gray-900/60 dark:text-gray-300"
    >
      <dl className="flex flex-wrap gap-x-6 gap-y-2 px-3 py-2.5">
        <Fact
          label="Provider"
          title={`${request.provider_label} (${request.provider}) via ${request.transport === "api" ? "direct API" : "CLI"}${request.local_endpoint ? ` at ${request.local_endpoint}` : ""}`}
        >
          {request.provider_label}{" "}
          <span className="text-gray-500 dark:text-gray-400">
            · {transport}
          </span>
        </Fact>
        <Fact
          label="Model"
          title={`${request.model} (${request.model_policy})`}
        >
          <span className="font-mono">{request.model}</span>{" "}
          <span className="text-gray-500 dark:text-gray-400">
            · {request.model_policy}
          </span>
        </Fact>
        <Fact label="Effort">{request.effort}</Fact>
        <Fact label="Tools" title={tools}>
          {tools}
        </Fact>
        <Fact
          label="Limits"
          title={`Timeout ${fmtCount(request.timeout_secs)} seconds; ${
            request.max_output_tokens == null
              ? "provider default output limit"
              : `${fmtCount(request.max_output_tokens)} max output tokens`
          }`}
        >
          {fmtDuration(request.timeout_secs)} ·{" "}
          {request.max_output_tokens == null
            ? "default output"
            : `${fmtTokens(request.max_output_tokens)} output`}
        </Fact>
        <Fact label="Context" title={context}>
          {context}
        </Fact>
      </dl>

      {usage && hasTokenUsage(usage) && (
        <div className="border-t border-gray-200 px-3 py-2 dark:border-gray-800">
          <TokenUsage usage={usage} label="Tokens" />
        </div>
      )}

      <div className="flex flex-wrap items-center gap-1 border-t border-gray-200 px-1.5 py-1 dark:border-gray-800">
        <PanelToggle open={panel === "prompt"} onClick={() => toggle("prompt")}>
          Prompt
          <span className="tabular-nums text-gray-500 dark:text-gray-400">
            {fmtCount(request.prompt_chars)} chars
          </span>
          {panel !== "prompt" && (
            <TruncatedPreviewBadge truncated={hasTruncatedPreview} />
          )}
        </PanelToggle>
        {paths.length > 0 && (
          <PanelToggle open={panel === "paths"} onClick={() => toggle("paths")}>
            Execution paths
            <span className="tabular-nums text-gray-500 dark:text-gray-400">
              {paths.length}
            </span>
          </PanelToggle>
        )}
        {panel === "prompt" && (
          <button
            onClick={() => void copyText(requestText(request))}
            className="ml-auto rounded-md px-2 py-1 text-gray-600 transition-colors hover:bg-gray-100 hover:text-gray-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-200"
          >
            Copy prompt
          </button>
        )}
      </div>

      {panel === "prompt" && (
        <div className="space-y-3 border-t border-gray-200 px-3 py-2.5 dark:border-gray-800">
          <p className="text-gray-500 dark:text-gray-400">
            Shown in the order requests were sent.
            {hasTruncatedPreview &&
              " Long fields are shortened; Copy prompt copies only the visible text."}
          </p>
          {request.system_prompt && (
            <PromptSection
              title={`System prompt${
                request.system_prompt_chars !== undefined
                  ? ` (${fmtCount(request.system_prompt_chars)} characters)`
                  : ""
              }`}
              truncated={request.system_prompt_truncated}
              text={request.system_prompt}
            />
          )}
          {request.shared_context && (
            <PromptSection
              title={`Shared context (${fmtCount(request.shared_context_chars)} characters)`}
              truncated={request.shared_context_truncated}
              text={request.shared_context}
            />
          )}
          <PromptSection
            title={`Task prompt (${fmtCount(request.prompt_chars)} characters)`}
            truncated={request.prompt_truncated}
            text={request.prompt}
          />
        </div>
      )}

      {panel === "paths" && (
        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 border-t border-gray-200 px-3 py-2.5 dark:border-gray-800">
          {paths.map(({ label, path }) => (
            <div key={`${label}:${path}`} className="contents">
              <dt className="text-gray-500 dark:text-gray-400">{label}</dt>
              <dd className="break-all font-mono text-gray-700 dark:text-gray-300">
                {path}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </section>
  );
}
