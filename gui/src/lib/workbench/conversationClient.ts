import { invoke } from "@tauri-apps/api/core";
import type { FilePreview } from "../fileWorkspaceClient";
import type {
  CreateSessionRequest,
  DeleteSessionRequest,
  MoveSessionRequest,
  Mutation,
  SessionList,
  SessionSnapshot,
  TitlePreferences,
  UpdateSessionRequest,
  WorkbenchSession,
  ConversationSnapshot,
} from "../workbenchTypes";

export const conversationClient = {
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
  listSessions(workspaceId: string | null, includeArchived = false) {
    return invoke<SessionList>("workbench_list_sessions", {
      workspaceId,
      includeArchived,
    });
  },
  sessionSnapshot(sessionId: string) {
    return invoke<SessionSnapshot>("workbench_session_snapshot", { sessionId });
  },
  transcriptPage(
    sessionId: string,
    before: import("../workbenchTypes").TranscriptCursor,
  ) {
    return invoke<import("../workbenchTypes").TranscriptPage>(
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
  readConversationFile(sessionId: string, path: string) {
    return invoke<FilePreview>("workbench_conversation_file_read", {
      sessionId,
      path,
    });
  },
  exportConversation(sessionId: string, path: string) {
    return invoke<void>("workbench_export_conversation", { sessionId, path });
  },
};
