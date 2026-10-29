import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import RunUsageEstimate from "./RunUsageEstimate";
import type { UsageEstimate } from "../lib/usageEstimate";

const base: UsageEstimate = {
  documentTokens: 20_000,
  callsLow: 8,
  callsHigh: 12,
  inputTokensLow: 180_000,
  inputTokensHigh: 400_000,
  outputTokensLow: 30_000,
  outputTokensHigh: 110_000,
  apiCostLow: 0.85,
  apiCostHigh: 2.4,
  unpricedCalls: 0,
  subscriptionCallsHigh: 0,
  maxCostUsd: null,
  maxCalls: null,
  maxMinutes: null,
  exceedsCostLimit: false,
  exceedsCallLimit: false,
};

describe("RunUsageEstimate", () => {
  it("shows ranges, list price, and where to set limits", () => {
    render(<RunUsageEstimate estimate={base} />);
    expect(screen.getByText("8–12")).toBeInTheDocument();
    expect(screen.getByText("180k–400k · 30k–110k")).toBeInTheDocument();
    expect(screen.getByText("$0.850–$2.40")).toBeInTheDocument();
    expect(screen.getByText("None set")).toBeInTheDocument();
    expect(screen.getByText(/Settings → Reviews/)).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("warns when the estimate exceeds a limit and explains subscriptions", () => {
    render(
      <RunUsageEstimate
        estimate={{
          ...base,
          apiCostLow: null,
          apiCostHigh: null,
          subscriptionCallsHigh: 12,
          maxCalls: 10,
          maxMinutes: 30,
          exceedsCallLimit: true,
        }}
        batchCount={3}
      />,
    );
    expect(screen.getByText("Estimated use per document")).toBeInTheDocument();
    expect(screen.queryByText("API list price")).not.toBeInTheDocument();
    expect(screen.getByText("10 model calls · 30 minutes")).toBeInTheDocument();
    expect(screen.getByText(/not billed per token/)).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(
      "saved as a partial report",
    );
  });

  it("renders nothing without an estimate", () => {
    const { container } = render(<RunUsageEstimate />);
    expect(container).toBeEmptyDOMElement();
  });
});
