import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { router, type AppRoute } from "../../lib/router";
import type { DepsReport } from "../../lib/types";
import { useReviewDependencyWarning, type ReviewSetup } from "./useReviewSetup";

const missing = (name: string): DepsReport => ({
  ready: false,
  deps: [
    {
      name,
      found: false,
      version: "",
      path: "",
      required: true,
      hint: "Install or configure model access.",
    },
  ],
});

function renderWarning(initial: DepsReport | null) {
  const setShowDeps = vi.fn();
  const hook = renderHook(
    ({ report }: { report: DepsReport | null }) =>
      useReviewDependencyWarning(
        { depsReport: report, setShowDeps } as unknown as ReviewSetup,
        router.route.page,
      ),
    { initialProps: { report: initial } },
  );
  return { ...hook, setShowDeps };
}

const go = (route: AppRoute) => act(() => router.apply(route));

describe("useReviewDependencyWarning", () => {
  beforeEach(() => router.init({ page: "main" }));

  it("prompts in the new-review setup but not over an open review", () => {
    router.init({ page: "main", runKey: "finished" });
    const { setShowDeps, rerender } = renderWarning(missing("Claude CLI"));
    expect(setShowDeps).not.toHaveBeenCalled();

    go({ page: "main", runKey: "queued" });
    rerender({ report: missing("Claude CLI") });
    expect(setShowDeps).not.toHaveBeenCalled();

    go({ page: "main" });
    rerender({ report: missing("Claude CLI") });
    expect(setShowDeps).toHaveBeenCalledWith(true);
  });

  it("shows an unchanged report once and a changed or recurring one again", () => {
    const { setShowDeps, rerender } = renderWarning(missing("Claude CLI"));
    expect(setShowDeps).toHaveBeenCalledTimes(1);

    // Leaving Settings rechecks and returns an equal, new report object.
    go({ page: "settings" });
    rerender({ report: missing("Claude CLI") });
    go({ page: "main" });
    rerender({ report: missing("Claude CLI") });
    expect(setShowDeps).toHaveBeenCalledTimes(1);

    rerender({ report: missing("Google API") });
    expect(setShowDeps).toHaveBeenCalledTimes(2);

    rerender({ report: { ready: true, deps: [] } });
    rerender({ report: missing("Google API") });
    expect(setShowDeps).toHaveBeenCalledTimes(3);
  });
});
