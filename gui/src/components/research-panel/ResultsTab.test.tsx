import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import ResultsTab from "./ResultsTab";
import type { ExecutionProfile } from "../../lib/workbenchTypes";

const mocks = vi.hoisted(() => ({ save: vi.fn() }));
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: { saveExecutionProfile: mocks.save },
}));
vi.mock("../../lib/projectClient", () => ({ projectClient: {} }));
vi.mock("./ToolchainPicker", () => ({ default: () => null }));

const profile: ExecutionProfile = {
  id: "profile",
  workspaceId: "w",
  name: "Estimate",
  adapter: "stata",
  argv: ["stata-mp", "-b", "do", "main.do"],
  cwd: "/project/code",
  environment: { LANG: "C" },
  inputs: ["main.do"],
  timeoutSeconds: 900,
  outputs: ["main.log"],
  testedAt: null,
  testStatus: "passed",
  revision: 4,
};
const preview = {
  preview: {
    profileId: "profile",
    boundary: "Runs with your permissions.",
    command: [],
    cwd: "/project",
    inputs: [],
    launch: {},
    outputs: [],
    fingerprint: "f",
  },
  testOnly: true,
};

function Tab({
  reviewing = false,
  executions = [],
}: {
  reviewing?: boolean;
  executions?: unknown[];
}) {
  const [profileName, setProfileName] = useState("Research command");
  const [adapter, setAdapter] =
    useState<ExecutionProfile["adapter"]>("command");
  const [argv, setArgv] = useState('["/usr/bin/true"]');
  const [inputs, setInputs] = useState("");
  const [outputs, setOutputs] = useState("");
  return (
    <ResultsTab
      busy={false}
      act={async (fn) => {
        await fn().catch((e) => document.body.append(String(e.message)));
      }}
      workspace={{ id: "w", root: "/project" } as never}
      profileName={profileName}
      setProfileName={setProfileName}
      adapter={adapter}
      setAdapter={setAdapter}
      argv={argv}
      setArgv={setArgv}
      inputs={inputs}
      setInputs={setInputs}
      outputs={outputs}
      setOutputs={setOutputs}
      profiles={[profile]}
      hostPreview={reviewing ? (preview as never) : null}
      setHostPreview={vi.fn()}
      executions={executions as never}
      workspaceId="w"
      snapshot={null}
      resultsView="profiles"
      onError={vi.fn()}
    />
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.save.mockResolvedValue({});
});
afterEach(() => {
  cleanup();
  document.body.textContent = "";
});

it("saves the time limit the researcher chose", async () => {
  render(<Tab />);
  const limit = screen.getByLabelText("Time limit (seconds)");
  expect(limit).toHaveValue(300);
  expect(screen.getByText(/The default is 300 seconds/)).toBeInTheDocument();
  fireEvent.change(limit, { target: { value: "3600" } });
  fireEvent.click(screen.getByRole("button", { name: "Save profile" }));
  await vi.waitFor(() => expect(mocks.save).toHaveBeenCalledOnce());
  expect(mocks.save.mock.calls[0][0]).toMatchObject({
    profileId: null,
    timeoutSeconds: 3600,
    expectedRevision: null,
  });
});

it.each(["0", "7201", "12.5", ""])(
  "refuses a time limit of %j before saving",
  async (value) => {
    render(<Tab />);
    fireEvent.change(screen.getByLabelText("Time limit (seconds)"), {
      target: { value },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save profile" }));
    await screen.findByText(/Enter a time limit from 1 to/);
    expect(mocks.save).not.toHaveBeenCalled();
  },
);

it("shows the time limit in the command review", () => {
  render(<Tab reviewing />);
  expect(
    screen.getByText("Pipeline stops this command after 900 seconds."),
  ).toBeInTheDocument();
});

it("edits a saved profile in place and keeps what the form does not show", async () => {
  render(<Tab />);
  expect(screen.getByText("Time limit: 900 seconds")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Edit" }));
  expect(screen.getByText("Edit Estimate")).toBeInTheDocument();
  const limit = screen.getByLabelText("Time limit (seconds)");
  expect(limit).toHaveValue(900);
  fireEvent.change(limit, { target: { value: "7200" } });
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
  await vi.waitFor(() => expect(mocks.save).toHaveBeenCalledOnce());
  expect(mocks.save.mock.calls[0][0]).toMatchObject({
    profileId: "profile",
    name: "Estimate",
    adapter: "stata",
    argv: profile.argv,
    cwd: "/project/code",
    environment: { LANG: "C" },
    inputs: ["main.do"],
    outputs: ["main.log"],
    timeoutSeconds: 7200,
    expectedRevision: 4,
  });
  await screen.findByText("New execution profile");
});

it("names every field of the profile form", () => {
  render(<Tab />);
  for (const label of ["Profile name", "Input paths", "Expected output paths"])
    expect(screen.getByLabelText(label)).toBeInTheDocument();
});

it("does not colour a queued run as a failure", () => {
  const run = (id: string, outcome: string) => ({
    id,
    outcome,
    adapter: "stata",
    dependencyHash: "abc",
    command: [],
    inputManifest: {},
    validation: {},
  });
  render(<Tab executions={[run("a", "queued"), run("b", "timed_out")]} />);
  expect(screen.getByText("Queued")).not.toHaveClass("text-danger");
  expect(screen.getByText("Timed out")).toHaveClass("text-danger");
  expect(screen.getAllByText(/Stata do-file/).length).toBeGreaterThan(0);
});
