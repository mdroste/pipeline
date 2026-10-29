import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import RunSetupPanel from "./RunSetupPanel";
import type { PrimaryInputSelection } from "../lib/types";

vi.mock("./PaperSelector", () => ({
  default: ({ selection }: { selection?: PrimaryInputSelection | null }) => (
    <p>Shown input: {selection?.paths.join(", ") ?? "none"}</p>
  ),
}));
vi.mock("./WorkflowPanel", () => ({ default: () => null }));
vi.mock("./RunParallelAgents", () => ({ default: () => null }));

const selection: PrimaryInputSelection = {
  paths: ["/papers/draft.pdf"],
  interpretation: "document",
  selectionKind: "file",
};

function renderPanel(props: Partial<Parameters<typeof RunSetupPanel>[0]> = {}) {
  localStorage.setItem("pipeline.privacyNoticeAcknowledged.v1", "true");
  return render(
    <RunSetupPanel
      configVersion={0}
      configError={null}
      configLoading={false}
      dependenciesError={null}
      inputMode="document"
      inputSelection={selection}
      listenersReady
      localLlmActive={false}
      paperPath="/papers/draft.pdf"
      preparingRun={false}
      parallelOverrides={null}
      selectionKey={0}
      width={288}
      onConfigureWorkflow={vi.fn()}
      onGenerate={vi.fn()}
      onPaperPathChange={vi.fn()}
      onInputSelectionChange={vi.fn()}
      onPrivacyDetails={vi.fn()}
      onParallelOverridesChange={vi.fn()}
      onProfileChange={vi.fn()}
      onRetryConfig={vi.fn()}
      onRetryDependencies={vi.fn()}
      onResize={vi.fn()}
      {...props}
    />,
  );
}

describe("RunSetupPanel", () => {
  it("shows the input the app holds, so a remount does not hide it", () => {
    renderPanel();
    expect(
      screen.getByText("Shown input: /papers/draft.pdf"),
    ).toBeInTheDocument();
  });

  it("reports a refused launch without blocking another attempt", () => {
    renderPanel({
      launchError:
        "The workflow execution plan could not be prepared: code is invalid",
    });
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("The review could not be started.");
    expect(alert).toHaveTextContent("code is invalid");
    expect(alert).not.toHaveTextContent("could not be loaded");
    expect(
      screen.queryByRole("button", { name: "Retry workflow" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Start review/ })).toBeEnabled();
  });

  it("still blocks Start review when the workflow setup failed to load", () => {
    renderPanel({ configError: "profile unreadable" });
    expect(
      screen.getByText("Workflow setup could not be loaded."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Start review/ })).toBeDisabled();
  });
});
