# Pipeline pre-release bug report — October 21, 2026

## Scope and release assessment

Reviewed Pipeline **1.1.0**, commit **`2a9236af65f09c8ed1a3a207720b26c5cd0a279c`**, with particular attention to macOS, Windows, Linux, filesystem boundaries, executable discovery, packaged resources, and user-visible failure handling. The checkout was clean before this review. Implementation and regression-test results are now recorded alongside the original audit evidence below. Original line numbers refer to the audited commit.

**Implementation status: all 11 findings have fixes and regression coverage in this checkout, starting with the three release blockers.** GitHub Actions now passes on Linux, macOS, and Windows. OneDrive hydration and installed-artifact qualification are still required before release; deterministic source tests and synthetic resource layouts do not certify a shipped installer's PDF workflow.

The October 14 report in `docs/bug-triage-2026-10.md` predates substantial fixes already present in this checkout. Its findings 1–44 are not simply repeated here. Finding 11 below is an explicitly identified carryover from that report, now addressed here.

Severity convention: **P1** = release blocker for an affected advertised feature or filesystem boundary; **P2** = significant functional or developer-workflow defect. “Reproduced” means the audited implementation was exercised with synthetic fixtures. “Source-confirmed” means the relevant call chain was traced but the native target or packaged installer was not executed. After each resolution/validation note, the original trigger, analysis, and proposed coverage remain as audit history; descriptions of defective behavior refer to the reviewed commit.

| # | Priority | Affected platform(s) | Finding | Current status |
| --- | --- | --- | --- | --- |
| 1 | P1 | Windows | Project reads escape through ancestor junctions | Fixed; Windows native regressions pass |
| 2 | P1 | Case-insensitive macOS; Windows | Metadata case aliases bypass exclusions | Fixed; public API regressions pass |
| 3 | P1 | Packaged CLI, all platforms | Bundled Poppler is never initialized | Fixed; resource/process regressions pass; installer checks pending |
| 4 | P2 | Windows | Research lookup omits executable extensions | Fixed; portable and native Windows regressions pass |
| 5 | P2 | Windows | LaTeX validation rejects `.exe` engines | Fixed; classifier/profile regressions pass |
| 6 | P2 | macOS, Linux | Research ignores recovered desktop PATH | Fixed; isolated-process regression passes |
| 7 | P2 | Windows | Canonical paths break Markdown links/images | Fixed; resolver/rendering regressions pass |
| 8 | P2 | All | Project viewer loses absolute-link scope | Fixed; backend/rendering regressions pass |
| 9 | P2 | DST-observing time zones | “Wait until” displays the wrong hour | Fixed; component/timezone regressions pass |
| 10 | P2 | Developer machines | Release tests assume `python` | Fixed; default release suite passes |
| 11 | P2 | All | Stop/Export failures have no feedback | Fixed; both Stop entry points and export retries pass |

## Itemized findings, fixes, and regression coverage

### 1. Windows project-file reads follow ancestor junctions outside the authorized root

**Priority:** P1. **Status:** fixed; Windows native regressions pass; OneDrive qualification pending.

**Resolution.** Windows `SafeRoot` now opens one component at a time relative to retained native directory handles using `NtCreateFile`. It refuses junctions, symlinks, and unknown reparse providers, checks resolved long names against reserved metadata (including DOS aliases), and reads from the opened file. Cloud files are reopened through their verified handle for hydration, with a final-path check. Missing files now produce the same optional-read result as Unix. The implementation follows the [Windows handle-relative open contract](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile).

**Validation.** Added Windows regressions for outside/inside junction refusal, directory/root replacement after opening handles, missing and ordinary files, and the public project/conversation readers. The actual Windows module and its tests successfully type-check for `x86_64-pc-windows-gnu` using a temporary standalone harness on macOS. That initial check was followed by successful native execution on `windows-2022`, including junction refusal, retained-parent/root replacement, and public file readers. The CI fixture correction is recorded below; cloud-provider behavior still requires manual qualification.

**Locations:** `gui/src-tauri/src/workbench/project/files.rs:154–167`; `gui/src-tauri/src/safety.rs:162–218`; `gui/src-tauri/src/workbench/project/file_workspace.rs:157–165` and `:265–284`.

**Trigger and impact.** A registered project contains a junction such as `project/linked` pointing to a separate directory. Requesting `linked/private.txt` can read the external file through the project/conversation file services, even though the path appears to be a normalized project-relative path. The parent link can also be introduced after registration. The same failure matters when content is rendered through an embedded image or a file link.

**Why it happens.** The Unix `SafeRoot` implementation walks directories with descriptor-relative `openat` calls and `O_NOFOLLOW`. Its Windows/non-Unix branch instead joins the untrusted relative path to `self.path` and calls `open_regular_file`. That helper checks the final file, not every ancestor directory. The root handle opened by `SafeRoot::open` does not constrain the Windows read. Neither the final file's regular-file type nor the registered root's identity proves that intermediate components stayed inside the root. Junctions are directory reparse points; see [Microsoft's junction documentation](https://learn.microsoft.com/en-ca/windows/win32/fileio/hard-links-and-junctions).

**Reproduction to run on Windows.** Use a disposable registered project and a separate temporary directory containing a sentinel text file. Create a junction inside the project to the external directory. Call `read_workspace_file` with the junction-relative path and separately exercise `read_conversation_file` for a conversation rooted in that project. Expected: refusal before returning any bytes. The current Windows branch has no ancestor or final-handle containment check to enforce that result.

**Proposed fix.** Implement a Windows root-bound reader that validates directory/reparse-point traversal and verifies the opened file's final location against the registered root using handles. Keep the verified handle for the read. Reject ancestor junctions/symlinks that escape the scope. A pathname-only `canonicalize` check followed by a separate open would leave a replacement race. Preserve legitimate cloud-file hydration without treating it as permission to follow arbitrary links.

**Regression coverage.** On Windows, test an outside junction, an inside junction according to the chosen policy, a directory symlink, a parent swapped during open, ordinary nested files, and a OneDrive placeholder where available. Run these against the public project and conversation readers, not just a path-string helper.

### 2. Case variants bypass metadata exclusions, including actual writes into `.git` on macOS

**Priority:** P1. **Status:** fixed; public API regression passed on macOS.

**Resolution.** A shared reserved-component guard rejects ASCII case variants and trailing-dot/space aliases of Git, task, and apply metadata. Capture/editor exclusions apply the same case policy to build/cache directories. Windows paths additionally reject alternate data streams and trailing-dot/space components. Windows handle checks also inspect the resolved long name.

**Validation.** `metadata_case_aliases_are_refused_by_read_edit_save_and_checkpoint` passed. It exercises the public read, editor, Studio save, and checkpoint paths, checking that synthetic metadata remains unchanged after rejected operations; mixed-case and ordinary-source validation are included.

**Locations:** `gui/src-tauri/src/workbench/project/files.rs:4–19`; `gui/src-tauri/src/workbench/project.rs:611–630`; `gui/src-tauri/src/workbench/project/studio/manuscript.rs:29–39` and `:63–111`.

**Trigger and impact.** Opening `.git/config` is rejected, but opening `.GIT/config` succeeds when the filesystem resolves both names to the same directory. The shared guards also permit editor saves through the alias. This bypasses the explicit promise that project operations exclude Git/task metadata. Git configuration, hooks with supported source extensions, task copies, and recovery metadata deserve filesystem-aware protection.

**Executed reproduction.** In a fresh temporary project/store, the audit created only synthetic files:

1. `read_workspace_file(".git/config")` returned an error.
2. `read_workspace_file(".GIT/config")` returned `audit-only sentinel`.
3. `editor_file(".GIT/audit.md")`, followed by `studio_mutate(SaveText)` with its expected hash, returned `saved: true`.
4. Reading the real `.git/audit.md` afterwards returned `changed metadata`.

No real repository metadata was used in this reproduction. Windows uses the same lexical guard for reads; Windows writes remain subject to the separately documented unsupported-write limitation.

**Why it happens.** Both `relative` and `ignored` compare reserved directory names using case-sensitive string equality. Their decision disagrees with the filesystem. The save path passes through both checks, captures a temporary copy, and applies the write under the alternate spelling.

**Proposed fix.** Centralize reserved-component validation and conservatively reject ASCII case variants of `.git`, `.pipeline-tasks`, and `.pipeline-apply-*` at every read/write/capture boundary. Apply the same policy to ignored build/cache paths where aliases could bypass capture rules. Audit Windows trailing-dot/space and other name aliases as part of that implementation. Do not rely only on frontend navigation filtering.

**Regression coverage.** Verify reads, `editor_file`, `SaveText`, checkpoint selection, and apply/undo using `.GIT`, `.GiT`, and mixed-case task metadata. On a case-insensitive test volume, assert that the canonical metadata bytes remain unchanged after every rejected operation. Keep ordinary source-file editing covered on both case-sensitive and case-insensitive volumes.

### 3. The installed CLI cannot discover the PDF tools bundled with Pipeline

**Priority:** P1 for the documented self-contained CLI PDF workflow. **Status:** fixed; resource regressions passed; clean-installer PDF qualification pending.

**Resolution.** Shared `env::initialize_resources` is invoked before GUI/CLI dependency checks. It resolves macOS bundle Resources, Windows adjacent resources, and Linux `usr/bin` → `usr/lib/Pipeline` resources from the executable, independent of the working directory. Linux uses Tauri's configured product name rather than assuming the Rust crate name; a crate-name fallback is also retained. Missing bundles retain system-tool fallback.

**Validation.** All three resource tests passed: candidate layouts for all platforms, explicit GUI resources/absent bundle, and a copied test process launched from a simulated native bundle with empty PATH (through a symlink on Unix). Installed CLI extraction and page rendering remain part of the native package release gate.

**Locations:** `gui/src-tauri/src/bin/cli.rs:249–287`; `gui/src-tauri/src/lib.rs:36–49` and `:80–84`; `gui/src-tauri/src/env.rs:34–66`; `gui/src-tauri/src/deps/command.rs:322–352`.

**Trigger and impact.** Install Pipeline on a machine without a separate Poppler installation, then invoke the shipped `pipeline-cli` on a PDF. Required `pdftotext`/`pdftoppm` probes search the inherited/recovered PATH and can report missing dependencies even though both utilities are present in the application bundle. This affects Poppler extraction and other extraction modes that require those tools. A developer's Homebrew/APT/Scoop installation masks the defect.

**Why it happens.** The only call to `env::set_bundled_poppler_dir` is inside the GUI's Tauri setup callback. The CLI starts its own Tokio runtime and dispatches directly, without running that callback or locating application resources. `env::full_path` prepends bundled Poppler only after that process-local state has been initialized. Launching the GUI earlier cannot initialize a separate CLI process.

**Proposed fix.** Move packaged-resource discovery into a shared initializer usable without opening a Tauri window. Call it before GUI or CLI dependency checks. Resolve resources from the installed executable/bundle layout, including macOS `Contents/Resources`, the Windows installation directory, and the actual Linux AppImage layout. Keep source-checkout/system-tool fallback explicit. Do not assume the caller's working directory is the installation directory.

**Regression coverage.** Test resource-location candidates independently, then run the installed CLI's `check` and PDF extraction with a minimal PATH on each packaged platform. Include a PDF requiring both text extraction and page rendering. Verify that dependency diagnostics identify the bundled tool path and that no unrelated system Poppler is being used.

### 4. Windows research command lookup does not resolve normal executable extensions

**Priority:** P2. **Status:** fixed; portable resolver and native Windows execution regressions passed.

**Resolution.** Research profiles now share the executable resolver with provider commands, using an explicit PATH and profile cwd. Windows extension search, spaced/Unicode paths, and safe shim argument handling are retained; arbitrary batch wrappers remain rejected. The manifest fingerprints the actual executable, prefix launch scripts, and effective PATH/PATHEXT, and execution consumes those resolved arguments.

**Validation.** Portable Windows-resolution fixtures passed for extensionless/explicit `.exe`, case variants, Unicode/spaces, relative PATH entries, and rejected batch wrappers. The Windows-only preview-to-execution test passed on `windows-2022`.

**Locations:** `gui/src-tauri/src/workbench/research/execution.rs:1387–1408`; default manuscript commands are constructed in `gui/src-tauri/src/workbench/project/studio/manuscript.rs:170–194`.

**Trigger and impact.** A Windows user installs Python or a TeX engine on PATH and configures `python` or `pdflatex`, including a default manuscript build. Preview/authorization reports “Execution program is missing” because the resolver looks only for the literal extensionless filename. It never reaches process execution.

**Executed reproduction.** The audit saved a command execution profile using `audit-tool`, with its explicit PATH containing a regular file named `audit-tool.exe`. `preview_host_execution` returned the missing-program error. This exercises the same platform-independent scan used on Windows; it does not emulate Windows process creation.

**Why it happens.** `launch_manifest` uses `split_paths(...).map(|p| p.join(requested)).find(|p| p.is_file())`. It has no Windows extension search. Pipeline already has a more capable provider-command resolver in `deps/command.rs`, but this research path does not use it.

**Proposed fix.** Share an executable-resolution primitive with explicit PATH input, platform rules, and working-directory semantics. Support native `.exe`/`.com` resolution on Windows while retaining argument boundaries. If script shims are supported, resolve their actual interpreter and prefix arguments rather than routing arbitrary arguments through a command shell. Fingerprint and launch the same resolved command. Preserve Python virtual-environment invocation paths and Unix multicall aliases.

**Regression coverage.** Test extensionless Python/TeX names, explicit `.exe`, paths containing spaces and non-ASCII characters, PATH overrides, relative PATH components, and unsupported batch wrappers on Windows. Preview, authorization, and execution must agree about the chosen program.

### 5. Windows LaTeX engine names with `.exe` are rejected before execution

**Priority:** P2. **Status:** fixed; classifier and profile regressions passed on macOS; Windows cases included in native CI.

**Resolution.** One platform-aware LaTeX engine classifier is used for both validation and argument insertion. Windows `.exe` and case variants are accepted; Tectonic avoids the TeX shell-escape switch, latexmk receives `-norc`, and shell-escape requests are still rejected.

**Validation.** Tests cover all supported engine names, Windows paths and uppercase suffixes, invalid wrappers/lookalikes, and public profile save/read behavior for engine-specific safety flags.

**Locations:** `gui/src-tauri/src/workbench/research/execution.rs:197–211` and `:235–244`.

**Trigger and impact.** Trying to work around finding 4 with `pdflatex.exe` or an absolute Windows path ending in it still fails: “LaTeX profiles require a supported engine and cannot enable shell escape.” The same issue applies to `xelatex.exe`, `lualatex.exe`, `tectonic.exe`, and native `latexmk.exe` launchers. Thus simply documenting an absolute path does not repair Windows manuscript builds.

**Executed reproduction.** A fresh workspace and `adapter: "latex"`, `argv: ["pdflatex.exe"]` were passed to the current public save API. Validation rejected the profile with the above error before any executable was launched.

**Why it happens.** The allowlist compares the complete basename against extensionless, case-sensitive names. Later engine-specific handling also recognizes `tectonic`/`latexmk` using extensionless string tests, so fixing only the first allowlist would leave inconsistent flag injection.

**Proposed fix.** Introduce one engine-classification helper that recognizes supported native executable suffixes and platform-appropriate case rules. Use it for validation and engine-specific argument construction. Preserve shell-escape restrictions and continue rejecting unrelated executables or arbitrary wrappers. Handle installed `latexmk` distributions according to the executable representation actually supported.

**Regression coverage.** Cover both extensionless and `.exe` variants, absolute paths, mixed-case Windows names, and Tectonic/latexmk flag behavior. Combine this with finding 4 in a real Windows “save build → preview → authorize → compile” test.

### 6. Research execution ignores the login-shell PATH recovered for desktop launches

**Priority:** P2. **Status:** fixed; isolated-process regression passed on macOS.

**Resolution.** Research lookup uses the recovered application PATH unless the profile overrides it. The exact effective PATH is included in authorization and installed in the child process. Resolution retains Unix executable-permission checks and invocation aliases.

**Validation.** A fresh test process with a minimal inherited PATH and synthetic login shell successfully resolves a command and runs its PATH-only helper. An explicit override wins, and an altered PATH invalidates the previous launch authorization. Existing resolver tests cover non-executable shadows and preserved aliases.

**Locations:** `gui/src-tauri/src/workbench/research/execution.rs:1394–1403` and `:419–457`; compare `gui/src-tauri/src/env.rs:105–129` and `gui/src-tauri/src/deps/command.rs:322–352`.

**Trigger and impact.** Start the app from Finder or a Linux desktop with a minimal PATH. A tool installed under a login-shell-managed directory is detected by Pipeline's normal dependency machinery, but a research command using its bare name cannot be previewed or authorized. Supplying an absolute executable may get past lookup while child programs launched by that executable still inherit the deficient PATH.

**Executed reproduction.** An isolated test supplied a fake login shell that returned a temporary tool directory plus `/usr/bin:/bin`. The actual `env::full_path()` included the executable. With inherited PATH set to `/usr/bin:/bin`, the actual `preview_host_execution` still returned “Execution program is missing.” The environment changes were confined to the temporary test process.

**Why it happens.** Research lookup reads `std::env::var_os("PATH")` rather than Pipeline's recovered `env::full_path()`. The launch path also never installs the recovered PATH unless the profile supplies one. Unlike the Windows extension bug, this fails even when the executable's filename exactly matches the request.

**Proposed fix.** Resolve and execute research commands with the same effective environment: an explicit profile PATH when present, otherwise the recovered app PATH. Include the effective value in the authorization identity where changes could alter tool resolution. Preserve profile overrides and use executable permission checks so a non-executable file does not shadow a later executable.

**Regression coverage.** Test a GUI-like minimal environment with login-shell-only Python/TeX tools and a command that launches a second helper from that directory. Verify preview and launch use the same PATH, and that explicit profile overrides still win.

### 7. Windows canonical paths break project Markdown links and inline images

**Priority:** P2. **Status:** fixed; resolver and rendered-view regressions passed.

**Resolution.** Native path parsing now normalizes extended drive and scoped UNC roots/targets, preserves line/page fragments, and refuses device namespaces, query strings, controls, and out-of-root paths. The Markdown renderer preserves supported native paths until scoped link/image validation; its default URL transform had also discarded drive-letter links as unknown protocols. Metadata case aliases are filtered in the frontend as well as the backend.

**Validation.** Tests exercise ordinary/extended drive paths, Unicode/spaces, UNC roots, unsafe paths, fragments, and actual Markdown link/image rendering with a Windows canonical root. Native Windows installer qualification remains separate.

**Locations:** `gui/src/lib/fileLinks.ts:9–16`, `:29–34`, and `:58–74`; `gui/src-tauri/src/workbench/store/workspaces.rs:55–56`; `gui/src-tauri/src/workbench/store.rs:737–745`; `gui/src/components/WorkspaceConversationView.tsx:137–163`.

**Trigger and impact.** Register a Windows project and receive a model reply containing an absolute project file link or image. Rust stores the canonical project root, but the frontend only recognizes ordinary drive-letter roots. On Windows, Rust canonicalization uses extended-length paths, as documented in the [Rust standard library](https://doc.rust-lang.org/std/fs/fn.canonicalize.html).

**Executed reproduction using the actual TypeScript module:**

```text
resolveFileLink("conversation.md", "C:/Research/main.tex#L7", "C:\\Research")
  => { path: "main.tex", line: 7 }

resolveFileLink("conversation.md", "C:/Research/main.tex#L7", "\\\\?\\C:\\Research")
  => null

absoluteLocalFilePath("\\\\?\\C:\\Research\\main.tex")
  => null
```

**User-visible distinction.** An ordinary drive-letter hyperlink may take the conversation's native-open fallback rather than the in-app source/page navigation, losing its location information. An extended-length link itself is rejected, including by the fallback. An ordinary absolute inline image falls back to an absolute path passed to the project adapter, whose backend requires a relative path; it fails to render.

**Why it happens.** The frontend does not normalize `\\?\` roots before drive-letter classification. A raw `?` is also rejected as a URL query before extended-path syntax can be recognized. The tests cover an ordinary `C:\...` root but not the canonical representation emitted by the Windows backend.

**Proposed fix.** Define a consistent native-path representation across the DTO boundary, or normalize supported extended drive/UNC forms before URL parsing. Preserve scope validation and reject device namespaces. Resolve in-root targets to relative locations while retaining line/page fragments. Do not broadly permit arbitrary UNC or external paths merely to make the regex pass.

**Regression coverage.** Use actual Windows canonical roots in integration tests. Cover ordinary and extended drive paths, spaces, Unicode, supported UNC projects, inline images, line/page fragments, and out-of-root targets.

### 8. Absolute links stop working when project Markdown is opened in the file viewer

**Priority:** P2. **Status:** fixed; backend scope and rendered-view regressions passed.

**Resolution.** File previews now carry a backend-selected `scopeRoot` for live projects/checkpoints. Each viewer pane uses that scope for navigation. Absolute targets start at the scope root, while relative targets start beside the current document. Captured revisions receive no live root.

**Validation.** Nested Markdown tests render distinct root/relative image bytes and open distinct CSV sentinel files on POSIX and Windows-shaped roots. Captured Markdown refuses live absolute links/images. Backend reader tests verify that live previews include their canonical root and captures do not.

**Locations:** `gui/src/components/file-workspace/FileWorkspace.tsx:292–304` and `:423`; `gui/src/components/file-workspace/ProjectFiles.tsx:18–30`; `gui/src/lib/fileLinks.ts:60–80`.

**Trigger and impact.** Open a project Markdown document containing an absolute link or image to another file in the same project. The conversation view supplies the registered root, but the project file viewer's navigation object has no root. Those absolute references become unavailable in the document preview even when they point to authorized project files. This is especially noticeable in model-generated reports that use absolute paths.

**Why it happens.** The file adapter carries workspace/checkpoint/revision identity, but no root reaches `FileNavigationScope`. `resolveFileLink` consequently cannot convert an absolute project path into a scoped relative path.

**Related defect that must be fixed together.** Simply supplying the root is insufficient for nested documents. The resolver currently converts an absolute target to a root-relative string and then prefixes the current document's directory:

```text
resolveFileLink("notes/review.md", "/project/results.csv", "/project")
  => { path: "notes/results.csv" }   // should be results.csv
```

This second result was executed against the current module. It is a latent wrong-target bug in the root-aware helper; the current project viewer fails earlier because it omits the root. The report does not claim the current viewer already supplies that missing argument.

**Proposed fix.** Thread the authorized live project/checkpoint root into the viewer's navigation context. For a target mapped from an absolute path, begin resolution at the scope root; for a genuine relative link, begin at the current document's parent. Keep immutable captured revisions scoped to their captured contents rather than mapping back into a live project.

**Regression coverage.** Render a nested project Markdown file with absolute and relative links/images, with different sentinel files at `results.csv` and `notes/results.csv`. Verify both displayed bytes and clicked target. Cover captured revisions separately and retain out-of-root refusal tests.

### 9. “Wait until” displays and edits the wrong local time across daylight-saving changes

**Priority:** P2. **Status:** fixed; component and timezone regressions passed.

**Resolution.** Outline and timing inputs share a target-date conversion. Invalid/nonexistent DST wall times are refused; repeated autumn times use the browser's earlier occurrence, explained by the input hint. Invalid date values no longer throw during formatting.

**Validation.** The actual outline input displays the November deadline correctly while “today” has a summer offset. Separate processes test winter/summer targets and minute-only edits in Los Angeles, Berlin, and Tokyo, plus the US spring gap and autumn repeated hour.

**Location:** `gui/src/components/tasks/OutlineEditor.tsx:239–253`. The correct target-date approach already exists in `gui/src/components/tasks/Timing.tsx:5–8`.

**Trigger and impact.** Edit an automation's “Wait until” step whose target date has a different UTC offset from today. The input shows an hour different from the actual stored target. Editing its minutes/date and saving then converts the displayed, wrong wall time back to a timestamp, shifting the intended deadline.

**Executed example.** With timezone `America/Los_Angeles`, an edit date of October 21, 2026, and a stored target of November 21, 2026 at 09:00 PST, the current expression displays `2026-11-21T10:00`. The original timestamp remains correct until the field is edited; the display is wrong immediately.

**Why it happens.** The component subtracts `new Date().getTimezoneOffset()` instead of the offset of `new Date(step.at * 1000)`.

**Proposed fix.** Share the `Timing.tsx` conversion that uses the target date's offset. Specify handling for nonexistent/ambiguous local times rather than allowing an unexplained browser normalization.

**Regression coverage.** Freeze “today” on either side of DST and edit targets on the opposite side. Assert the displayed local value and the saved epoch after a minute-only edit. Include a non-DST zone and both spring and autumn transitions.

### 10. The release test launcher assumes `python`, so documented checks fail on Python-3-only systems

**Priority:** P2 for release/developer tooling. **Status:** fixed; launcher and full release tests passed without a PYTHON override.

**Resolution.** Release tests verify Python 3.11+ and discover `python3`, Windows `py -3`, or a supported `python`. An explicit `PYTHON` executable path is preserved as one argument and failures show the spawn error. Contributor instructions document the prerequisite; CI installs Python 3.12 using a pinned setup action and passes its exact executable path.

**Validation.** Four launcher tests cover Python-3-only discovery, paths with spaces/Unicode, Windows launcher arguments/version rejection, and missing explicit interpreters. The complete 67-test release suite and release-identity validator passed on the audit Mac without setting PYTHON.

**Location:** `scripts/release/paddle-parser-lock.test.mjs:20–26`; contributor test instructions in `CONTRIBUTING.md`.

**Trigger and impact.** Follow the documented `npm run test:release` instructions on a system with working `python3` but no `python` alias. The command fails with an unhelpful assertion even though the Python test itself is runnable. This is a developer-tooling dependency, not evidence that the distributed application requires system Python.

**Executed reproduction.** `python3 --version` returned Python 3.14.5. `spawnSync("python", ["--version"])` returned `status: null` and `error.code: "ENOENT"`. The release suite reported 62 passes and one failed parser-download test. After selecting `PYTHON=python3` and allowing the test's loopback mock server, all 63 tests and release-identity validation passed.

**Why it happens.** The launcher uses `process.env.PYTHON || "python"` and does not report `result.error` when process creation fails. The contributing instructions do not document this executable assumption.

**Proposed fix.** Honor an explicit interpreter setting, otherwise discover a supported Python 3 launcher appropriate to the host (`python3`, or a verified Windows launcher). Check the interpreter version needed by the test script, surface spawn errors clearly, and document the tooling requirement. Configure the CI interpreter explicitly rather than relying on runner aliases.

**Regression coverage.** Exercise a PATH containing only `python3`, an explicit interpreter with spaces in its path, and a missing interpreter. The error should identify the missing prerequisite rather than printing `undefined` or an assertion about a null status.

### 11. Failed Stop and Export conversation actions still provide no visible error

**Priority:** P2. **Status:** fixed; component regressions passed. **Carryover from October 14 finding 46**, not a newly discovered regression.

**Resolution.** Shared conversation actions catch Stop/dialog/export failures, prevent duplicate pending requests, and expose pending state. Stop failures trigger status reconciliation without asserting cancellation. Feedback remains visible even when the chat pane is hidden. Export cancellation is a no-op and success shows the destination; stale results do not post feedback to another conversation.

**Validation.** Tests reject Stop from both chat and project controls, assert visible errors, pending-button disabling, and successful retry while the turn remains active. Export tests cover dialog cancellation, dialog rejection, write failure, retained conversation state, and successful retry/destination feedback. Vitest reports no unhandled rejections.

**Locations:** `gui/src/components/WorkspacePage.tsx:506–513`, `:693–695`, and `:904–910`; `gui/src/hooks/useWorkspacePageController.ts:732–741`.

**Trigger and impact.** The interrupt request fails because the runtime is disconnected, or a conversation export fails because the selected destination is unwritable or its drive disappears. The UI neither explains the failure nor gives a clear retry path. A failed Stop is particularly confusing while a turn may still be running.

**Why it happens.** Both Stop handlers discard the `interruptTurn` promise with `void` and no rejection handler. The Export menu similarly discards `exportCurrent`; that async function awaits the dialog/export but never catches failures or sets the controller error. React error boundaries do not convert these asynchronous event-handler rejections into user feedback.

**Proposed fix.** Add shared controller actions for Stop and Export that catch errors, set a visible error/notification, and track pending state. On interrupt failure, refresh/reconcile turn status without falsely claiming cancellation. For export, report the successful destination and preserve the conversation state on failure. Treat dialog cancellation as an ordinary no-op.

**Regression coverage.** Reject both IPC calls in component/controller tests and assert visible errors. Test both Stop entry points, export dialog cancellation, export write failure, and successful retry. Verify there is no unhandled promise rejection.

## Original audit verification and limits

The audit host was **macOS 27.0, arm64**, using **Node 24.20.0**, **Cargo/Rust toolchain 1.97.1**, and **Python 3.14.5**. It was not a clean installer test or a test of the advertised minimum macOS version.

| Check | Result |
| --- | --- |
| Frontend `npm test -- --run` | 113 files, 827 tests passed |
| Frontend `npm run build` | Passed; emitted the existing large-chunk advisory |
| Rust library portion of `cargo test --locked --offline --all-targets` | 1,021 passed, 11 ignored, one sandbox-related failure |
| Exact retry of the failed Rust engine-lock test with required filesystem access | Passed; the initial failure was opening `engine.lock`, not a failed behavior assertion |
| Rust binary/example targets, run separately after the library command stopped | 16 CLI tests passed; remaining targets had zero tests |
| Temporary Rust audit probe against public APIs | Passed its assertions reproducing findings 2, 4, 5, and 6; included an actual metadata write in an isolated temporary project |
| Direct import of `fileLinks.ts` and date-expression probe | Reproduced findings 7, the helper portion of 8, and 9 |
| Default release-test command | Reproduced finding 10: 62 passed, one failed |
| Release tests with `PYTHON=python3` and loopback server access | 63 passed; Pipeline 1.1.0 release identity validated |

Rust's locked dependencies were fetched after the initial network-restricted attempt could not resolve the registry. Frontend tests/build used the pre-existing `node_modules` tree, not a fresh `npm ci`; for example, the running Vitest identified itself as 4.1.10 while the manifest requests 4.1.11. These results are useful local checks, not certification of a clean locked frontend install. The native test results above are reported as separate runs rather than claiming a single uninterrupted all-targets pass.

The audit did not call live model providers, operate on real research data, publish issues, change application implementation, or run Windows/Linux installers. Temporary reproduction source was removed after use. The original audit left native Windows checks pending; the follow-up CI results below now cover the deterministic Windows regressions. No claim is made that every source file or every possible release defect was exhaustively reviewed.

## Fix verification — October 21, 2026

The fixes were checked on macOS arm64 with Node 24.20.0 and Rust 1.97.1. A clean `npm ci --no-audit --no-fund` installed the committed dependency tree, including Vitest 4.1.11. CI remains configured for the repository's pinned Node 24.18.0; that exact Node patch and other native operating systems were not run locally.

| Check | Result |
| --- | --- |
| Full frontend suite after clean install | 116 files, 847 tests passed |
| Final focused navigation/action suite after UNC and feedback refinements | 61 tests passed, including one additional UNC hostname case |
| Production `npm run build` | Passed; existing large-chunk advisory remains |
| `cargo test --locked --offline --all-targets` | 1,030 library tests and 16 CLI tests passed; 11 existing tests ignored; other targets passed with zero tests |
| Final resource-layout/process tests after product-name review | 3 tests passed; Linux fixture uses `usr/lib/Pipeline`, matching Tauri productName |
| `cargo clippy --locked --offline --all-targets --all-features -- -D warnings` | Passed |
| Windows boundary module and its tests, standalone cross-target harness | Type-check and strict Clippy passed for `x86_64-pc-windows-gnu`; no Windows execution claimed |
| Default `npm run test:release` (no PYTHON override) | 67 tests passed; Pipeline 1.1.0 release identity validated |
| Rust/frontend formatting, source-size budget, Markdown links, diff whitespace | Passed |

The native suite needed filesystem access for its existing engine-lock test; release tests needed loopback access for the local mock server. Initial restricted attempts and temporary build failures were corrected before the successful results above. No live provider qualification, installer execution, signing, or publication was performed. Windows symlink regressions run when creation privileges are available; junction regressions require no symlink privilege. OneDrive hydration remains a manual native check.

Research launch fingerprints now include the effective PATH and resolved prefix arguments. Existing authorizations may require preview and explicit authorization again; this is intentional so the newly resolved command/environment is the one the user approves.

## CI follow-up — October 21, 2026

The first [GitHub Actions run for the fixes](https://github.com/mdroste/pipeline/actions/runs/39673818507) passed the macOS native suite (1,030 library tests and 16 CLI tests). Windows exposed a regression-fixture error: after verifying that an open descendant still reads the original file, the test attempted to rename its ancestor while retaining the descendant file and directory handles. Windows rejected that rename with access denied. The fixture now closes those completed-scenario handles before testing root replacement, retaining the original root handle and all security assertions. The corrected commit, `900d8eccf51396261d38e9e90d6981a6f876c21a`, passed all three jobs in the [follow-up CI run](https://github.com/mdroste/pipeline/actions/runs/39674971858). The Windows retained-root regression, public junction-reader checks, and extensionless preview/execution test all passed. OneDrive and installed-artifact checks remain separate release gates.

| CI check | Result |
| --- | --- |
| Linux (`ubuntu-22.04`) | 1,030 Rust library tests and 16 CLI tests passed; 6 ignored |
| macOS (`macos-15`) | 1,030 Rust library tests and 16 CLI tests passed; 11 ignored |
| Windows (`windows-2022`) | 959 Rust library tests and 16 CLI tests passed; 4 ignored |
| Frontend, pinned Node 24.18.0 | 116 files / 848 tests passed; production build passed |
| Release contracts, explicit CI Python | 67 tests passed |
| Strict Rust lint, formatting, source budget, documentation links | Passed |

## Additional release gates to complete

These are evidence gaps, not additional proven runtime bugs:

1. **Complete the candidate qualification manifest.** `docs/releases/1.1.0-qualification.json` still has an empty commit and pending scenario/platform records. Fill it from the final candidate and actual installers, then run the documented qualifier. The current checked-in file is intentionally a template; its existence is not qualification evidence.
2. **Exercise installed artifacts on all four advertised targets.** Test macOS ARM and Intel, Windows, and Linux AppImage on clean supported systems. Include first launch, upgrade/data preservation, bundled PDF extraction through both GUI and CLI, and Windows WebView2 setup. The release workflow builds packages but does not launch them.
3. **Cover the platform cases behind these findings.** Use macOS case-insensitive storage and GUI launch; Windows ordinary, extended, spaced, Unicode, and junction-containing paths; and Linux desktop launch with a minimal inherited PATH. Include unavailable/custom storage drives without altering real user stores.
4. **Run the live qualification scenarios that deterministic tests cannot establish.** Authentication, native model/tool permissions, provider failure recovery, cancellation, and packaged crash recovery remain separate checks. Review the 11 ignored native tests and the existing qualification documents when deciding which require final-candidate evidence.
5. **Complete final artifact/toolchain checks.** Clean-install source checks are recorded above. CI passed with the pinned toolchain for the corrected source commit above. Repeat it if candidate code changes, and finish dependency advisory review, signing/notarization acceptance, and bundled-component notice verification.

All itemized code fixes are implemented. The remaining gates above qualify the actual release candidate and installed packages; passing local source tests is not a substitute for them.
