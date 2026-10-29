import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import WorkspaceRecipesPanel from "./WorkspaceRecipesPanel";
import type {
  ConversationSnapshot,
  RecipeRun,
  ResearchRecipe,
} from "../lib/workbenchTypes";

const mocks = vi.hoisted(() => ({
  listRecipes: vi.fn(),
  listRecipeRuns: vi.fn(),
  listExecutions: vi.fn(),
  checkRecipeInputs: vi.fn(),
  cloneRecipe: vi.fn(),
  updateRecipe: vi.fn(),
  completeRecipe: vi.fn(),
  promptDialog: vi.fn(),
}));
vi.mock("../lib/workbench/recipeClient", () => ({ recipeClient: mocks }));
vi.mock("../lib/workbench/executionClient", () => ({ executionClient: mocks }));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: {},
}));
vi.mock("./DialogService", () => ({ promptDialog: mocks.promptDialog }));

const recipe = (overrides: Partial<ResearchRecipe>): ResearchRecipe => ({
  id: "recipe",
  workspaceId: "ws",
  sourceRecipeId: null,
  name: "Recipe",
  description: "",
  instructions: "",
  requiredInputs: [],
  requiredTools: [],
  suggestedPermissionMode: "inspect",
  expectedChecks: [],
  version: 1,
  revision: 1,
  builtIn: false,
  ...overrides,
});
const editRecipe = recipe({
  id: "a",
  name: "Edit recipe",
  suggestedPermissionMode: "edit",
});
const inspectRecipe = recipe({ id: "b", name: "Inspect recipe" });
const snapshot = {
  session: { id: "s1", workspaceId: "ws", overrides: {} },
} as unknown as ConversationSnapshot;
const panel = () => (
  <WorkspaceRecipesPanel
    snapshot={snapshot}
    onSnapshot={vi.fn()}
    onError={vi.fn()}
  />
);

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listRecipes.mockResolvedValue([editRecipe, inspectRecipe]);
  mocks.listRecipeRuns.mockResolvedValue([]);
  mocks.listExecutions.mockResolvedValue([]);
  mocks.checkRecipeInputs.mockResolvedValue([]);
});

it("names a recipe copy in an in-app prompt and aborts when cancelled", async () => {
  render(panel());
  const copy = await screen.findByRole("button", { name: "Copy and edit" });
  mocks.promptDialog.mockResolvedValueOnce(null);
  fireEvent.click(copy);
  await waitFor(() =>
    expect(mocks.promptDialog).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ defaultValue: "Edit recipe copy" }),
    ),
  );
  expect(mocks.cloneRecipe).not.toHaveBeenCalled();

  mocks.promptDialog.mockResolvedValueOnce("  My recipe  ");
  mocks.cloneRecipe.mockResolvedValue(recipe({ id: "c", name: "My recipe" }));
  await waitFor(() => expect(copy).toBeEnabled());
  fireEvent.click(copy);
  await waitFor(() =>
    expect(mocks.cloneRecipe).toHaveBeenCalledWith({
      workspaceId: "ws",
      sourceRecipeId: "a",
      name: "My recipe",
    }),
  );
});

it("resets the custom recipe form when another recipe is selected", async () => {
  mocks.updateRecipe.mockResolvedValue({});
  const { container } = render(panel());
  await screen.findByRole("option", { name: /Inspect recipe/ });
  fireEvent.click(screen.getByText("Edit custom recipe"));
  const mode = () =>
    container.querySelector<HTMLSelectElement>('select[name="mode"]')!;
  expect(mode()).toHaveValue("edit");
  fireEvent.change(screen.getByRole("combobox", { name: "Recipe" }), {
    target: { value: "b" },
  });
  expect(mode()).toHaveValue("inspect");
  fireEvent.click(screen.getByRole("button", { name: "Save recipe version" }));
  await waitFor(() =>
    expect(mocks.updateRecipe).toHaveBeenCalledWith(
      expect.objectContaining({
        recipeId: "b",
        name: "Inspect recipe",
        suggestedPermissionMode: "inspect",
      }),
    ),
  );
});

it("records the unresolved issues typed next to the checks", async () => {
  const active: RecipeRun = {
    id: "run",
    workspaceId: "ws",
    sessionId: "s1",
    recipe: recipe({ id: "a", expectedChecks: ["robustness"] }),
    status: "active",
    artifacts: [],
    checks: [],
    unresolvedIssues: [],
    missingEvidence: [],
    startedAt: "t",
    completedAt: null,
  };
  mocks.listRecipeRuns.mockResolvedValue([active]);
  mocks.completeRecipe.mockResolvedValue({});
  render(panel());
  fireEvent.change(
    await screen.findByRole("textbox", {
      name: "Unresolved issues, one per line",
    }),
    { target: { value: "Clustered errors\n\n  Missing appendix  " } },
  );
  fireEvent.click(screen.getByRole("button", { name: "Record completion" }));
  await waitFor(() =>
    expect(mocks.completeRecipe).toHaveBeenCalledWith(
      expect.objectContaining({
        recipeRunId: "run",
        unresolvedIssues: ["Clustered errors", "Missing appendix"],
      }),
    ),
  );
});
