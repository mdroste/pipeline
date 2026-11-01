// One open file: its toolbar and the viewer its kind selects. Heavy viewers are
// lazy, so opening a text file never loads PDF, table or notebook code.
import { lazy, Suspense, useMemo, useRef, useState } from "react";
import type { FileAdapter } from "../../lib/fileWorkspaceClient";
import { fetchRange, streamUrl } from "../../lib/fileWorkspaceClient";
import type { FileLocation } from "../../lib/fileLinks";
import {
  availableModes,
  defaultMode,
  fileKind,
  formatFileSize,
  modeLabel,
  type FileMode,
} from "../../lib/fileKinds";
import { confirmDialog } from "../DialogService";
import {
  EmptyState,
  IconButton,
  Menu,
  MenuItem,
  MenuSeparator,
} from "../../ui";
import { Icon } from "../../ui/icons";
import ReportViewer from "../ReportViewer";
import ResearchTablePreview from "../ResearchTablePreview";
import SplitView from "../SplitView";
import SourceDiff from "./SourceDiff";
import SourceEditor from "./SourceEditor";
import type { Buffer } from "./useFileBuffers";
import type { FilePassage } from "./FileWorkspace";
import type { PdfSelection } from "./PdfReader";
import MediaViewer from "./viewers/MediaViewer";
import HtmlViewer from "./viewers/HtmlViewer";
import "./viewers/viewers.css";

const PdfReader = lazy(() => import("./PdfReader"));
const ImageViewer = lazy(() => import("./viewers/ImageViewer"));
const ConvertedImage = lazy(() =>
  import("./viewers/ImageViewer").then((m) => ({ default: m.ConvertedImage })),
);
const TableViewer = lazy(() => import("./viewers/TableViewer"));
const NotebookViewer = lazy(() => import("./viewers/NotebookViewer"));
const DocumentTextViewer = lazy(() => import("./viewers/DocumentTextViewer"));

/** The largest file a "Load more" view keeps in memory. */
const MAX_LOADED_TEXT = 64 * 1024 * 1024;
const LOAD_STEP = 2 * 1024 * 1024;

interface Props {
  adapter: FileAdapter;
  location: FileLocation;
  buffer: Buffer;
  mode?: FileMode;
  setMode: (mode: FileMode) => void;
  saving: string | null;
  completionTexts: string[];
  onDraft: (draft: string) => void;
  onDiscard: () => void;
  onSave: () => void;
  onCompare: () => void;
  onReload: () => void;
  onExternal: () => void;
  onReveal: () => void;
  onPassage: (passage: FilePassage | null) => void;
}
function splitPath(path: string): [string, string] {
  const slash = path.lastIndexOf("/");
  return [path.slice(0, Math.max(slash, 0)), path.slice(slash + 1)];
}
function byteOffset(text: string, index: number) {
  return new TextEncoder().encode(text.slice(0, index)).length;
}
export default function FilePane({
  adapter,
  location,
  buffer,
  mode: chosen,
  setMode,
  saving,
  completionTexts,
  onDraft,
  onDiscard,
  onSave,
  onCompare,
  onReload,
  onExternal,
  onReveal,
  onPassage,
}: Props) {
  const file = buffer.file;
  const kind = fileKind(file);
  const dirty = buffer.draft !== file.text;
  const modes = availableModes(file);
  const mode = chosen && modes.includes(chosen) ? chosen : defaultMode(file);
  const stream = useMemo(
    () => (file.streamToken ? streamUrl(file.streamToken) : null),
    [file.streamToken],
  );
  const [extra, setExtra] = useState("");
  const [extraBytes, setExtraBytes] = useState(0);
  const [loading, setLoading] = useState(false);
  // One decoder for the whole tail: a chunk can end inside a character.
  const tailDecoder = useRef(new TextDecoder());
  const [loadError, setLoadError] = useState("");
  const [menu, setMenu] = useState(false);
  const menuButton = useRef<HTMLButtonElement>(null);
  const [folder, name] = splitPath(file.path);
  const shownText = file.truncated ? buffer.draft + extra : buffer.draft;
  const headBytes = useMemo(
    () => file.textBytes ?? new TextEncoder().encode(file.text ?? "").length,
    [file.textBytes, file.text],
  );
  const shownBytes = file.truncated ? headBytes + extraBytes : file.bytes;
  const canLoadMore =
    file.truncated &&
    Boolean(stream) &&
    shownBytes < file.bytes &&
    shownBytes < MAX_LOADED_TEXT;
  const loadMore = async () => {
    setLoading(true);
    setLoadError("");
    try {
      const bytes = await fetchRange(
        stream!,
        shownBytes,
        Math.min(file.bytes, shownBytes + LOAD_STEP),
      );
      const text = tailDecoder.current.decode(bytes, { stream: true });
      setExtra((old) => old + text);
      setExtraBytes((old) => old + bytes.length);
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };
  const loadWhole = useMemo(
    () =>
      stream
        ? async () => {
            const response = await fetch(stream);
            if (!response.ok)
              throw new Error("Reload this notebook to continue.");
            return response.text();
          }
        : undefined,
    [stream],
  );
  const svgPreview = file.mime === "image/svg+xml" && file.text !== null;
  const renderedPreview = svgPreview ? (
    <img
      alt={file.path}
      src={`data:${file.mime};charset=utf-8,${encodeURIComponent(buffer.draft)}`}
    />
  ) : (
    <ReportViewer markdown={buffer.draft} />
  );
  const editor = (
    <SourceEditor
      value={shownText}
      path={`${adapter.id}/${file.path}`}
      label={`Source editor: ${file.path}`}
      readOnly={!file.editable}
      reveal={location}
      completionTexts={completionTexts}
      onSave={onSave}
      onChange={file.editable ? onDraft : undefined}
      onSelection={(s) =>
        onPassage(
          s.text
            ? {
                path: file.path,
                hash: file.hash,
                quote: s.text,
                start: byteOffset(buffer.draft, s.start),
                end: byteOffset(buffer.draft, s.end),
                line: s.line,
                draft: dirty,
              }
            : null,
        )
      }
    />
  );
  const pdfSelection = (value: PdfSelection) =>
    onPassage({
      path: file.path,
      hash: file.hash,
      quote: value.quote,
      start: 0,
      end: 0,
      line: 0,
      page: value.page,
      draft: false,
    });
  const version = file.stamp ?? file.hash;
  /** A status beside a draft is about the file on disk, unless it just saved. */
  const conflict = dirty && file.editable && buffer.status !== "Saved";
  const notice =
    kind === "data"
      ? { title: "Data files are not shown here", type: "Data" }
      : kind === "table"
        ? { title: "This table cannot be previewed here", type: "Table" }
        : { title: "Pipeline cannot show this type", type: "Binary" };
  const binaryNotice = (
    <div className="file-empty">
      <EmptyState
        icon={<Icon name="file" className="h-8 w-8" />}
        title={notice.title}
        description={`${notice.type} file · ${formatFileSize(file.bytes)}`}
        action={
          (adapter.external || adapter.reveal) && (
            <span className="flex gap-1">
              {adapter.external && (
                <button className="file-action" onClick={onExternal}>
                  Open externally
                </button>
              )}
              {adapter.reveal && (
                <button className="file-action" onClick={onReveal}>
                  Reveal in folder
                </button>
              )}
            </span>
          )
        }
      />
    </div>
  );
  const reload = async () => {
    if (
      dirty &&
      file.editable &&
      !(await confirmDialog(
        `Reload ${file.path} from the project and discard your unsaved draft?`,
        {
          title: "Discard unsaved draft",
          confirmLabel: "Discard and reload",
          destructive: true,
        },
      ))
    )
      return;
    setExtra("");
    setExtraBytes(0);
    tailDecoder.current = new TextDecoder();
    onReload();
  };
  const body = () => {
    if (kind === "pdf" && (stream || file.base64))
      return (
        <PdfReader
          documentKey={`${adapter.id}:${file.path}:${file.hash}`}
          title={file.path}
          initialPage={location.page}
          load={() => Promise.resolve(file.base64 ?? "")}
          stream={stream ? { url: stream, size: file.bytes } : undefined}
          onSelection={stream ? pdfSelection : undefined}
        />
      );
    if (kind === "image" && file.text === null && (stream || file.base64))
      return (
        <ImageViewer
          alt={file.path}
          bytes={file.bytes}
          src={stream ?? `data:${file.mime};base64,${file.base64}`}
        />
      );
    if (kind === "tiff")
      return <ConvertedImage adapter={adapter} file={file} />;
    if ((kind === "audio" || kind === "video") && stream)
      return (
        <MediaViewer src={stream} title={file.path} video={kind === "video"} />
      );
    if (kind === "docx")
      return (
        <DocumentTextViewer
          adapter={adapter}
          path={file.path}
          version={version}
        />
      );
    if (kind === "table" && file.text === null)
      return adapter.table ? (
        <TableViewer adapter={adapter} path={file.path} version={version} />
      ) : (
        binaryNotice
      );
    if (file.text === null || kind === "data") return binaryNotice;
    if (mode === "table")
      return <ResearchTablePreview text={buffer.draft} path={file.path} />;
    if (mode === "diff")
      return (
        <SourceDiff before={file.text} after={buffer.draft} path={file.path} />
      );
    if (mode === "source") return editor;
    if (mode === "preview" && kind === "html" && stream)
      return <HtmlViewer src={stream} title={file.path} stale={dirty} />;
    if (mode === "preview" && kind === "notebook")
      return (
        <NotebookViewer
          text={file.truncated ? null : buffer.draft}
          load={loadWhole}
          path={file.path}
        />
      );
    if (mode === "preview")
      return <div className="file-preview">{renderedPreview}</div>;
    return (
      <SplitView
        key={`${adapter.id}:${file.path}`}
        storageKey={`pipeline.files.sourceSplit.${adapter.id}.${file.path}`}
        firstLabel="Source"
        secondLabel="Preview"
        first={editor}
        second={renderedPreview}
      />
    );
  };
  return (
    <div className="file-pane" tabIndex={0}>
      <div className="file-pane-header">
        <div className="file-crumbs" title={file.path}>
          {folder && <span>{folder.split("/").join(" / ")} /</span>}
          <strong>{name}</strong>
        </div>
        {modes.length > 1 && (
          <div className="file-modes" role="group" aria-label="View">
            {modes.map((m) => (
              <button
                key={m}
                aria-pressed={mode === m}
                onClick={() => setMode(m)}
              >
                {modeLabel(m)}
              </button>
            ))}
          </div>
        )}
        {file.editable && dirty && (
          <>
            <button className="file-action" onClick={onDiscard}>
              Discard draft
            </button>
            <button
              className="file-action primary"
              disabled={saving !== null}
              title="Command/Ctrl+S"
              onClick={onSave}
            >
              {saving === file.path ? "Saving…" : "Save"}
            </button>
          </>
        )}
        <IconButton
          ref={menuButton}
          size="sm"
          label="More file actions"
          tooltipSide="bottom"
          aria-haspopup="menu"
          aria-expanded={menu}
          onClick={() => setMenu(!menu)}
        >
          <Icon name="more" className="h-4 w-4" />
        </IconButton>
        <Menu
          open={menu}
          onClose={() => setMenu(false)}
          anchorRef={menuButton}
          label="More file actions"
        >
          {file.editable && dirty && (
            <MenuItem disabled={saving !== null} onSelect={onCompare}>
              Compare with current file
            </MenuItem>
          )}
          <MenuItem onSelect={() => void reload()}>Reload</MenuItem>
          {(adapter.external || adapter.reveal) && <MenuSeparator />}
          {adapter.external && (
            <MenuItem onSelect={onExternal}>Open externally</MenuItem>
          )}
          {adapter.reveal && (
            <MenuItem onSelect={onReveal}>Reveal in folder</MenuItem>
          )}
        </Menu>
      </div>
      {file.truncated && (
        <p role="status" className="file-status">
          Showing the first {formatFileSize(shownBytes)} of{" "}
          {formatFileSize(file.bytes)}. Editing is disabled.
          {canLoadMore && (
            <button
              className="file-load-more"
              disabled={loading}
              onClick={() => void loadMore()}
            >
              Load more
            </button>
          )}
        </p>
      )}
      {file.approximateText && (
        <p role="status" className="file-status">
          This file is not UTF-8, so some characters may be wrong. Editing is
          disabled.
        </p>
      )}
      {loadError && (
        <p role="alert" className="file-error">
          {loadError}
        </p>
      )}
      {buffer.status &&
        (conflict ? (
          <div role="status" className="file-notice">
            {buffer.status}
            <button disabled={saving !== null} onClick={onCompare}>
              Compare with current file
            </button>
            <button onClick={() => void reload()}>Reload</button>
          </div>
        ) : (
          <p role="status" className="file-status">
            {buffer.status}
          </p>
        ))}
      <Suspense fallback={<p className="file-status">Loading viewer…</p>}>
        {body()}
      </Suspense>
    </div>
  );
}
