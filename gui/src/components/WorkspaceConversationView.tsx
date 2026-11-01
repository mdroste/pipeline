import { useAppPreferences, shouldSendMessage } from "../lib/appPreferences";
import { isMac } from "../lib/platform";
import {
  payloadText,
  roleFor,
  TRANSCRIPT_WINDOW,
} from "../lib/workspaceTranscript";
import {
  useCallback,
  useDeferredValue,
  useLayoutEffect,
  useMemo,
  useRef,
  useSyncExternalStore,
} from "react";
import { FileNavigationContext } from "./file-workspace/FileNavigation";
import { workspaceFileAdapter } from "../lib/fileWorkspaceClient";
import { appEvents } from "../lib/appEvents";
import { conversationClient } from "../lib/workbench/conversationClient";
import type { MutableRefObject, ReactNode, RefObject } from "react";
import TranscriptMessage, {
  TranscriptMarkdown,
} from "./WorkspaceTranscriptMessage";
import useTranscriptFollow from "../hooks/workspace/useTranscriptFollow";
import type { StreamStore } from "../hooks/workspace/streamStore";
import WorkspaceIcon from "./WorkspaceIcon";
import Tooltip from "../ui/Tooltip";
import { Icon } from "../ui/icons";
import type { FileDropState } from "../hooks/useComposerFileDrop";
import "./WorkspaceTranscript.css";
import "./WorkspaceComposer.css";
import "../ui/page.css";
import type {
  ConversationSnapshot,
  TranscriptItem,
} from "../lib/workbenchTypes";
interface Props {
  assistantName?: string;
  taskCards?: ReactNode;
  inspector?: ReactNode;
  inspectorOpen?: boolean;
  composerControls?: ReactNode;
  composerSetup?: ReactNode;
  /** A file drag is over the conversation, or dropped files are importing. */
  dropState?: FileDropState;
  header: ReactNode;
  requests: ReactNode;
  composerMenu: ReactNode;
  contextTray: ReactNode;
  snapshot: ConversationSnapshot | null;
  renderedItems: TranscriptItem[];
  totalItems: number;
  hasEarlier?: boolean;
  loadingEarlier?: boolean;
  pendingUser: string | null;
  streamStore: StreamStore;
  selectedMessage: string | null;
  transcriptStart: number;
  transcriptEnd: number;
  transcriptRef: RefObject<HTMLDivElement | null>;
  messageRef: RefObject<HTMLTextAreaElement | null>;
  error: string | null;
  /** A neutral update, such as a response the researcher stopped. */
  notice?: string | null;
  draft: string;
  disabled: boolean;
  active: boolean;
  submitting: boolean;
  contextBusy: boolean;
  onDraft: (text: string) => void;
  onSend: () => void;
  onStop: () => void;
  stopping?: boolean;
  /** Set only while no conversation is selected, when the composer cannot open. */
  onNewConversation?: () => void;
  onLatest: () => void;
  onEarlier: () => void;
  onNewer: () => void;
  /** Shared with the controller: whether the latest message stays in view. */
  followLatestRef: MutableRefObject<boolean>;
}
/** The answer as it arrives. Flushes re-render this message and nothing else. */
function LiveMessage({
  store,
  assistantName,
  active,
}: {
  store: StreamStore;
  assistantName: string;
  active: boolean;
}) {
  const stream = useSyncExternalStore(store.subscribe, store.get);
  // Streamed Markdown may lag a frame behind so typing stays responsive.
  const deferredStream = useDeferredValue(stream);
  if (!stream) return null;
  return (
    <article className="workspace-message workspace-message--assistant">
      <div className="workspace-message-author">
        <span className="workspace-assistant-mark">
          <WorkspaceIcon name="message" size={14} />
        </span>
        {assistantName}
        {/* A finished answer stays here until the transcript reloads. */}
        {active && (
          <span className="ml-1 h-1.5 w-1.5 rounded-full bg-current motion-safe:animate-pulse" />
        )}
      </div>
      <div className="workspace-message-body">
        <TranscriptMarkdown text={deferredStream} />
      </div>
    </article>
  );
}
export default function WorkspaceConversationView({
  assistantName = "ChatGPT",
  taskCards,
  inspector,
  inspectorOpen = Boolean(inspector),
  composerControls,
  composerSetup,
  dropState = "idle",
  header,
  requests,
  composerMenu,
  contextTray,
  snapshot,
  renderedItems,
  totalItems,
  hasEarlier = false,
  loadingEarlier = false,
  pendingUser,
  streamStore,
  selectedMessage,
  transcriptStart,
  transcriptEnd,
  transcriptRef,
  messageRef,
  error,
  notice = null,
  draft,
  disabled,
  active,
  submitting,
  contextBusy,
  onDraft,
  onSend,
  onStop,
  stopping = false,
  onNewConversation,
  onLatest,
  onEarlier,
  onNewer,
  followLatestRef,
}: Props) {
  const streaming = useSyncExternalStore(streamStore.subscribe, () =>
    Boolean(streamStore.get()),
  );
  const { following, follow, beforePaging, scrollHandlers } =
    useTranscriptFollow({
      transcriptRef,
      followLatestRef,
      sessionId: snapshot?.session.id,
      streaming,
      pendingUser,
      items: renderedItems,
      start: transcriptStart,
      end: transcriptEnd,
      total: totalItems,
      loadingEarlier,
    });
  // WebKit ends an input-method composition before the Enter keydown that
  // confirms it, so that keydown alone does not report isComposing.
  const composing = useRef(false);
  const fileNavigation = useMemo(() => {
    const sessionId = snapshot?.session.id;
    if (!sessionId) return null;
    const workspaceId = snapshot?.workspace?.id;
    const checkpointId = snapshot.session.overrides?.projectCheckpointId;
    const adapter = workspaceId
      ? workspaceFileAdapter({
          workspaceId,
          checkpointId: typeof checkpointId === "string" ? checkpointId : null,
        })
      : null;
    return {
      path: "conversation.md",
      root: snapshot.workspace?.root ?? undefined,
      open: (location: import("../lib/fileLinks").FileLocation) => {
        if (!workspaceId || !snapshot.workspace?.root || checkpointId)
          return conversationClient.openConversationFile(
            sessionId,
            location.path,
          );
        sessionStorage.setItem(
          `pipeline.openFile.${workspaceId}`,
          JSON.stringify(location),
        );
        appEvents.emit("open-file", { workspaceId, location });
      },
      openAbsolute: (path: string) =>
        conversationClient.openConversationFile(sessionId, path),
      image: async (path: string) => {
        const file = adapter
          ? await adapter.read(path)
          : await conversationClient.readConversationFile(sessionId, path);
        if (!file.base64 || !file.mime.startsWith("image/"))
          throw new Error("Image unavailable");
        return `data:${file.mime};base64,${file.base64}`;
      },
    };
  }, [
    snapshot?.session.id,
    snapshot?.workspace?.id,
    snapshot?.workspace?.root,
    snapshot?.session.overrides?.projectCheckpointId,
  ]);
  const { sendShortcut } = useAppPreferences();
  const shortcut =
    sendShortcut === "enter" ? "Enter" : `${isMac ? "⌘" : "Ctrl+"}Enter`;
  // The field grows with its content; CSS caps the height and scrolls past it.
  const fitComposer = useCallback(() => {
    const field = messageRef.current;
    if (!field) return;
    // Collapsing the field to measure it lets the transcript grow for a
    // moment, which can clamp its scroll position; put the position back so
    // the change does not read as the researcher scrolling up.
    const transcript = transcriptRef.current;
    const top = transcript?.scrollTop;
    field.style.height = "auto";
    // A hidden pane measures zero; leave the stylesheet's minimum in charge.
    field.style.height = field.scrollHeight ? `${field.scrollHeight}px` : "";
    if (transcript && top !== undefined) transcript.scrollTop = top;
  }, [messageRef, transcriptRef]);
  useLayoutEffect(fitComposer, [draft, fitComposer]);
  useLayoutEffect(() => {
    const field = messageRef.current;
    if (!field || typeof ResizeObserver === "undefined") return;
    let width = field.clientWidth;
    const observer = new ResizeObserver(() => {
      if (field.clientWidth === width) return;
      width = field.clientWidth;
      fitComposer();
    });
    observer.observe(field);
    return () => observer.disconnect();
  }, [fitComposer, messageRef]);
  return (
    <FileNavigationContext.Provider value={fileNavigation}>
      {header}
      {snapshot?.session.overrides?.automaticWorkingCopy === true &&
        snapshot.workspace && (
          <div className="border-b border-line px-4 py-2 text-ui-meta">
            Changes stay in a separate copy until you accept them.{" "}
            <button
              className="underline"
              onClick={() =>
                appEvents.emit("workspace-destination", {
                  workspaceId: snapshot.workspace!.id,
                  tab: "edits",
                })
              }
            >
              Review edits
            </button>
          </div>
        )}
      {inspector && (
        <div hidden={!inspectorOpen} className="workspace-assistant-inspector">
          {inspector}
        </div>
      )}
      <div
        hidden={inspectorOpen}
        ref={transcriptRef}
        {...scrollHandlers}
        className="workspace-transcript min-h-0 flex-1 overflow-auto"
      >
        <div className="workspace-transcript-content">
          {(!snapshot ||
            (!totalItems && !hasEarlier && !pendingUser && !streaming)) && (
            <div className="workspace-chat-empty">
              <WorkspaceIcon
                name="message"
                size={30}
                style={{ margin: "0 auto" }}
              />
              <h2>
                {snapshot
                  ? "What are you working on?"
                  : "A place to think things through"}
              </h2>
              <p>
                {snapshot
                  ? "Ask a question, explore an idea, or add files to work from."
                  : "Start a conversation to explore an idea or work through your research."}
              </p>
              {onNewConversation && (
                <button
                  type="button"
                  className="ui-page-button primary"
                  onClick={onNewConversation}
                >
                  New conversation
                </button>
              )}
            </div>
          )}
          {(transcriptStart > 0 || hasEarlier) && (
            <button
              type="button"
              onClick={() => {
                beforePaging("earlier");
                onEarlier();
              }}
              disabled={loadingEarlier}
              className="mx-auto block rounded border px-3 py-2 text-xs text-ink-muted"
            >
              {loadingEarlier
                ? "Loading earlier messages…"
                : "Show earlier messages"}
            </button>
          )}
          {renderedItems.map((item) => {
            const text = payloadText(item);
            return text ? (
              <TranscriptMessage
                key={item.id}
                id={item.id}
                text={text}
                role={roleFor(item)}
                selected={selectedMessage === item.id}
              />
            ) : null;
          })}
          {pendingUser && (
            <article className="workspace-message workspace-message--user">
              <div className="workspace-message-author">You</div>
              <div className="workspace-message-body whitespace-pre-wrap">
                {pendingUser}
              </div>
            </article>
          )}
          <LiveMessage
            store={streamStore}
            assistantName={assistantName}
            active={active}
          />
          {taskCards}
          {transcriptEnd < totalItems && (
            <button
              type="button"
              onClick={() => {
                beforePaging("newer");
                onNewer();
              }}
              className="mx-auto block rounded border px-3 py-2 text-xs text-ink-muted"
            >
              Show {TRANSCRIPT_WINDOW} newer messages
            </button>
          )}
        </div>
        {(!following || transcriptEnd < totalItems) && (
          // Out of flow at the end, so showing it never moves the text.
          <div className="workspace-latest-control">
            <button
              type="button"
              onClick={() => {
                follow(true);
                onLatest();
              }}
              className="rounded-lg px-2 py-1 text-xs text-gray-500 hover:bg-gray-100 dark:hover:bg-neutral-800"
            >
              ↓ Latest
            </button>
          </div>
        )}
      </div>
      <div className="workspace-conversation-notices">
        {requests}
        {notice && (
          <div
            role="status"
            className="rounded-lg border border-gray-200 bg-gray-50 p-3 text-sm text-gray-700 dark:border-neutral-800 dark:bg-neutral-900 dark:text-gray-300"
          >
            {notice}
          </div>
        )}
        {error && (
          <div
            role="alert"
            className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300"
          >
            {error}
          </div>
        )}
      </div>
      <div className="workspace-composer-dock" aria-busy={contextBusy}>
        <div className="mx-auto max-w-3xl">
          <div
            className="workspace-composer"
            data-drop={dropState === "idle" ? undefined : dropState}
          >
            {dropState !== "idle" && (
              <div className="workspace-composer-drop" role="status">
                <Icon name="import" />
                {dropState === "over"
                  ? "Drop to add these files to the conversation"
                  : "Adding files…"}
              </div>
            )}
            {contextTray}
            <textarea
              ref={messageRef}
              aria-label="Message"
              value={draft}
              // Read-only during a response keeps keyboard focus here, where a
              // disabled field would drop it.
              disabled={Boolean(onNewConversation)}
              readOnly={disabled}
              onChange={(event) => onDraft(event.target.value)}
              onCompositionStart={() => {
                composing.current = true;
              }}
              onCompositionEnd={() => {
                window.setTimeout(() => {
                  composing.current = false;
                });
              }}
              onKeyDown={(event) => {
                if (
                  !disabled &&
                  !active &&
                  !submitting &&
                  draft.trim() &&
                  shouldSendMessage(
                    {
                      key: event.key,
                      shiftKey: event.shiftKey,
                      metaKey: event.metaKey,
                      ctrlKey: event.ctrlKey,
                      altKey: event.altKey,
                      isComposing:
                        event.nativeEvent.isComposing ||
                        event.keyCode === 229 ||
                        composing.current,
                    },
                    sendShortcut,
                  )
                ) {
                  event.preventDefault();
                  onSend();
                }
              }}
              placeholder={`Ask ${assistantName}… ${shortcut} to send`}
              rows={1}
              className="workspace-composer-input"
            />
            <div className="workspace-composer-actions">
              <div className="workspace-composer-leading">
                {composerMenu}
                {composerControls}
                {composerSetup}
              </div>
              {/* One control in one place: Send when idle, Stop while working. */}
              <Tooltip
                label={
                  active
                    ? "Stop the response"
                    : `Send · ${shortcut}. Shift+Enter adds a line.`
                }
              >
                <button
                  type="button"
                  disabled={active ? stopping : disabled || !draft.trim()}
                  onClick={() => (active ? onStop() : onSend())}
                  className="workspace-send-button"
                  aria-label={
                    active
                      ? stopping
                        ? "Stopping…"
                        : "Stop response"
                      : submitting
                        ? "Sending…"
                        : "Send"
                  }
                >
                  <Icon
                    name={active ? "stop" : "arrow-up"}
                    className="h-[18px] w-[18px]"
                  />
                </button>
              </Tooltip>
            </div>
          </div>
        </div>
      </div>
    </FileNavigationContext.Provider>
  );
}
