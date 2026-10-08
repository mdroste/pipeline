import { useEffect, useRef, useState } from "react";
import Popover from "../ui/Popover";
import SegmentedControl from "../ui/SegmentedControl";
import Skeleton from "../ui/Skeleton";
import { Icon } from "../ui/icons";
import { moveFocusInList } from "../ui/listKeys";
import { workbenchClient } from "../lib/workbenchClient";
import type {
  WorkspaceAccountState,
  WorkspaceModel,
  WorkspaceRateLimits,
} from "../lib/workbenchTypes";

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
    workbenchClient.accountState().catch(() => null),
    workbenchClient.rateLimits().catch(() => null),
  ]);
  return { account, limits };
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

const usageLine = (limits: WorkspaceRateLimits | null) => {
  const window = limits?.buckets.find((bucket) => bucket.primary)?.primary;
  if (!window) return null;
  let resets = "";
  if (window.resetsAt) {
    // The account service reports seconds; tolerate milliseconds.
    const at = new Date(
      window.resetsAt < 1e12 ? window.resetsAt * 1000 : window.resetsAt,
    );
    const soon = at.getTime() - Date.now() < 24 * 60 * 60 * 1000;
    resets = ` · resets ${
      soon
        ? at.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })
        : at.toLocaleDateString([], { month: "short", day: "numeric" })
    }`;
  }
  return `${Math.round(window.remainingPercent)}% of your usage limit left${resets}`;
};

const OPTION = '[role="option"]:not([aria-disabled="true"])';
const optionClass =
  "flex cursor-default items-start gap-2 rounded-ui-sm px-2.5 py-2 outline-none hover:bg-sunken focus:bg-sunken aria-disabled:opacity-50";

/**
 * The conversation's model and thinking level, shared by Send and
 * follow-ups. One chip shows the current choice; its popover lists what the
 * catalog offers. Saved choices that are no longer offered stay visible as
 * warnings and are never silently replaced.
 */
export default function WorkspaceComposerControls({
  models,
  model,
  effort,
  disabled,
  onChange,
  loadAccount,
}: {
  models: WorkspaceModel[];
  model: string;
  effort: string;
  disabled: boolean;
  onChange: (model: string, effort: string) => void;
  loadAccount?: () => Promise<AccountSummary>;
}) {
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [summary, setSummary] = useState<AccountSummary | null>(null);
  const [summaryState, setSummaryState] = useState<"idle" | "loading" | "done">(
    "idle",
  );
  useEffect(() => {
    if (!open || !loadAccount) return;
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
  }, [open, loadAccount]);

  // Automatic may also include connection defaults; do not infer an effective
  // model from the catalog's isDefault flag when the connection has overrides.
  const selected = models.find((item) => item.model === model);
  const supported = selected?.supportedReasoningEfforts ?? [];
  const unknownModel = Boolean(model) && !selected;
  const unknownEffort =
    Boolean(effort) &&
    !supported.some((item) => item.reasoningEffort === effort);
  const shownEffort = effort || selected?.defaultReasoningEffort || "";
  const chip = [
    model ? (selected?.displayName ?? model) : "Auto",
    shownEffort ? effortLabel(shownEffort) : null,
  ]
    .filter(Boolean)
    .join(" · ");

  const effortDescription = supported.find(
    (item) => item.reasoningEffort === effort,
  )?.description;
  const thinkingHint = !model
    ? "Uses your connection's default. Choose a model to set a level."
    : !supported.length
      ? selected
        ? "This model does not offer adjustable thinking."
        : "Choose an available model to set a level."
      : unknownEffort
        ? `${effortLabel(effort)} is not offered for this model. Choose another level.`
        : !effort
          ? selected?.defaultReasoningEffort
            ? `Uses the model's default: ${effortLabel(selected.defaultReasoningEffort)}.`
            : "Uses the model's default."
          : effortDescription && effortDescription !== effortLabel(effort)
            ? effortDescription
            : null;

  const account = accountLine(summary?.account ?? null);
  const usage = usageLine(summary?.limits ?? null);
  const choose = (next: string) => {
    if (!disabled && next !== model) onChange(next, "");
  };
  const option = (
    value: string,
    title: string,
    description: string | null,
    extra?: { tag?: string; warning?: boolean },
  ) => {
    const current = value === model;
    return (
      <div
        key={value || "automatic"}
        role="option"
        tabIndex={-1}
        aria-selected={current}
        aria-disabled={disabled || undefined}
        data-label={title}
        data-autofocus={current ? "" : undefined}
        onClick={() => choose(value)}
        className={optionClass}
      >
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-2">
            <span className="truncate font-medium">{title}</span>
            {extra?.tag && (
              <span className="shrink-0 rounded-full border border-line px-1.5 text-[11px] leading-4 text-ink-muted">
                {extra.tag}
              </span>
            )}
          </span>
          {description && (
            <span
              className={`mt-0.5 block text-ui-meta ${extra?.warning ? "text-warning" : "text-ink-muted"}`}
            >
              {description}
            </span>
          )}
        </span>
        {current && (
          <Icon
            name={extra?.warning ? "warning" : "check"}
            className={`mt-0.5 h-4 w-4 shrink-0 ${extra?.warning ? "text-warning" : ""}`}
          />
        )}
      </div>
    );
  };

  return (
    <>
      <button
        ref={trigger}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={`Model and thinking: ${chip}`}
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
        width={320}
      >
        {loadAccount && (summaryState === "loading" || account || usage) && (
          <div className="border-b border-line px-3 py-2.5">
            <div className="text-ui-meta font-medium text-ink-muted">
              ChatGPT account
            </div>
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
              </>
            )}
          </div>
        )}
        {disabled && (
          <p
            role="note"
            className="border-b border-line px-3 py-2 text-ui-meta text-ink-muted"
          >
            These settings apply to your next message. You can change them when
            the conversation is idle.
          </p>
        )}
        <div
          role="listbox"
          aria-label="Model"
          className="p-1.5"
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              (document.activeElement as HTMLElement | null)?.click();
            } else moveFocusInList(event, OPTION);
          }}
        >
          {option(
            "",
            "Automatic",
            "Uses the default model from your ChatGPT connection.",
          )}
          {unknownModel &&
            option(model, model, "No longer offered. Choose another model.", {
              warning: true,
            })}
          {models.map((item) =>
            option(item.model, item.displayName, item.description || null, {
              tag: item.isDefault ? "Default" : undefined,
            }),
          )}
        </div>
        <div className="border-t border-line px-3 pb-3 pt-2.5">
          <div className="mb-1.5 text-ui-meta font-medium text-ink-muted">
            Thinking
          </div>
          <SegmentedControl
            label="Thinking"
            value={effort}
            disabled={disabled || !supported.length}
            onChange={(value) => onChange(model, value)}
            options={[
              { value: "", label: "Default" },
              ...(unknownEffort
                ? [
                    {
                      value: effort,
                      label: `${effortLabel(effort)} · unavailable`,
                    },
                  ]
                : []),
              ...supported.map((item) => ({
                value: item.reasoningEffort,
                label: effortLabel(item.reasoningEffort),
              })),
            ]}
          />
          {thinkingHint && (
            <p className="mt-2 text-ui-meta text-ink-muted">{thinkingHint}</p>
          )}
        </div>
      </Popover>
    </>
  );
}
