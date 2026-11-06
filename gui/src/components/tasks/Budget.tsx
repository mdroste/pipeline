import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatUsd, type UsageEstimate } from "../../lib/usageEstimate";
import type { Chain, Step } from "../../lib/taskTypes";

export function AllowanceField({
  cents,
  onChange,
}: {
  cents?: number;
  onChange: (v: number | undefined) => void;
}) {
  return (
    <label>
      API allowance (USD)
      <input
        type="number"
        min="0"
        max="42949672.95"
        step="0.01"
        placeholder="No limit"
        value={cents === undefined ? "" : cents / 100}
        onChange={(e) =>
          onChange(
            e.target.value === ""
              ? undefined
              : Math.round(e.target.valueAsNumber * 100),
          )
        }
      />
      <small>
        Applies to priced API requests across retries and Reviews. With an
        allowance, an unknown price prevents dispatch.
      </small>
    </label>
  );
}

export function ReviewEstimate({
  profile,
  taskId,
}: {
  profile?: string | null;
  taskId?: string;
}) {
  const [result, setResult] = useState<UsageEstimate | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    setResult(null);
    setError("");
    if (profile)
      void invoke<UsageEstimate>("automation_review_estimate", {
        profileId: profile,
        taskId: taskId ?? null,
      })
        .then((r) => {
          if (live) setResult(r);
        })
        .catch((e) => {
          if (live) setError(String(e));
        });
    return () => {
      live = false;
    };
  }, [profile, taskId]);
  if (!profile) return null;
  return (
    <p>
      {result ? (
        <>
          Per structured Review: {result.callsLow}–{result.callsHigh} model
          calls; up to {result.subscriptionCallsHigh} subscription and{" "}
          {result.localCallsHigh ?? 0} local calls.{" "}
          {result.apiCostHigh != null && (
            <>
              Priced API estimate {formatUsd(result.apiCostLow ?? 0)}–
              {formatUsd(result.apiCostHigh)}.{" "}
            </>
          )}
          {result.unpricedCalls > 0 && (
            <>{result.unpricedCalls} calls have no known price. </>
          )}
          The manuscript length is not yet known.
        </>
      ) : error ? (
        "The Review estimate is unavailable: " + error
      ) : (
        "Estimating the selected Review…"
      )}
    </p>
  );
}

interface Usage {
  pricedCalls: number;
  unpricedCalls: number;
  subscriptionUnits: number;
  localCalls: number;
  chargedMicroUsd: number;
  heldMicroUsd: number;
  inputTokens: number;
  outputTokens: number;
}
export function RecordedUsage({
  id,
  revision,
}: {
  id: string;
  revision: number;
}) {
  const [open, setOpen] = useState(false),
    [usage, setUsage] = useState<Usage | null>(null),
    [error, setError] = useState("");
  useEffect(() => {
    if (!open) return;
    let live = true;
    setError("");
    setUsage(null);
    void invoke<Usage>("automation_usage", { id })
      .then((v) => {
        if (live) setUsage(v);
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
    };
  }, [open, id, revision]);
  return (
    <details onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary>Recorded usage</summary>
      {usage ? (
        <p>
          {usage.pricedCalls} priced API requests · {usage.unpricedCalls}{" "}
          unpriced · {usage.subscriptionUnits} subscription turns or Review
          calls · {usage.localCalls} local calls. API charges at list price:{" "}
          {formatUsd(usage.chargedMicroUsd / 1e6)}, including{" "}
          {formatUsd(usage.heldMicroUsd / 1e6)} reserved for unconfirmed usage.
          Reported tokens: {usage.inputTokens} in, {usage.outputTokens} out.
        </p>
      ) : (
        <p>{error || "Reading usage…"}</p>
      )}
    </details>
  );
}

// Ranges are conditional on branches and iterations. Sum parallel branches:
// execution slots change elapsed time but never discount actions.
type Range = [number, number];
const add = (a: number, b: number) => Math.min(Number.MAX_SAFE_INTEGER, a + b);
const mul = (a: number, b: number) => Math.min(Number.MAX_SAFE_INTEGER, a * b);
function range(steps: Step[]): Range {
  return steps.reduce<Range>(
    ([low, high], s) => {
      let r: Range;
      switch (s.kind) {
        case "if": {
          const a = range(s.thenSteps),
            b = range(s.elseSteps ?? []);
          r = [Math.min(a[0], b[0]), Math.max(a[1], b[1])];
          break;
        }
        case "repeat":
        case "while": {
          const body = range(s.steps);
          r = [
            s.kind === "repeat" ? body[0] : 0,
            mul(body[1], s.maxIterations),
          ];
          break;
        }
        case "forEach": {
          const body = range(s.steps);
          const n =
            s.input.kind === "literal" && Array.isArray(s.input.value)
              ? s.input.value.length
              : null;
          r = [mul(body[0], n ?? 0), mul(body[1], n ?? s.maxItems)];
          break;
        }
        case "chain":
          r = range(s.chain.steps);
          break;
        case "parallel":
          r = range(s.branches.flat());
          break;
        default:
          r = [1, 1];
      }
      return [add(low, r[0]), add(high, r[1])];
    },
    [0, 0],
  );
}
export function ChainPreview({ chain }: { chain: Chain }) {
  const rows: {
    id: string;
    label: string;
    limits: Chain["limits"];
    range: Range;
  }[] = [];
  function visit(steps: Step[], limits: Chain["limits"]) {
    for (const s of steps) {
      if (s.kind === "chain") {
        const l = s.chain.limits;
        const c = [limits.maxApiCostCents, l.maxApiCostCents].filter(
          (x): x is number => x !== undefined,
        );
        const next = {
          maxActions: Math.min(limits.maxActions, l.maxActions),
          actionTimeoutSecs: Math.min(
            limits.actionTimeoutSecs,
            l.actionTimeoutSecs,
          ),
          deadlineHours: Math.min(limits.deadlineHours, l.deadlineHours),
          maxApiCostCents: c.length ? Math.min(...c) : undefined,
        };
        rows.push({
          id: s.id,
          label: s.label,
          limits: next,
          range: range(s.chain.steps),
        });
        visit(s.chain.steps, next);
      } else if (s.kind === "if") {
        visit(s.thenSteps, limits);
        visit(s.elseSteps ?? [], limits);
      } else if (s.kind === "parallel") {
        s.branches.forEach((b) => visit(b, limits));
      } else if ("steps" in s) visit(s.steps, limits);
    }
  }
  visit(chain.steps, chain.limits);
  const [lo, hi] = range(chain.steps);
  return (
    <div>
      <p>
        {lo}–{hi} actions to finish, depending on conditions and iteration
        counts.{" "}
        {lo > chain.limits.maxActions
          ? "The minimum exceeds this run’s action limit."
          : hi > chain.limits.maxActions
            ? "Some paths can exceed the action limit."
            : "The action limit covers this range before retries."}{" "}
        {chain.limits.maxApiCostCents != null && (
          <>API allowance: {formatUsd(chain.limits.maxApiCostCents / 100)}.</>
        )}
      </p>
      {rows.map(({ id, label, limits, range: [min] }) => (
        <p key={id}>
          <strong>{label}:</strong> up to {limits.maxActions} action
          {limits.maxActions === 1 ? "" : "s"} · {limits.deadlineHours} h from
          group entry · {limits.actionTimeoutSecs} s per active step
          {limits.maxApiCostCents != null
            ? ` · ${formatUsd(limits.maxApiCostCents / 100)} API allowance`
            : ""}
          . Parent time and actions already used reduce what remains.{" "}
          {min > limits.maxActions &&
            "This group cannot finish within its action limit."}
        </p>
      ))}
    </div>
  );
}
