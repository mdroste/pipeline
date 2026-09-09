import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import WorkspaceConnectionSettings from "./WorkspaceConnectionSettings";

const mocks = vi.hoisted(() => ({
  connectCodex: vi.fn(),
  accountState: vi.fn(),
  modelCatalog: vi.fn(),
  rateLimits: vi.fn(),
  titlePreferences: vi.fn(),
  saveTitlePreferences: vi.fn(),
  capabilities: vi.fn(),
}));
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: mocks }));
vi.mock("../lib/projectClient", () => ({
  projectClient: { capabilities: mocks.capabilities },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

const model = (id: string, isDefault: boolean, efforts: string[]) => ({
  id,
  model: id,
  displayName: id,
  description: "",
  isDefault,
  defaultReasoningEffort: efforts[0],
  supportedReasoningEfforts: efforts.map((reasoningEffort) => ({
    reasoningEffort,
    description: "",
  })),
});

beforeEach(() => {
  vi.clearAllMocks();
  mocks.connectCodex.mockResolvedValue({});
  mocks.accountState.mockResolvedValue({
    status: "chatgpt",
    email: "r@example.test",
    planType: "plus",
  });
  mocks.modelCatalog.mockResolvedValue({
    models: [
      model("gpt-large", true, ["medium", "high"]),
      model("gpt-mini", false, ["low", "medium"]),
    ],
  });
  mocks.rateLimits.mockResolvedValue({ source: "perBucket", buckets: [] });
  mocks.capabilities.mockResolvedValue(null);
  mocks.titlePreferences.mockResolvedValue({
    enabled: true,
    model: null,
    effort: null,
  });
  mocks.saveTitlePreferences.mockImplementation(
    async (preferences) => preferences,
  );
});

it("edits automatic title preferences and saves each change", async () => {
  render(<WorkspaceConnectionSettings embedded />);
  const toggle = await screen.findByLabelText(
    "Name new conversations automatically",
  );
  expect(toggle).toBeChecked();
  const modelSelect = await screen.findByLabelText("Title model");
  await waitFor(() =>
    expect(
      screen.getByRole("option", { name: "gpt-mini" }),
    ).toBeInTheDocument(),
  );
  expect(screen.getByRole("option", { name: "low" })).toBeInTheDocument();
  fireEvent.change(modelSelect, { target: { value: "gpt-large" } });
  await waitFor(() =>
    expect(mocks.saveTitlePreferences).toHaveBeenLastCalledWith({
      enabled: true,
      model: "gpt-large",
      effort: null,
    }),
  );
  expect(screen.queryByRole("option", { name: "low" })).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Title reasoning effort"), {
    target: { value: "high" },
  });
  await waitFor(() =>
    expect(mocks.saveTitlePreferences).toHaveBeenLastCalledWith({
      enabled: true,
      model: "gpt-large",
      effort: "high",
    }),
  );
  fireEvent.click(toggle);
  await waitFor(() =>
    expect(mocks.saveTitlePreferences).toHaveBeenLastCalledWith({
      enabled: false,
      model: "gpt-large",
      effort: "high",
    }),
  );
  expect(screen.getByLabelText("Title model")).toBeDisabled();
});

it("keeps title settings editable while signed out", async () => {
  mocks.accountState.mockResolvedValue({ status: "signedOut" });
  mocks.titlePreferences.mockResolvedValue({
    enabled: true,
    model: "gpt-mini",
    effort: "low",
  });
  render(<WorkspaceConnectionSettings embedded />);
  expect(await screen.findByLabelText("Title model")).toHaveValue("gpt-mini");
  expect(
    screen.getByRole("option", { name: "gpt-mini (not currently available)" }),
  ).toBeInTheDocument();
  expect(screen.getByLabelText("Title reasoning effort")).toHaveValue("low");
  expect(
    screen.getByText("Model choices appear after signing in."),
  ).toBeInTheDocument();
});
