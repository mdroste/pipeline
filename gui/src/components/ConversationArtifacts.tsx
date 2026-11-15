// The file workspace is mounted on demand. Private chats can browse their
// working folder. In projects the list comes from assistant messages, so a
// large shared folder is never scanned to find a conversation's files.
import { useEffect, useMemo, useState } from "react";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import remarkParse from "remark-parse";
import { unified } from "unified";
import { normalizeMathDelimiters } from "../lib/mathMarkdown";
import { appEvents } from "../lib/appEvents";
import { resolveFileLink, type FileLocation } from "../lib/fileLinks";
import {
  releaseFileStream,
  type FileAdapter,
} from "../lib/fileWorkspaceClient";
import { conversationClient } from "../lib/workbench/conversationClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type {
  ConversationSnapshot,
  TranscriptCursor,
  TranscriptItem,
} from "../lib/workbench/sessionTypes";
import { isMessage, payloadText } from "../lib/workspaceTranscript";
import { Button } from "../ui";
import FileWorkspace from "./file-workspace/FileWorkspace";

const CONVERSATION_PATH = "conversation.md";
const markdown = unified().use(remarkParse).use(remarkGfm).use(remarkMath);

function linkedPaths(items: TranscriptItem[], root: string): string[] {
  const paths = new Set<string>();
  for (const item of items) {
    if (!isMessage(item) || item.itemKind.toLowerCase().includes("user"))
      continue;
    const text = payloadText(item);
    if (!text.includes("[")) continue;
    const tree = markdown.parse(normalizeMathDelimiters(text));
    const definitions = new Map<string, string>();
    const references = new Set<string>();
    const add = (url: string) => {
      const location = resolveFileLink(CONVERSATION_PATH, url, root);
      if (location) paths.add(location.path);
    };
    function visit(node: (typeof tree.children)[number]) {
      if (node.type === "link" || node.type === "image") add(node.url);
      if (node.type === "definition" && !definitions.has(node.identifier))
        definitions.set(node.identifier, node.url);
      if (node.type === "linkReference" || node.type === "imageReference")
        references.add(node.identifier);
      if ("children" in node) node.children.forEach(visit);
    }
    tree.children.forEach(visit);
    for (const reference of references) {
      const url = definitions.get(reference);
      if (url) add(url);
    }
  }
  return [...paths];
}

function normalizeLocation(location: FileLocation | null, root: string) {
  if (!location) return null;
  // Paths from the transcript have already been decoded. Encode once for the
  // shared resolver while preserving native Windows drive and UNC syntax.
  const href = location.path.split("/").map(encodeURIComponent).join("/");
  const mapped = resolveFileLink(CONVERSATION_PATH, href, root);
  return mapped ? { ...location, path: mapped.path } : location;
}

export default function ConversationArtifacts({
  snapshot,
  active,
  initial,
  onClose,
}: {
  snapshot: ConversationSnapshot;
  active: boolean;
  initial: FileLocation | null;
  onClose: () => void;
}) {
  const sessionId = snapshot.session.id;
  const [scope, setScope] = useState<Awaited<
    ReturnType<typeof conversationClient.listConversationFiles>
  > | null>(null);
  const [history, setHistory] = useState<{
    paths: string[];
    next: TranscriptCursor | null;
  } | null>(null);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let current = true;
    void conversationClient.listConversationFiles(sessionId).then(
      (result) => {
        if (current) setScope(result);
      },
      (cause) => {
        if (current) setError(workbenchErrorMessage(cause));
      },
    );
    return () => {
      current = false;
    };
  }, [sessionId, active, attempt]);
  const root = scope?.scopeRoot;
  const privateFolder = Boolean(scope?.listing);
  const location = useMemo(
    () => (root ? normalizeLocation(initial, root) : initial),
    [initial, root],
  );
  const adapter = useMemo<FileAdapter>(
    () => ({
      id: `conversation:${sessionId}:${root ?? ""}`,
      artifactSessionId: sessionId,
      read: (path) =>
        conversationClient.previewConversationFile(sessionId, path),
      readInline: (path) =>
        conversationClient.readConversationFile(sessionId, path),
      save: (file, content) =>
        conversationClient.saveConversationFile({
          sessionId,
          path: file.path,
          scopeRoot: file.scopeRoot ?? "",
          expectedHash: file.hash,
          content,
        }),
      release: releaseFileStream,
      external: (path) =>
        conversationClient.openConversationFile(sessionId, path),
      openCopyWith: (path) =>
        conversationClient.openConversationFileWith(sessionId, path),
      ...(scope?.listing
        ? {
            list: async (directory: string) => {
              if (!directory) return scope.listing!;
              const { listing } =
                await conversationClient.listConversationFiles(
                  sessionId,
                  directory,
                );
              if (!listing) throw new Error("This working folder has changed.");
              return listing;
            },
          }
        : {}),
    }),
    [sessionId, root, scope],
  );
  const recent = useMemo(
    () => (root && !privateFolder ? linkedPaths(snapshot.items, root) : []),
    [root, privateFolder, snapshot.items],
  );
  const paths = useMemo(
    () => [...new Set([...recent, ...(history?.paths ?? [])])],
    [recent, history?.paths],
  );
  const before = history ? history.next : snapshot.olderCursor;
  const projectId =
    !snapshot.session.overrides?.projectCheckpointId &&
    root &&
    root === snapshot.workspace?.root
      ? snapshot.workspace.id
      : null;
  const openInFiles = (location: FileLocation) => {
    if (!projectId) return;
    sessionStorage.setItem(
      `pipeline.openFile.${projectId}`,
      JSON.stringify(location),
    );
    onClose();
    appEvents.emit("open-file", { workspaceId: projectId, location });
  };
  const loadOlder = async () => {
    if (!before || !root || loadingOlder) return;
    setLoadingOlder(true);
    setError("");
    try {
      const page = await conversationClient.transcriptPage(sessionId, before);
      const paths = linkedPaths(page.items, root);
      setHistory((previous) => ({
        paths: [...new Set([...(previous?.paths ?? []), ...paths])],
        next: page.nextCursor,
      }));
    } catch (cause) {
      setError(workbenchErrorMessage(cause));
    } finally {
      setLoadingOlder(false);
    }
  };
  return (
    <div className="flex h-full min-h-0 flex-col">
      {error && (
        <p className="file-error" role="alert">
          {error}
        </p>
      )}
      {scope ? (
        <>
          {!privateFolder && !paths.length && (
            <p className="shrink-0 border-b border-line px-4 py-2 text-ui-meta text-ink-muted">
              File links from the assistant appear here.
            </p>
          )}
          <div className="min-h-0 flex-1">
            <FileWorkspace
              key={adapter.id}
              adapter={adapter}
              paths={paths}
              initial={location}
              initialFileListOpen={!initial}
              allowSplit={false}
              editOnDemand
              onOpenInFiles={projectId ? openInFiles : undefined}
              fileListLabel={
                privateFolder || paths.length ? "Artifact files" : undefined
              }
            />
          </div>
          {!privateFolder && before && (
            <div className="shrink-0 border-t border-line px-4 py-2">
              <Button disabled={loadingOlder} onClick={() => void loadOlder()}>
                {loadingOlder
                  ? "Finding files…"
                  : "Find files in earlier messages"}
              </Button>
            </div>
          )}
        </>
      ) : error ? (
        <div className="px-4 py-2">
          <Button
            onClick={() => {
              setError("");
              setAttempt((value) => value + 1);
            }}
          >
            Retry
          </Button>
        </div>
      ) : (
        <p className="px-4 py-2 text-ui-meta" role="status">
          Loading artifacts…
        </p>
      )}
    </div>
  );
}
