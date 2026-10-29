import { beforeEach, describe, expect, it, vi } from "vitest";
import snapshotFixture from "../../../tests/fixtures/workbench/store-dto-v2.json";
import { workspaceClient } from "./workspaceClient";
import { conversationClient } from "./conversationClient";
import { harnessClient } from "./harnessClient";
import { researchClient } from "./researchClient";
import { executionClient } from "./executionClient";
import { recipeClient } from "./recipeClient";
import { releaseClient } from "./releaseClient";
import { runtimeClient } from "./runtimeClient";
import { isSessionSnapshot, type SessionSnapshot } from "../workbenchTypes";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

describe("Workbench DTO boundary", () => {
  beforeEach(() => invoke.mockReset());

  it("accepts the shared Rust/TypeScript snapshot fixture", () => {
    const snapshot = snapshotFixture satisfies SessionSnapshot;
    expect(isSessionSnapshot(snapshot)).toBe(true);
    expect(snapshot.session.workspaceId).toBe(snapshot.workspace?.id);
    expect(snapshot.session.overrides.reasoningEffort).toBe("high");
  });

  it("accepts unfiled snapshots and rejects cross-workspace snapshots", () => {
    expect(isSessionSnapshot(null)).toBe(false);
    expect(
      isSessionSnapshot({
        ...snapshotFixture,
        workspace: null,
        session: { ...snapshotFixture.session, workspaceId: null },
      }),
    ).toBe(true);
    expect(
      isSessionSnapshot({
        ...snapshotFixture,
        session: {
          ...snapshotFixture.session,
          workspaceId: "another-workspace",
        },
      }),
    ).toBe(false);
  });

  it("maps typed operations to the Tauri command contract", async () => {
    invoke.mockResolvedValue({ sessions: [], sequence: 4 });
    await conversationClient.listSessions("workspace_fixture", true);
    expect(invoke).toHaveBeenCalledWith("workbench_list_sessions", {
      workspaceId: "workspace_fixture",
      includeArchived: true,
    });

    const request = {
      sessionId: "session_fixture",
      expectedRevision: 2,
      operationId: "operation-3",
      draft: "Updated draft",
    };
    await conversationClient.updateSession(request);
    expect(invoke).toHaveBeenLastCalledWith("workbench_update_session", {
      request,
    });

    const move = {
      sessionId: "session_fixture",
      expectedRevision: 3,
      operationId: "operation-4",
      workspaceId: null,
    };
    await conversationClient.moveSession(move);
    expect(invoke).toHaveBeenLastCalledWith("workbench_move_session", {
      request: move,
    });
    const removal = {
      sessionId: "session_fixture",
      operationId: "operation-5",
    };
    await conversationClient.deleteSession(removal);
    expect(invoke).toHaveBeenLastCalledWith("workbench_delete_session", {
      request: removal,
    });
    await conversationClient.titlePreferences();
    expect(invoke).toHaveBeenLastCalledWith("workbench_get_title_preferences");
    const preferences = { enabled: false, model: "gpt-mini", effort: null };
    await conversationClient.saveTitlePreferences(preferences);
    expect(invoke).toHaveBeenLastCalledWith(
      "workbench_save_title_preferences",
      { preferences },
    );
    await conversationClient.generateSessionTitle("session_fixture");
    expect(invoke).toHaveBeenLastCalledWith(
      "workbench_generate_session_title",
      { sessionId: "session_fixture" },
    );

    await runtimeClient.accountState(true);
    expect(invoke).toHaveBeenLastCalledWith("workbench_codex_account_state", {
      refreshToken: true,
    });
    await runtimeClient.loginCancel("login-1");
    expect(invoke).toHaveBeenLastCalledWith("workbench_codex_login_cancel", {
      loginId: "login-1",
    });
    await runtimeClient.modelCatalog();
    expect(invoke).toHaveBeenLastCalledWith("workbench_codex_model_catalog");
    await runtimeClient.validateModelSelection("gpt-test", "high");
    expect(invoke).toHaveBeenLastCalledWith(
      "workbench_codex_validate_model_selection",
      { model: "gpt-test", effort: "high" },
    );
    const sendRequest = {
      sessionId: "session_fixture",
      text: "Estimate the comparative static.",
      clientSubmissionId: "submission-4",
      model: "gpt-test",
      effort: "high",
    };
    await runtimeClient.sendTurn(sendRequest);
    expect(invoke).toHaveBeenLastCalledWith("workbench_codex_send_turn", {
      request: sendRequest,
    });
    await runtimeClient.pendingRequests();
    expect(invoke).toHaveBeenLastCalledWith("workbench_codex_pending_requests");
    await runtimeClient.reconcileSession("session_fixture");
    expect(invoke).toHaveBeenLastCalledWith(
      "workbench_codex_reconcile_session",
      {
        sessionId: "session_fixture",
      },
    );
    await conversationClient.exportConversation(
      "session_fixture",
      "/tmp/chat.md",
    );
    expect(invoke).toHaveBeenLastCalledWith("workbench_export_conversation", {
      sessionId: "session_fixture",
      path: "/tmp/chat.md",
    });

    await harnessClient.harnessCatalog("workspace_fixture");
    expect(invoke).toHaveBeenLastCalledWith("workbench_harness_catalog", {
      workspaceId: "workspace_fixture",
    });
    await researchClient.paperSearch(
      "workspace_fixture",
      "revision_fixture",
      "Euler equation",
      12,
    );
    expect(invoke).toHaveBeenLastCalledWith("workbench_paper_search", {
      workspaceId: "workspace_fixture",
      revisionId: "revision_fixture",
      query: "Euler equation",
      limit: 12,
    });
    const runRequest = {
      profileId: "profile_fixture",
      sessionId: "session_fixture",
      testOnly: false,
      operationId: "run-fixture",
    };
    await executionClient.runExecution(runRequest);
    expect(invoke).toHaveBeenLastCalledWith("workbench_run_execution", {
      request: runRequest,
    });
    await executionClient.detectToolchains("workspace_fixture");
    expect(invoke).toHaveBeenLastCalledWith("workbench_detect_toolchains", {
      workspaceId: "workspace_fixture",
    });

    await recipeClient.startRecipe({
      sessionId: "session_fixture",
      recipeId: "theory_audit_recipe",
      operationId: "recipe-operation",
    });
    expect(invoke).toHaveBeenLastCalledWith("workbench_start_recipe", {
      request: {
        sessionId: "session_fixture",
        recipeId: "theory_audit_recipe",
        operationId: "recipe-operation",
      },
    });
    await releaseClient.prepareReviewHandoff({
      workspaceId: "workspace_fixture",
      sessionId: "session_fixture",
      paperId: "paper_fixture",
      metadata: { label: "revision" },
      operationId: "handoff-operation",
    });
    expect(invoke).toHaveBeenLastCalledWith(
      "workbench_prepare_review_handoff",
      {
        request: {
          workspaceId: "workspace_fixture",
          sessionId: "session_fixture",
          paperId: "paper_fixture",
          metadata: { label: "revision" },
          operationId: "handoff-operation",
        },
      },
    );
    await releaseClient.inspectResearchArchive("/tmp/research.pwrx");
    expect(invoke).toHaveBeenLastCalledWith(
      "workbench_inspect_research_archive",
      {
        request: { path: "/tmp/research.pwrx" },
      },
    );
    await releaseClient.importResearchArchive("/tmp/research.pwrx", {
      "/old/root": "/new/root",
    });
    expect(invoke).toHaveBeenLastCalledWith(
      "workbench_import_research_archive",
      {
        request: {
          path: "/tmp/research.pwrx",
          rootMappings: { "/old/root": "/new/root" },
        },
      },
    );
  });
  it("preserves default, null, and omitted values in boundary payloads", async () => {
    await conversationClient.listSessions(null);
    expect(invoke).toHaveBeenLastCalledWith("workbench_list_sessions", {
      workspaceId: null,
      includeArchived: false,
    });
    await workspaceClient.listWorkspaces();
    expect(invoke).toHaveBeenLastCalledWith("workbench_list_workspaces", {
      includeArchived: false,
    });
    await runtimeClient.accountState();
    expect(invoke).toHaveBeenLastCalledWith("workbench_codex_account_state", {
      refreshToken: false,
    });
    await releaseClient.importResearchArchive("archive.pwrx");
    expect(invoke).toHaveBeenLastCalledWith(
      "workbench_import_research_archive",
      { request: { path: "archive.pwrx", rootMappings: {} } },
    );
    await releaseClient.pruneStorage(["logs"], false);
    expect(invoke).toHaveBeenLastCalledWith("workbench_prune_storage", {
      request: {
        categories: ["logs"],
        apply: false,
        expectedPreviewToken: null,
      },
    });
    const request = {
      workspaceId: null,
      sourcePresetId: "preset",
      name: "Copy",
      operationId: "copy",
    };
    await harnessClient.clonePreset(request);
    expect(invoke).toHaveBeenLastCalledWith("workbench_clone_preset", {
      request,
    });
    expect(invoke.mock.calls.at(-1)?.[1].request).not.toHaveProperty(
      "sourceWorkspaceId",
    );
  });

  it("returns the original result promise and rejection without translation", async () => {
    const value = { marker: "unchanged" };
    const promise = Promise.resolve(value);
    invoke.mockReturnValueOnce(promise);
    expect(releaseClient.storageReport()).toBe(promise);
    const failure = { code: "native-error", detail: "unchanged" };
    invoke.mockRejectedValueOnce(failure);
    await expect(researchClient.listSources("workspace")).rejects.toBe(failure);
  });
});
