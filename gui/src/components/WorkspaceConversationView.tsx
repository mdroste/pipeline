import { useAppPreferences, shouldSendMessage } from "../lib/appPreferences";
import { isMac } from "../lib/platform";
import { useEffect, useLayoutEffect, useMemo, useState } from "react";
import { FileNavigationContext } from "./file-workspace/FileNavigation";
import { workspaceFileAdapter } from "../lib/fileWorkspaceClient";
import { appEvents } from "../lib/appEvents";
import { workbenchClient } from "../lib/workbenchClient";
import type { ReactNode, RefObject } from "react";
import ReactMarkdown from "react-markdown";
import remarkMath from "remark-math";
import remarkGfm from "remark-gfm";
import { fileMarkdownComponents } from "./file-workspace/markdownComponents";
import rehypeKatex from "rehype-katex";
import WorkspaceMessageActions from "./WorkspaceMessageActions";
import WorkspaceIcon from "./WorkspaceIcon";
import Tooltip from "../ui/Tooltip";
import { Icon } from "../ui/icons";
import type { FileDropState } from "../hooks/useComposerFileDrop";
import "./WorkspaceTranscript.css";
import "./WorkspaceComposer.css";
import type {
  ConversationSnapshot,
  TranscriptItem,
} from "../lib/workbenchTypes";
export function payloadText(item: TranscriptItem): string {
  const payload = item.payload;
  if (!payload) return "";
  for (const key of ["text", "message", "content"]) {
    const value = payload[key];
    if (typeof value === "string") return value;
    if (Array.isArray(value)) {
      const joined = value
        .map((part) => {
          if (typeof part === "string") return part;
          if (
            part &&
            typeof part === "object" &&
            "text" in part &&
            typeof part.text === "string"
          )
            return part.text;
          return "";
        })
        .filter(Boolean)
        .join("\n");
      if (joined) return joined;
    }
  }
  return "";
}

export function isMessage(item: TranscriptItem) {
  return item.itemKind.toLowerCase().includes("message");
}

export function roleFor(item: TranscriptItem) {
  return item.itemKind.toLowerCase().includes("user") ? "You" : "ChatGPT";
}

interface Props {
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
  stream: string;
  selectedMessage: string | null;
  transcriptStart: number;
  transcriptEnd: number;
  transcriptRef: RefObject<HTMLDivElement | null>;
  messageRef: RefObject<HTMLTextAreaElement | null>;
  error: string | null;
  draft: string;
  disabled: boolean;
  active: boolean;
  submitting: boolean;
  contextBusy: boolean;
  onDraft: (text: string) => void;
  onSend: () => void;
  onStop: () => void;
  onLatest: () => void;
  onEarlier: () => void;
  onNewer: () => void;
  onFollow: (follow: boolean) => void;
}
export default function WorkspaceConversationView({
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
  stream,
  selectedMessage,
  transcriptStart,
  transcriptEnd,
  transcriptRef,
  messageRef,
  error,
  draft,
  disabled,
  active,
  submitting,
  contextBusy,
  onDraft,
  onSend,
  onStop,
  onLatest,
  onEarlier,
  onNewer,
  onFollow,
}: Props) {
  const [following, setFollowing] = useState(true);
  useEffect(() => setFollowing(true), [snapshot?.session.id]);
  const fileNavigation = useMemo(() => {
    const sessionId = snapshot?.session.id;
    if (!sessionId) return null;
    const workspaceId = snapshot?.workspace?.id;
    const adapter = workspaceId ? workspaceFileAdapter({ workspaceId }) : null;
    return {
      path: "conversation.md",
      root: snapshot.workspace?.root ?? undefined,
      open: (location: import("../lib/fileLinks").FileLocation) => {
        if (!workspaceId || !snapshot.workspace?.root)
          return workbenchClient.openConversationFile(sessionId, location.path);
        sessionStorage.setItem(
          `pipeline.openFile.${workspaceId}`,
          JSON.stringify(location),
        );
        appEvents.emit("open-file", { workspaceId, location });
      },
      openAbsolute: (path: string) =>
        workbenchClient.openConversationFile(sessionId, path),
      image: async (path: string) => {
        const file = adapter
          ? await adapter.read(path)
          : await workbenchClient.readConversationFile(sessionId, path);
        if (!file.base64 || !file.mime.startsWith("image/"))
          throw new Error("Image unavailable");
        return `data:${file.mime};base64,${file.base64}`;
      },
    };
  }, [
    snapshot?.session.id,
    snapshot?.workspace?.id,
    snapshot?.workspace?.root,
  ]);
  const { sendShortcut } = useAppPreferences();
  const shortcut =
    sendShortcut === "enter" ? "Enter" : `${isMac ? "⌘" : "Ctrl+"}Enter`;
  // The field grows with its content; CSS caps the height and scrolls past it.
  useLayoutEffect(() => {
    const field = messageRef.current;
    if (!field) return;
    const fit = () => {
      field.style.height = "auto";
      // A hidden pane measures zero; leave the stylesheet's minimum in charge.
      field.style.height = field.scrollHeight ? `${field.scrollHeight}px` : "";
    };
    fit();
    if (typeof ResizeObserver === "undefined") return;
    let width = field.clientWidth;
    const observer = new ResizeObserver(() => {
      if (field.clientWidth === width) return;
      width = field.clientWidth;
      fit();
    });
    observer.observe(field);
    return () => observer.disconnect();
  }, [draft, messageRef]);
  return (
    <FileNavigationContext.Provider value={fileNavigation}>
      {header}
      {inspector && (
        <div hidden={!inspectorOpen} className="workspace-assistant-inspector">
          {inspector}
        </div>
      )}
      <div
        hidden={inspectorOpen}
        ref={transcriptRef}
        onScroll={(event) => {
          const node = event.currentTarget;
          const follow =
            node.scrollHeight - node.scrollTop - node.clientHeight < 80;
          setFollowing(follow);
          onFollow(follow);
        }}
        className="workspace-transcript min-h-0 flex-1 overflow-auto"
      >
        {(!following || transcriptEnd < totalItems) && (
          <div className="workspace-latest-control">
            <button
              type="button"
              onClick={() => {
                setFollowing(true);
                onLatest();
              }}
              className="rounded-lg px-2 py-1 text-xs text-gray-500 hover:bg-gray-100 dark:hover:bg-neutral-800"
            >
              ↓ Latest
            </button>
          </div>
        )}
        <div className="workspace-transcript-content">
          {(!snapshot || (!totalItems && !pendingUser && !stream)) && (
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
            </div>
          )}
          {(transcriptStart > 0 || hasEarlier) && (
            <button
              type="button"
              onClick={() => {
                onFollow(false);
                onEarlier();
              }}
              disabled={loadingEarlier}
              className="mx-auto block rounded border px-3 py-2 text-xs text-gray-600"
            >
              {loadingEarlier
                ? "Loading earlier messages…"
                : "Show earlier messages"}
            </button>
          )}
          {renderedItems.map((item) => {
            const text = payloadText(item);
            const user = roleFor(item) === "You";
            return text ? (
              <article
                key={item.id}
                id={`workspace-message-${item.id}`}
                tabIndex={-1}
                aria-label={`${roleFor(item)} message`}
                className={`workspace-message workspace-message--${user ? "user" : "assistant"}${selectedMessage === item.id ? " ring-2 ring-blue-300 ring-offset-4 dark:ring-offset-neutral-950" : ""}`}
              >
                <div className="workspace-message-author">
                  {!user && (
                    <span className="workspace-assistant-mark">
                      <WorkspaceIcon name="message" size={14} />
                    </span>
                  )}
                  {roleFor(item)}
                </div>
                <div className="workspace-message-body">
                  <ReactMarkdown
                    remarkPlugins={[remarkGfm, remarkMath]}
                    rehypePlugins={[rehypeKatex]}
                    components={fileMarkdownComponents}
                  >
                    {text}
                  </ReactMarkdown>
                </div>
                <WorkspaceMessageActions
                  text={text}
                  label={user ? "prompt" : "response"}
                />
              </article>
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
          {stream && (
            <article className="workspace-message workspace-message--assistant">
              <div className="workspace-message-author">
                <span className="workspace-assistant-mark">
                  <WorkspaceIcon name="message" size={14} />
                </span>
                ChatGPT
                <span className="ml-1 h-1.5 w-1.5 rounded-full bg-current motion-safe:animate-pulse" />
              </div>
              <div className="workspace-message-body">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm, remarkMath]}
                  rehypePlugins={[rehypeKatex]}
                  components={fileMarkdownComponents}
                >
                  {stream}
                </ReactMarkdown>
              </div>
            </article>
          )}
          {taskCards}
          {transcriptEnd < totalItems && (
            <button
              type="button"
              onClick={() => onNewer()}
              className="mx-auto block rounded border px-3 py-2 text-xs text-gray-600"
            >
              Show 200 newer messages
            </button>
          )}
        </div>
      </div>
      <div className="workspace-conversation-notices">
        {requests}
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
              disabled={disabled}
              onChange={(event) => onDraft(event.target.value)}
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
                      isComposing: event.nativeEvent.isComposing,
                    },
                    sendShortcut,
                  )
                ) {
                  event.preventDefault();
                  onSend();
                }
              }}
              placeholder={`Ask ChatGPT… ${shortcut} to send`}
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
                  disabled={!active && (disabled || !draft.trim())}
                  onClick={() => (active ? onStop() : onSend())}
                  className="workspace-send-button"
                  aria-label={
                    active ? "Stop response" : submitting ? "Sending…" : "Send"
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
