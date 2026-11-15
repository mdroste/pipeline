import { invoke } from "@tauri-apps/api/core";
import type { FileListing, FilePreview } from "../fileWorkspaceClient";
import type {
  CreateSessionRequest,
  DeleteSessionRequest,
  MoveSessionRequest,
  Mutation,
  SessionCursor,
  SessionList,
  SessionSnapshot,
  TitlePreferences,
  UpdateSessionRequest,
  WorkbenchSession,
  ConversationSnapshot,
} from "./sessionTypes";

export interface ArtifactRunTarget {
  sessionId: string;
  scopeRoot: string;
  path: string;
}

export interface ArtifactRun {
  id: string;
  state:
    | "running"
    | "stopping"
    | "completed"
    | "failed"
    | "interrupted"
    | "timed_out"
    | "outcome_unknown";
  exitStatus: number | null;
  stdout: string;
  stderr: string;
  truncated: boolean;
}

export interface ConversationIndexRow {
  session: Pick<
    WorkbenchSession,
    "id" | "workspaceId" | "title" | "archivedAt" | "updatedAt"
  >;
  projectName: string | null;
}
export interface IndexCursor extends SessionCursor {
  projectUpdatedAt: string;
  workspaceId: string;
}
export const conversationClient = {
  index(
    archived: boolean,
    unfiled: boolean,
    query: string,
    palette: boolean,
    cursor: IndexCursor | null,
  ) {
    return invoke<{
      items: ConversationIndexRow[];
      nextCursor: IndexCursor | null;
    }>("workbench_conversation_index", {
      archived,
      unfiled,
      query,
      palette,
      cursor,
    });
  },
  createSession(request: CreateSessionRequest) {
    return invoke<Mutation<WorkbenchSession>>("workbench_create_session", {
      request,
    });
  },
  updateSession(request: UpdateSessionRequest) {
    return invoke<Mutation<WorkbenchSession>>("workbench_update_session", {
      request,
    });
  },
  moveSession(request: MoveSessionRequest) {
    return invoke<Mutation<WorkbenchSession>>("workbench_move_session", {
      request,
    });
  },
  deleteSession(request: DeleteSessionRequest) {
    return invoke<number>("workbench_delete_session", { request });
  },
  titlePreferences() {
    return invoke<TitlePreferences>("workbench_get_title_preferences");
  },
  saveTitlePreferences(preferences: TitlePreferences) {
    return invoke<TitlePreferences>("workbench_save_title_preferences", {
      preferences,
    });
  },
  generateSessionTitle(sessionId: string) {
    return invoke<string>("workbench_generate_session_title", { sessionId });
  },
  listSessions(
    workspaceId: string | null,
    includeArchived = false,
    cursor?: SessionCursor | null,
  ) {
    return invoke<SessionList>("workbench_list_sessions", {
      workspaceId,
      includeArchived,
      ...(cursor ? { cursor } : {}),
    });
  },
  sessionSnapshot(sessionId: string) {
    return invoke<SessionSnapshot>("workbench_session_snapshot", { sessionId });
  },
  transcriptPage(
    sessionId: string,
    before: import("./sessionTypes").TranscriptCursor,
  ) {
    return invoke<import("./sessionTypes").TranscriptPage>(
      "workbench_transcript_page",
      { sessionId, before },
    );
  },
  conversationSnapshot(sessionId: string) {
    return invoke<ConversationSnapshot>("workbench_conversation_snapshot", {
      sessionId,
    });
  },
  openConversationFile(sessionId: string, path: string) {
    return invoke<void>("workbench_conversation_file_open", {
      sessionId,
      path,
    });
  },
  openConversationFileWith(sessionId: string, path: string) {
    return invoke<void>("workbench_conversation_file_open_with", {
      sessionId,
      path,
    });
  },
  revealConversationFolder(sessionId: string) {
    return invoke<void>("workbench_conversation_folder_reveal", { sessionId });
  },
  readConversationFile(sessionId: string, path: string) {
    return invoke<FilePreview>("workbench_conversation_file_read", {
      sessionId,
      path,
    });
  },
  previewConversationFile(sessionId: string, path: string) {
    return invoke<FilePreview>("workbench_conversation_file_preview", {
      sessionId,
      path,
    });
  },
  saveConversationFile(request: {
    sessionId: string;
    path: string;
    scopeRoot: string;
    expectedHash: string;
    content: string;
  }) {
    return invoke<string>("workbench_conversation_file_save", { request });
  },
  runArtifact(target: ArtifactRunTarget, expectedHash: string) {
    return invoke<ArtifactRun>("workbench_conversation_file_run", {
      request: { target, expectedHash },
    });
  },
  artifactRunStatus(target: ArtifactRunTarget) {
    return invoke<ArtifactRun | null>(
      "workbench_conversation_file_run_status",
      { target },
    );
  },
  stopArtifactRun(target: ArtifactRunTarget, runId: string) {
    return invoke<void>("workbench_conversation_file_run_stop", {
      target,
      runId,
    });
  },
  listConversationFiles(sessionId: string, directory = "") {
    return invoke<{ scopeRoot: string; listing: FileListing | null }>(
      "workbench_conversation_file_list",
      { sessionId, directory },
    );
  },
  exportConversation(sessionId: string, path: string) {
    return invoke<void>("workbench_export_conversation", { sessionId, path });
  },
};
