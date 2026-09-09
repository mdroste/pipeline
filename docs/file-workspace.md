# File workspace

The shared file workspace connects source editing, Markdown, PDFs and revision
review. The presentation components are shared; Workspace files use Workspace
services and Workflow artifacts remain read-only in their owning run.

## Locations and navigation

- Workspace → project → Library → **Files** opens the attached folder's files.
  Quick open accepts a filtered inventory entry or a normalized relative path.
  Cmd/Ctrl+P focuses it. Tabs, back/forward history, two file panes, remembered
  editor/PDF positions, external open, and reveal-in-folder are available.
- Chat Markdown links open project files. Relative links resolve from the
  project root; absolute agent-authored links are converted to project-relative
  paths only when they remain inside the registered root. In unfiled or rootless
  conversations, an absolute generated-file link opens through a native command
  only after validation against that conversation's private runtime root. The
  explicit quoted selection action carries path, content identity, UTF-8
  offsets, line and unsaved-draft status.
- Workflow Sources retains its structured bundle/page inspection. **Open in file
  tabs** opens the shared workspace. Report links resolve within the retained
  run and open the referenced source line, PDF page or Markdown heading.
- Imported source trees expose a captured-file selector. Captured-file links and
  images resolve inside that immutable tree. A single-file import cannot resolve
  siblings that were not captured; the UI reports the missing target.

`fileWorkspaceClient.ts` supplies separate owning adapters. `FileNavigation`
provides document-relative navigation and images to reports, chat and file panes.
URLs may carry `#L12`, `#page=4`, or a heading fragment. Traversal outside the root,
Git/task metadata, unsafe URL schemes and uncaptured assets are not resolved.
HTTP/HTTPS/mailto links open through the OS. Markdown supports GFM, math, source,
preview and split modes, and highlighted code blocks with copy controls.

Each report/Markdown reader namespaces its rendered heading, issue and footnote
IDs. Contents links and same-document fragments scroll and focus only that
reader, including when the same file is open twice. Canonical source fragments
remain in `data-source-anchor` for file navigation; saved source text and export
identities are unchanged. Contents text, active links and keyboard focus use
theme-specific semantic colors.

## Editing and revision review

`SourceEditor` uses CodeMirror 6 with line numbers, search/replace, indentation,
bracket matching, folding, multiple selections, go-to-line, line wrapping,
manual language selection, and Cmd/Ctrl+S. A shared language registry covers
LaTeX/BibTeX, Stata, Python, R, Julia, MATLAB/Octave, Markdown, SQL, shell,
configuration formats, and common programming languages. Stata uses an explicit
stream mode; code previews/fences use highlight.js grammars. Language modes load
on demand. LaTeX completion suggests captured labels and bibliography keys from
open files; the Manuscript panel also reads a bounded set of project TeX/BibTeX
files. This is lexical completion, without a language-server process.

Each file's unsaved draft retains the loaded content and hash independently.
Switching or closing tabs retains drafts; restored drafts preserve their original
hash if another editor changed the file. Saves use Studio's expected hash,
recoverable application journal, execution exclusion, and check invalidation.
Typing during an asynchronous file-workspace save or reload is preserved. A reload that encounters a changed buffer keeps its draft and original base hash. The editor preserves LF/CRLF separators and translates selection positions back into serialized-source offsets before creating UTF-8 byte spans. No immutable
revision or Workflow output is edited in place. Existing platform restrictions
on file acceptance remain; the shared file viewer exposes editing on Unix.

`SourceDiff` uses exact offsets, preserving CRLF, Unicode and final-newline
changes. It bounds detailed diff computation and falls back to a complete file
replacement rather than accepting a truncated preview. Edits & acceptance can
accept selected changes in an existing text file as a separate reversible edit.
The original task proposal remains available; subsequent saves still check the
current working file's hash. File additions/deletions and executable changes keep
using whole-file acceptance. Source and simplified-prose comparisons remain
separate.

## PDFs and manuscript navigation

`PdfReader` uses the PDF.js compatibility build, with its worker, CMaps, fonts,
image resources and codecs bundled for offline development and packaging.
It provides continuous scrolling, text selection, annotation links, highlighted
search results, page labels, outline, paged thumbnails, zoom, fit modes and
rotation. Only supplied, scoped bytes enter the reader. PDF scripts are not
connected to an execution manager. Existing Poppler page images remain an
explicit fallback for Workflow artifacts and imported Workspace papers.

Each PDF viewer owns an abort signal. Replacing a document, retrying, entering
fallback mode or unmounting aborts its listeners/observer, clears viewer and
link-service document references, and destroys the loading task with handled
teardown errors. Component tests cover replacement, retry, fallback and unmount;
they do not constitute a native or packaged memory-stress qualification.

Workspace text selections retain a normalized PDF page region and a quote;
image-only pages retain region selection. Saved annotations reopen their exact
revision. Source text selections continue to use validated UTF-8 byte spans.

The Manuscript editor uses the shared source editor, automatically opens a
selected diagnostic's file/line, and can display the exact build PDF beside the
source. Double-clicking a PDF point requests the existing exact-build SyncTeX
mapping. Working source can differ from the captured build, which is disclosed
when navigating. Missing SyncTeX keeps the existing source/page fallback.

`PdfComparison` synchronizes page, scale and rotation, with independent navigation
as an option. Changed-page navigation compares rendered page thumbnails at equal
page numbers. It is cancellable and uses bounded canvases; it is a visual aid,
not semantic page alignment or a guarantee that subpixel changes are detected.

Limits: scoped file reads and interactive PDF transfer stop at 32 MiB; editable
text stops at 2 MiB; existing Workflow text previews keep their own bounds.
Interactive PDFs stop at 5,000 pages; the outline is bounded to 2,000 entries and
12 nesting levels, and thumbnails load 12 at a time. Plain chat does not load
CodeMirror or PDF.js until their owning file surfaces open.

## Implementation and verification

Frontend: `gui/src/components/file-workspace/`, `fileLanguages.ts`, `fileLinks.ts`,
`fileDiff.ts`, `fileWorkspaceClient.ts`, and `pdfAssets.ts`. `pdfAssetsPlugin.ts`
serves/copies the PDF resources. Workspace reads live in
`workbench/project/file_workspace.rs`; Workflow PDF reads live in
`runs/artifacts.rs`. `file_viewer.rs` contains only the neutral native reveal
helper. No new model protocol, credential store, scheduler or migration is added.

The historical full-editor exclusion in `notes/workbench_plan.md` is superseded for
this feature by the user's explicit implementation request.

Verification covers scoped reads, retained-file tampering, the existing
16-character Workflow digest, academic languages, exact diff application,
per-file draft restoration/conflicts/concurrent saves, Markdown navigation,
PDF controls and location conversion, and existing reader/studio regressions.
The frontend production build, 593 frontend tests (80 files, run without file
parallelism), 893 Rust library tests (10 ignored), and strict Clippy checks passed.
Parallel frontend runs exposed intermittent timing failures in existing Context
Tray and Harness Editor tests; the complete sequential run passed. A real,
two-page LaTeX PDF was read and rendered using PDF.js with local resources;
text and internal/external annotations were inspected. Native Tauri UI inspection
is pending because the host Mac is locked. This does not qualify packaged or
cross-platform behavior; retain the normal release qualification gates.
