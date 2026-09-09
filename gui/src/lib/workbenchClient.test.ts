import { beforeEach, describe, expect, it, vi } from "vitest";
import snapshotFixture from "../../tests/fixtures/workbench/store-dto-v2.json";
import { workbenchClient } from "./workbenchClient";
import { isSessionSnapshot, type SessionSnapshot } from "./workbenchTypes";

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
    await workbenchClient.listSessions("workspace_fixture", true);
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
    await workbenchClient.updateSession(request);
    expect(invoke).toHaveBeenLastCalledWith("workbench_update_session", {
      request,
    });

    const move = {
      sessionId: "session_fixture",
      expectedRevision: 3,
      operationId: "operation-4",
      workspaceId: null,
    };
    await workbenchClient.moveSession(move);
    expect(invoke).toHaveBeenLastCalledWith("workbench_move_session", {
      request: move,
    });
    const removal = {
      sessionId: "session_fixture",
      operationId: "operation-5",
    };
    await workbenchClient.deleteSession(removal);
    expect(invoke).toHaveBeenLastCalledWith("workbench_delete_session", {
      request: removal,
    });
    await workbenchClient.titlePreferences();
    expect(invoke).toHaveBeenLastCalledWith("workbench_get_title_preferences");
    const preferences = { enabled: false, model: "gpt-mini", effort: null };
    await workbenchClient.saveTitlePreferences(preferences);
    expect(invoke).toHaveBeenLastCalledWith(
      "workbench_save_title_preferences",
      { preferences },
    );
    await workbenchClient.generateSessionTitle("session_fixture");
    expect(invoke).toHaveBeenLastCalledWith(
      "workbench_generate_session_title",
      { sessionId: "session_fixture" },
    );

    await workbenchClient.accountState(true);
    expect(invoke).toHaveBeenLastCalledWith("workbench_codex_account_state", {
      refreshToken: true,
    });
    await workbenchClient.loginCancel("login-1");
    expect(invoke).toHaveBeenLastCalledWith("workbench_codex_login_cancel", {
      loginId: "login-1",
    });
    await workbenchClient.modelCatalog();
    expect(invoke).toHaveBeenLastCalledWith("workbench_codex_model_catalog");
    await workbenchClient.validateModelSelection("gpt-test", "high");
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
    await workbenchClient.sendTurn(sendRequest);
    expect(invoke).toHaveBeenLastCalledWith("workbench_codex_send_turn", {
      request: sendRequest,
    });
    await workbenchClient.pendingRequests();
    expect(invoke).toHaveBeenLastCalledWith("workbench_codex_pending_requests");
    await workbenchClient.reconcileSession("session_fixture");
    expect(invoke).toHaveBeenLastCalledWith(
      "workbench_codex_reconcile_session",
      {
        sessionId: "session_fixture",
      },
    );
    await workbenchClient.exportConversation("session_fixture", "/tmp/chat.md");
    expect(invoke).toHaveBeenLastCalledWith("workbench_export_conversation", {
      sessionId: "session_fixture",
      path: "/tmp/chat.md",
    });

    await workbenchClient.harnessCatalog("workspace_fixture");
    expect(invoke).toHaveBeenLastCalledWith("workbench_harness_catalog", {
      workspaceId: "workspace_fixture",
    });
    await workbenchClient.paperSearch(
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
    await workbenchClient.runExecution(runRequest);
    expect(invoke).toHaveBeenLastCalledWith("workbench_run_execution", {
      request: runRequest,
    });

    await workbenchClient.startRecipe({
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
    await workbenchClient.prepareReviewHandoff({
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
    await workbenchClient.inspectResearchArchive("/tmp/research.pwrx");
    expect(invoke).toHaveBeenLastCalledWith(
      "workbench_inspect_research_archive",
      {
        request: { path: "/tmp/research.pwrx" },
      },
    );
    await workbenchClient.importResearchArchive("/tmp/research.pwrx", {
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
});
