import SegmentedControl from "../ui/SegmentedControl";
import {
  conversationProviders,
  providerName,
  isApiProvider,
  type ConversationProvider,
} from "../lib/workspaceProviders";
import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import Popover from "../ui/Popover";
import Select, { type SelectOption } from "../ui/Select";
import WorkspaceModelList from "./WorkspaceModelList";
import "./WorkspaceModelPicker.css";
import Skeleton from "../ui/Skeleton";
import { Icon } from "../ui/icons";
import { moveFocusInList } from "../ui/listKeys";
import { runtimeClient } from "../lib/workbench/runtimeClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { AppPreferences } from "../lib/appPreferences";
import type {
  ChatApprovalMode,
  ClaudeRateLimit,
  WorkspaceAccountState,
  WorkspaceModel,
  WorkspaceRateLimits,
} from "../lib/workbench/runtimeTypes";

const effortLabel = (value: string) =>
  ({
    none: "None",
    minimal: "Minimal",
    low: "Low",
    medium: "Medium",
    high: "High",
    xhigh: "Extra high",
    max: "Maximum",
    ultra: "Ultra",
  })[value] ?? value;

export interface AccountSummary {
  account: WorkspaceAccountState | null;
  limits: WorkspaceRateLimits | null;
}

/** Read on demand when the picker opens; either part may be unavailable. */
export async function loadAccountSummary(): Promise<AccountSummary> {
  const [account, limits] = await Promise.all([
    runtimeClient.accountState().catch(() => null),
    runtimeClient.rateLimits().catch(() => null),
  ]);
  return { account, limits };
}

/** Returns saved account-scoped observations, including reports from earlier app sessions. */
export function loadClaudeUsage(): Promise<ClaudeRateLimit | null> {
  return runtimeClient.claudeRateLimits().catch(() => null);
}

const accountLine = (account: WorkspaceAccountState | null) => {
  if (!account) return null;
  if (account.status === "signedOut") return "Not signed in to ChatGPT";
  if (account.status === "unsupported")
    return "This ChatGPT account type is not supported";
  const plan = account.planType
    ? account.planType.charAt(0).toUpperCase() + account.planType.slice(1)
    : null;
  return [account.email ?? "Signed in", plan].filter(Boolean).join(" · ");
};

const clockTime = (at: Date) =>
  at.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });

const resetsText = (resetsAt: number | null) => {
  if (!resetsAt) return "";
  // The account services report seconds; tolerate milliseconds.
  const at = new Date(resetsAt < 1e12 ? resetsAt * 1000 : resetsAt);
  const soon = at.getTime() - Date.now() < 24 * 60 * 60 * 1000;
  return ` · resets ${
    soon
      ? clockTime(at)
      : at.toLocaleDateString([], { month: "short", day: "numeric" })
  }`;
};

export const usageLine = (
  limits: WorkspaceRateLimits | null,
  now = Date.now(),
) => {
  if (!limits) return null;
  const lines = limits.buckets.flatMap((bucket) =>
    [bucket.primary, bucket.secondary].flatMap((window) => {
      if (!window) return [];
      const minutes = window.windowDurationMins;
      const label =
        minutes === 10080
          ? "weekly"
          : minutes && minutes % 60 === 0
            ? `${minutes / 60}-hour`
            : minutes
              ? `${minutes}-minute`
              : "usage";
      const prefix = bucket.limitName ? `${bucket.limitName}: ` : "";
      const reset = window.resetsAt
        ? window.resetsAt < 1e12
          ? window.resetsAt * 1000
          : window.resetsAt
        : null;
      if (reset && reset <= now)
        return [`${prefix}${label} usage needs a fresh report`];
      return [
        `${prefix}${Math.round(window.remainingPercent)}% of your ${label} limit left${resetsText(window.resetsAt)}`,
      ];
    }),
  );
  return lines.length ? lines.join(" · ") : null;
};

const CLAUDE_WINDOWS: Record<string, string> = {
  five_hour: "5-hour ",
  seven_day: "weekly ",
  seven_day_opus: "weekly Opus ",
  seven_day_sonnet: "weekly Sonnet ",
};

/** `null` when there is no report, or its window has already reset. */
export const claudeUsageLine = (
  limit: ClaudeRateLimit | null,
  now = Date.now(),
) => {
  if (!limit || (limit.resetsAt && limit.resetsAt * 1000 <= now)) return null;
  const resets = resetsText(limit.resetsAt);
  const window = CLAUDE_WINDOWS[limit.rateLimitType ?? ""] ?? "";
  if (limit.status === "rejected") return `Usage limit reached${resets}`;
  if (limit.isUsingOverage || limit.rateLimitType === "overage")
    return `Using extra usage${resets}`;
  if (limit.usedPercent !== null)
    return `${Math.max(0, Math.round(100 - limit.usedPercent))}% of your ${window}limit left${resets}`;
  if (limit.status === "allowed_warning")
    return `Approaching your ${window}usage limit${resets}`;
  return `Within your usage limit${resets}`;
};

const APPROVAL_MODES: ChatApprovalMode[] = ["ask", "auto", "sandbox"];
const APPROVAL_LABELS: Record<ChatApprovalMode, string> = {
  ask: "Ask",
  auto: "Auto",
  sandbox: "Sandbox only",
};
/** The composer selector names the mode on its own, without its menu. */
const APPROVAL_CHIPS: Record<ChatApprovalMode, string> = {
  ask: "Ask first",
  auto: "Auto approvals",
  sandbox: "Sandbox only",
};
/** Narrow message boxes show the shortest name for each selector. */
const APPROVAL_COMPACT: Record<ChatApprovalMode, string> = {
  ask: "Ask",
  auto: "Auto",
  sandbox: "Sandbox",
};

const approvalDescription = (
  mode: ChatApprovalMode,
  runtime: ConversationProvider,
) =>
  mode === "sandbox"
    ? "Runs allowed commands in the sandbox without approval prompts."
    : mode === "auto"
      ? "The provider reviews tool approvals for you."
      : runtime === "claude" || runtime === "claude-code-api"
        ? "Asks before running a sandboxed shell command."
        : "ChatGPT can ask you to approve a command or additional access.";

const thinkingChipLabel = (effort: string) =>
  !effort
    ? "Default thinking"
    : effort === "none"
      ? "No thinking"
      : `${effortLabel(effort)} thinking`;

interface Props {
  /** Separate selectors (the default), or every choice in one menu. */
  layout?: AppPreferences["composerControls"];
  models: WorkspaceModel[];
  model: string;
  effort: string;
  disabled: boolean;
  onChange: (model: string, effort: string) => void;
  loadAccount?: () => Promise<AccountSummary>;
  loadClaudeUsage?: () => Promise<ClaudeRateLimit | null>;
  runtime?: ConversationProvider;
  onRuntimeChange?: (runtime: ConversationProvider) => void;
  approvalMode?: ChatApprovalMode;
  onApprovalModeChange?: (mode: ChatApprovalMode) => void;
  onClearApprovals?: () => Promise<void>;
}

/**
 * The conversation's provider, model, thinking level and tool approvals,
 * shared by Send and follow-ups. By default each has its own selector beside
 * the message field; a preference folds them into one menu. Saved choices stay
 * visible as warnings and are never silently replaced.
 */
export default function WorkspaceComposerControls({
  layout = "separate",
  ...props
}: Props) {
  return layout === "combined" ? (
    <CombinedControls {...props} />
  ) : (
    <SeparateControls {...props} />
  );
}

type ControlProps = Omit<Props, "layout">;

/** What the saved choices mean against the current catalog. */
function describeSelection(
  models: WorkspaceModel[],
  model: string,
  effort: string,
  runtime: ConversationProvider,
) {
  // Automatic may also include connection defaults; do not infer an effective
  // model from the catalog's isDefault flag when the connection has overrides.
  const selected = models.find((item) => item.model === model);
  const supported = selected?.supportedReasoningEfforts ?? [];
  const unknownModel = Boolean(model) && !selected && !isApiProvider(runtime);
  const unknownEffort =
    Boolean(effort) &&
    !supported.some((item) => item.reasoningEffort === effort);
  const effortOptions: SelectOption[] = [
    {
      value: "",
      label: selected?.defaultReasoningEffort
        ? `Default · ${effortLabel(selected.defaultReasoningEffort)}`
        : "Default",
    },
    ...(unknownEffort
      ? [{ value: effort, label: `${effortLabel(effort)} · unavailable` }]
      : []),
    ...supported.map((item) => ({
      value: item.reasoningEffort,
      label: effortLabel(item.reasoningEffort),
      description:
        item.description !== effortLabel(item.reasoningEffort)
          ? item.description
          : undefined,
    })),
  ];
  return {
    supported,
    unknownModel,
    unknownEffort,
    effortOptions,
    modelName: model ? (selected?.displayName ?? model) : "Auto",
    shownEffort: effort || selected?.defaultReasoningEffort || "",
    thinkingHint: !model
      ? "Uses your connection's default. Choose a model to set a level."
      : !supported.length
        ? selected
          ? "This model does not offer adjustable thinking."
          : "Choose an available model to set a level."
        : unknownEffort
          ? `${effortLabel(effort)} is not offered for this model. Choose another level.`
          : null,
  };
}

/** Account and usage reports, read each time the model menu opens. */
function useAccountDetails(
  open: boolean,
  runtime: ConversationProvider,
  loadAccount?: () => Promise<AccountSummary>,
  loadClaudeUsage?: () => Promise<ClaudeRateLimit | null>,
) {
  const [summary, setSummary] = useState<AccountSummary | null>(null);
  const [summaryState, setSummaryState] = useState<"idle" | "loading" | "done">(
    "idle",
  );
  useEffect(() => {
    if (!open || !loadAccount || runtime !== "codex") return;
    let live = true;
    setSummaryState((state) => (state === "done" ? state : "loading"));
    void loadAccount()
      .then((value) => {
        if (live) setSummary(value);
      })
      .catch(() => undefined)
      .finally(() => {
        if (live) setSummaryState("done");
      });
    return () => {
      live = false;
    };
  }, [open, loadAccount, runtime]);
  // Undefined until loaded. Reread on every open: replies update the report.
  const [claudeUsage, setClaudeUsage] = useState<
    ClaudeRateLimit | null | undefined
  >(undefined);
  useEffect(() => {
    if (!open || !loadClaudeUsage || runtime !== "claude") return;
    let live = true;
    void loadClaudeUsage()
      .catch(() => null)
      .then((value) => {
        if (live) setClaudeUsage(value);
      });
    return () => {
      live = false;
    };
  }, [open, loadClaudeUsage, runtime]);
  return { summary, summaryState, claudeUsage };
}

function AccountDetails({
  runtime,
  loadAccount,
  loadClaudeUsage,
  details: { summary, summaryState, claudeUsage },
}: {
  runtime: ConversationProvider;
  loadAccount?: () => Promise<AccountSummary>;
  loadClaudeUsage?: () => Promise<ClaudeRateLimit | null>;
  details: ReturnType<typeof useAccountDetails>;
}) {
  const account = accountLine(summary?.account ?? null);
  const usage = usageLine(summary?.limits ?? null);
  return (
    <>
      {runtime === "claude" && loadClaudeUsage && (
        <details className="workspace-model-account">
          <summary>
            Claude usage
            <Icon name="chevron-down" className="h-3.5 w-3.5" />
          </summary>
          {claudeUsage === undefined ? (
            <Skeleton label="Loading usage" lines={1} className="mt-2" />
          ) : claudeUsage ? (
            <>
              {(claudeUsage.windows ?? [claudeUsage]).map((window, index) => (
                <div className="mt-0.5" key={window.rateLimitType ?? index}>
                  <div>
                    {claudeUsageLine(window) ??
                      `${CLAUDE_WINDOWS[window.rateLimitType ?? ""] ?? ""}usage needs a fresh report`}
                  </div>
                  <div className="text-ui-meta text-ink-muted">
                    Reported by Claude Code at{" "}
                    {new Date(window.observedAt).toLocaleString()}.
                  </div>
                </div>
              ))}
              <div className="mt-0.5 text-ui-meta text-ink-muted">
                Other Claude sessions may have used more since these reports.
              </div>
            </>
          ) : (
            <div className="mt-0.5 text-ui-meta text-ink-muted">
              Claude Code reports usage while it replies. It will appear here
              after Claude&apos;s next reply.
            </div>
          )}
        </details>
      )}
      {runtime === "codex" &&
        loadAccount &&
        (summaryState === "loading" || account || usage) && (
          <details className="workspace-model-account">
            <summary>
              Account &amp; usage
              <Icon name="chevron-down" className="h-3.5 w-3.5" />
            </summary>
            {summaryState === "loading" && !summary ? (
              <Skeleton label="Loading account" lines={1} className="mt-2" />
            ) : (
              <>
                {account && <div className="mt-0.5 truncate">{account}</div>}
                {usage && (
                  <div className="mt-0.5 text-ui-meta text-ink-muted">
                    {usage}
                  </div>
                )}
                {summary?.limits?.observedAt && (
                  <div className="text-ui-meta text-ink-muted">
                    Reported{" "}
                    {new Date(summary.limits.observedAt).toLocaleString()}
                    {summary.limits.cached ? "; refresh unavailable." : "."}
                  </div>
                )}
              </>
            )}
          </details>
        )}
    </>
  );
}

function BusyNote() {
  return (
    <p
      role="note"
      className="border-b border-line px-3 py-2 text-ui-meta text-ink-muted"
    >
      These settings apply to your next message. You can change them when the
      conversation is idle.
    </p>
  );
}

/** Provider, model ID for API connections, and the catalog. */
function ModelPanel({
  runtime,
  onRuntimeChange,
  models,
  model,
  disabled,
  unknownModel,
  onChange,
  onChoose,
}: {
  runtime: ConversationProvider;
  onRuntimeChange?: (runtime: ConversationProvider) => void;
  models: WorkspaceModel[];
  model: string;
  disabled: boolean;
  unknownModel: boolean;
  onChange: (model: string, effort: string) => void;
  onChoose: (model: string) => void;
}) {
  return (
    <>
      <div className="workspace-model-picker-header">
        <span className="font-medium">Model</span>
        {onRuntimeChange ? (
          <Select
            label="Conversation provider"
            value={runtime}
            disabled={disabled}
            options={[...conversationProviders]}
            onChange={(value) => onRuntimeChange(value as ConversationProvider)}
            className="workspace-model-provider"
          />
        ) : (
          <span className="text-ui-meta text-ink-muted">
            {providerName(runtime)}
          </span>
        )}
      </div>
      {isApiProvider(runtime) && (
        <p className="px-3 pb-2 text-ui-meta text-ink-muted">
          Billed per call through your saved connection.
        </p>
      )}
      {disabled && <BusyNote />}
      {isApiProvider(runtime) && (
        <label className="block px-3 py-2 text-ui-meta">
          Model ID
          <input
            aria-label="Model ID"
            className="mt-1 w-full rounded border border-line bg-surface px-2 py-1"
            value={model}
            disabled={disabled}
            placeholder="Automatic"
            onChange={(event) => onChange(event.target.value, "")}
          />
        </label>
      )}
      <WorkspaceModelList
        key={runtime}
        models={models}
        model={model}
        disabled={disabled}
        unknownModel={unknownModel}
        provider={providerName(runtime)}
        onChange={onChoose}
      />
    </>
  );
}

function ClearApprovals({
  disabled,
  onClearApprovals,
}: {
  disabled: boolean;
  onClearApprovals: () => Promise<void>;
}) {
  const [clearing, setClearing] = useState(false);
  const [result, setResult] = useState("");
  return (
    <>
      <button
        type="button"
        className="mt-2 text-ui-meta text-ink-muted underline"
        disabled={disabled || clearing}
        onClick={async () => {
          setClearing(true);
          try {
            await onClearApprovals();
            setResult("Saved approvals cleared.");
          } catch (cause) {
            setResult(workbenchErrorMessage(cause));
          } finally {
            setClearing(false);
          }
        }}
      >
        Clear saved approvals
      </button>
      {result && (
        <p role="status" className="mt-1 text-ui-meta">
          {result}
        </p>
      )}
    </>
  );
}

/** The earlier single menu, kept as a preference. */
function CombinedControls({
  models,
  model,
  effort,
  disabled,
  onChange,
  loadAccount,
  loadClaudeUsage,
  runtime = "codex",
  onRuntimeChange,
  approvalMode,
  onApprovalModeChange,
  onClearApprovals,
}: ControlProps) {
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const details = useAccountDetails(
    open,
    runtime,
    loadAccount,
    loadClaudeUsage,
  );
  const selection = describeSelection(models, model, effort, runtime);
  const { unknownModel, unknownEffort, shownEffort, thinkingHint } = selection;
  const chip = [
    providerName(runtime),
    selection.modelName,
    shownEffort ? effortLabel(shownEffort) : null,
  ]
    .filter(Boolean)
    .join(" · ");
  const choose = (next: string) => {
    if (!disabled && next !== model) onChange(next, "");
  };
  return (
    <>
      <button
        ref={trigger}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={`Model and thinking: ${chip}`}
        title={chip}
        data-warning={unknownModel || unknownEffort || undefined}
        onClick={() => setOpen((value) => !value)}
        className="workspace-model-chip"
      >
        {(unknownModel || unknownEffort) && (
          <Icon name="warning" className="h-4 w-4 shrink-0" />
        )}
        <span>{chip}</span>
        <Icon name="chevron-down" className="h-3.5 w-3.5 shrink-0" />
      </button>
      <Popover
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={trigger}
        label="Model and thinking"
        side="top"
        align="start"
        width={360}
      >
        <ModelPanel
          runtime={runtime}
          onRuntimeChange={onRuntimeChange}
          models={models}
          model={model}
          disabled={disabled}
          unknownModel={unknownModel}
          onChange={onChange}
          onChoose={choose}
        />
        <div className="border-t border-line px-3 py-2.5">
          <div className="workspace-model-thinking">
            <span className="text-ui-label font-medium">Thinking</span>
            <Select
              label="Thinking"
              className="workspace-model-effort"
              value={effort}
              disabled={disabled || !selection.supported.length}
              onChange={(value) => onChange(model, value)}
              options={selection.effortOptions}
            />
          </div>
          {thinkingHint && (
            <p className="mt-2 text-ui-meta text-ink-muted">{thinkingHint}</p>
          )}
        </div>
        <AccountDetails
          runtime={runtime}
          loadAccount={loadAccount}
          loadClaudeUsage={loadClaudeUsage}
          details={details}
        />
        {!isApiProvider(runtime) && approvalMode && onApprovalModeChange && (
          <div className="border-b border-line px-3 py-2.5">
            <div className="mb-1.5 text-ui-meta font-medium text-ink-muted">
              Tool approvals
            </div>
            <SegmentedControl<ChatApprovalMode>
              label="Tool approvals"
              value={approvalMode}
              disabled={disabled}
              options={APPROVAL_MODES.map((value) => ({
                value,
                label: APPROVAL_LABELS[value],
              }))}
              onChange={onApprovalModeChange}
            />
            <p className="mt-2 text-ui-meta text-ink-muted">
              {approvalDescription(approvalMode, runtime)}
            </p>
            {onClearApprovals && (
              <ClearApprovals
                disabled={disabled}
                onClearApprovals={onClearApprovals}
              />
            )}
          </div>
        )}
      </Popover>
    </>
  );
}

type Menu = "model" | "thinking" | "approvals";

/** One selector per choice: model, thinking, and tool approvals. */
function SeparateControls({
  models,
  model,
  effort,
  disabled,
  onChange,
  loadAccount,
  loadClaudeUsage,
  runtime = "codex",
  onRuntimeChange,
  approvalMode,
  onApprovalModeChange,
  onClearApprovals,
}: ControlProps) {
  const modelChip = useRef<HTMLButtonElement>(null);
  const thinkingChip = useRef<HTMLButtonElement>(null);
  const approvalChip = useRef<HTMLButtonElement>(null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const details = useAccountDetails(
    menu === "model",
    runtime,
    loadAccount,
    loadClaudeUsage,
  );
  const selection = describeSelection(models, model, effort, runtime);
  const toggle = (next: Menu) =>
    setMenu((current) => (current === next ? null : next));
  // A choice closes its menu and returns focus to its selector.
  const done = (anchor: RefObject<HTMLButtonElement | null>) => {
    setMenu(null);
    anchor.current?.focus();
  };
  const modelText = `${providerName(runtime)} · ${selection.modelName}`;
  const thinkingText = thinkingChipLabel(selection.shownEffort);
  return (
    <>
      <ComposerChip
        buttonRef={modelChip}
        label={`Model: ${modelText}`}
        text={modelText}
        compact={selection.modelName}
        warning={selection.unknownModel}
        open={menu === "model"}
        onClick={() => toggle("model")}
      />
      <Popover
        open={menu === "model"}
        onClose={() => setMenu(null)}
        anchorRef={modelChip}
        label="Model"
        side="top"
        align="start"
        width={360}
      >
        <ModelPanel
          runtime={runtime}
          onRuntimeChange={onRuntimeChange}
          models={models}
          model={model}
          disabled={disabled}
          unknownModel={selection.unknownModel}
          onChange={onChange}
          onChoose={(next) => {
            if (disabled) return;
            if (next !== model) onChange(next, "");
            done(modelChip);
          }}
        />
        <AccountDetails
          runtime={runtime}
          loadAccount={loadAccount}
          loadClaudeUsage={loadClaudeUsage}
          details={details}
        />
      </Popover>
      <ComposerChip
        buttonRef={thinkingChip}
        label={thinkingText}
        text={thinkingText}
        compact={
          selection.shownEffort
            ? effortLabel(selection.shownEffort)
            : "Thinking"
        }
        title={selection.thinkingHint ?? thinkingText}
        warning={selection.unknownEffort}
        fixed
        open={menu === "thinking"}
        onClick={() => toggle("thinking")}
      />
      <Popover
        open={menu === "thinking"}
        onClose={() => setMenu(null)}
        anchorRef={thinkingChip}
        label="Thinking"
        side="top"
        align="start"
        width={280}
      >
        <ChoicePanel
          title="Thinking"
          meta={model ? selection.modelName : undefined}
          disabled={disabled}
          value={effort}
          options={selection.effortOptions}
          onChoose={(next) => {
            if (next !== effort) onChange(model, next);
            done(thinkingChip);
          }}
        >
          {selection.thinkingHint && (
            <p className="px-3 pb-2.5 text-ui-meta text-ink-muted">
              {selection.thinkingHint}
            </p>
          )}
        </ChoicePanel>
      </Popover>
      {!isApiProvider(runtime) && approvalMode && onApprovalModeChange && (
        <>
          <ComposerChip
            buttonRef={approvalChip}
            label={`Tool approvals: ${APPROVAL_CHIPS[approvalMode]}`}
            text={APPROVAL_CHIPS[approvalMode]}
            compact={APPROVAL_COMPACT[approvalMode]}
            fixed
            open={menu === "approvals"}
            onClick={() => toggle("approvals")}
          />
          <Popover
            open={menu === "approvals"}
            onClose={() => setMenu(null)}
            anchorRef={approvalChip}
            label="Tool approvals"
            side="top"
            align="start"
            width={300}
          >
            <ChoicePanel<ChatApprovalMode>
              title="Tool approvals"
              disabled={disabled}
              value={approvalMode}
              options={APPROVAL_MODES.map((value) => ({
                value,
                label: APPROVAL_LABELS[value],
                description: approvalDescription(value, runtime),
              }))}
              onChoose={(next) => {
                if (next !== approvalMode) onApprovalModeChange(next);
                done(approvalChip);
              }}
            >
              {onClearApprovals && (
                <div className="border-t border-line px-3 pb-2.5">
                  <ClearApprovals
                    disabled={disabled}
                    onClearApprovals={onClearApprovals}
                  />
                </div>
              )}
            </ChoicePanel>
          </Popover>
        </>
      )}
    </>
  );
}

function ComposerChip({
  buttonRef,
  label,
  text,
  compact,
  title = text,
  warning,
  fixed = false,
  open,
  onClick,
}: {
  buttonRef: RefObject<HTMLButtonElement | null>;
  /** Accessible name; it contains both the full and the compact text. */
  label: string;
  text: string;
  /** Shown instead of the text when the message box is narrow. */
  compact: string;
  title?: string;
  warning?: boolean;
  /** Keeps its full width; only the model selector truncates. */
  fixed?: boolean;
  open: boolean;
  onClick: () => void;
}) {
  return (
    <button
      ref={buttonRef}
      type="button"
      aria-haspopup="dialog"
      aria-expanded={open}
      aria-label={label}
      title={title}
      data-warning={warning || undefined}
      onClick={onClick}
      className={`workspace-model-chip workspace-composer-chip${
        fixed ? " workspace-composer-chip--fixed" : ""
      }`}
    >
      {warning && <Icon name="warning" className="h-4 w-4 shrink-0" />}
      <span>
        <span className="workspace-chip-full">{text}</span>
        <span className="workspace-chip-compact">{compact}</span>
      </span>
      <Icon
        name="chevron-down"
        className="workspace-chip-chevron h-3.5 w-3.5 shrink-0"
      />
    </button>
  );
}

const OPTION = '[role="option"]:not([aria-disabled="true"])';

/** A short single-choice list with each option's description in view. */
function ChoicePanel<T extends string = string>({
  title,
  meta,
  disabled,
  value,
  options,
  onChoose,
  children,
}: {
  title: string;
  meta?: string;
  disabled: boolean;
  value: T | "";
  options: SelectOption<T>[];
  onChoose: (value: T) => void;
  children?: ReactNode;
}) {
  const focusable = options.some((option) => option.value === value)
    ? value
    : options[0]?.value;
  return (
    <>
      <div className="workspace-model-picker-header">
        <span className="font-medium">{title}</span>
        {meta && (
          <span className="min-w-0 truncate text-ui-meta text-ink-muted">
            {meta}
          </span>
        )}
      </div>
      {disabled && <BusyNote />}
      <div
        role="listbox"
        aria-label={title}
        className="workspace-model-list"
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            (document.activeElement as HTMLElement | null)?.click();
          } else moveFocusInList(event, OPTION);
        }}
      >
        {options.map((option) => (
          <div
            key={option.value}
            role="option"
            tabIndex={!disabled && option.value === focusable ? 0 : -1}
            aria-selected={option.value === value}
            aria-disabled={disabled || undefined}
            data-label={option.label}
            data-autofocus={option.value === focusable ? "" : undefined}
            onClick={() => {
              if (!disabled) onChoose(option.value);
            }}
            className="workspace-model-option"
          >
            <span className="min-w-0 flex-1">
              <span className="block truncate font-medium">{option.label}</span>
              {option.description && (
                <span className="workspace-choice-description">
                  {option.description}
                </span>
              )}
            </span>
            <span className="w-4 shrink-0">
              {option.value === value && (
                <Icon name="check" className="h-4 w-4" />
              )}
            </span>
          </div>
        ))}
      </div>
      {children}
    </>
  );
}
