import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { router } from "../../lib/router";
import {
  useReviewSetup,
  useReviewSetupLifecycle,
  type ReviewSetup,
} from "./useReviewSetup";
import type { PipelineConfig } from "../../lib/types";

const client = vi.hoisted(() => ({
  executionPlan: vi.fn(),
  runSetup: vi.fn(),
}));
vi.mock("../../lib/appClient", () => ({ appClient: client }));

const check = vi.fn(async () => null);
const setup = {
  configVersion: 0,
  loadRunSetup: vi.fn(async () => undefined),
  checkDependencies: check,
  setRunConfigError: vi.fn(),
} as unknown as ReviewSetup;
const showPage = (page: "main" | "home") =>
  vi.spyOn(router, "getSnapshot").mockReturnValue({
    route: { page },
    revision: 0,
  });

beforeEach(() => {
  check.mockClear();
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it("checks dependencies at once on the review page", () => {
  showPage("main");
  renderHook(() => useReviewSetupLifecycle(setup, null));
  expect(check).toHaveBeenCalledOnce();
});

it("lets the first page load before checking dependencies elsewhere", () => {
  showPage("home");
  renderHook(() => useReviewSetupLifecycle(setup, null));
  vi.advanceTimersByTime(2499);
  expect(check).not.toHaveBeenCalled();
  vi.advanceTimersByTime(1);
  expect(check).toHaveBeenCalledOnce();
});

it("does not check after the app shell has gone", () => {
  showPage("home");
  const page = renderHook(() => useReviewSetupLifecycle(setup, null));
  page.unmount();
  vi.advanceTimersByTime(5000);
  expect(check).not.toHaveBeenCalled();
});

it("stays on checking after a workflow change until a new check reports", async () => {
  showPage("main");
  const ready = { ready: true, deps: [] };
  client.runSetup.mockResolvedValue({ inputMode: "document" });
  client.executionPlan.mockResolvedValue({ readiness: ready });
  const { result } = renderHook(() => {
    const live = useReviewSetup();
    useReviewSetupLifecycle(live, null);
    return live;
  });
  await act(async () => {});
  expect(result.current.depsReport).toEqual(ready);
  expect(result.current.depsLoading).toBe(false);
  expect(client.executionPlan).toHaveBeenCalledTimes(1);

  let finish!: (plan: unknown) => void;
  client.executionPlan.mockImplementationOnce(
    () => new Promise((resolve) => (finish = resolve)),
  );
  // The sidebar shows "Setup needed" for a missing report that is not loading.
  const seen: [unknown, boolean][] = [];
  const record = () =>
    seen.push([result.current.depsReport, result.current.depsLoading]);
  act(() =>
    result.current.handleProfileChange({
      extraction: { input_mode: "document" },
    } as PipelineConfig),
  );
  record();
  await act(async () => {});
  record();
  expect(client.executionPlan).toHaveBeenCalledTimes(2);
  await act(async () => finish({ readiness: ready }));
  record();
  expect(seen).toEqual([
    [null, true],
    [null, true],
    [ready, false],
  ]);
});

it("checks the workflow change against the reset input selection", async () => {
  showPage("home");
  client.runSetup.mockResolvedValue({ inputMode: "document" });
  client.executionPlan.mockResolvedValue({
    readiness: { ready: true, deps: [] },
  });
  const { result } = renderHook(() => {
    const live = useReviewSetup();
    useReviewSetupLifecycle(live, null);
    return live;
  });
  await act(async () => {});
  act(() => result.current.setPaperPath("/papers/draft.pdf"));
  client.runSetup.mockResolvedValue({ inputMode: "none" });
  client.executionPlan.mockClear();
  await act(async () =>
    result.current.handleProfileChange({
      extraction: { input_mode: "none" },
    } as PipelineConfig),
  );
  expect(client.executionPlan).toHaveBeenCalledTimes(1);
  expect(client.executionPlan).toHaveBeenCalledWith(
    expect.objectContaining({ paperPath: "" }),
  );
});
