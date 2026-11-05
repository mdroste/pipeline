import { invoke } from "@tauri-apps/api/core";
import type {
  ResearchRecipe,
  RecipeInputCheck,
  RecipeRun,
} from "../workbenchTypes";

export const recipeClient = {
  listRecipes(workspaceId: string | null) {
    return invoke<ResearchRecipe[]>("workbench_list_recipes", { workspaceId });
  },
  cloneRecipe(request: {
    workspaceId: string;
    sourceRecipeId: string;
    name: string;
  }) {
    return invoke<ResearchRecipe>("workbench_clone_recipe", { request });
  },
  updateRecipe(request: {
    recipeId: string;
    expectedRevision: number;
    name: string;
    description: string;
    instructions: string;
    requiredInputs: string[];
    requiredTools: string[];
    suggestedPermissionMode: "inspect" | "edit";
    expectedChecks: string[];
  }) {
    return invoke<ResearchRecipe>("workbench_update_recipe", { request });
  },
  checkRecipeInputs(sessionId: string, recipeId: string) {
    return invoke<RecipeInputCheck[]>("workbench_check_recipe_inputs", {
      sessionId,
      recipeId,
    });
  },
  startRecipe(request: {
    sessionId: string;
    recipeId: string;
    operationId: string;
  }) {
    return invoke<RecipeRun>("workbench_start_recipe", { request });
  },
  completeRecipe(request: {
    recipeRunId: string;
    artifacts: Array<Record<string, unknown>>;
    checks: Array<Record<string, unknown>>;
    unresolvedIssues: string[];
    missingEvidence: string[];
  }) {
    return invoke<RecipeRun>("workbench_complete_recipe", { request });
  },
  listRecipeRuns(sessionId: string) {
    return invoke<RecipeRun[]>("workbench_list_recipe_runs", { sessionId });
  },
};
