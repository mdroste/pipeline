# Changelog

Changes in each Pipeline release, newest first. Development before the first
public release was not tracked as a stable release series.

## 1.4.0 — 2026-11-01

- Projects have a **Version control** tab under **Library**. See what changed
  in each file, commit, switch branches, browse history, and undo an unpushed
  commit. Push and pull to GitHub, Bitbucket, or any Git host with your
  computer's Git sign-in, open pull requests, and resolve merge conflicts by
  choosing a version. Pipeline warns about files over 100 MB and sends discarded
  changes to the Trash.
- Pipeline no longer reopens on **Settings** or **Help**, including after an
  in-app update. If you last used either one, it opens **Home**. Your last
  project, conversation, or review still reopens as before.
- **New chat** on **Home** now starts a new conversation. Before, it opened your
  latest conversation, or left the message box greyed out if you had none. The
  conversation page also offers **New conversation** whenever none is open.
- On Mac, **View → Hide Sidebar** folds the sidebar, like the button at its top.
  Control+Command+S does the same.
- The **Get started** guide no longer asks what you work with. It opens on
  **Connections**, and **Next steps** shows the same suggestions to everyone.
- A conversation with no project can now create and change files. It has a
  private working folder that lasts as long as the conversation, and
  **Reveal working files** in its actions menu opens it.
- **Files** now has a folder tree. You can create, rename, move, and add files
  there, send items to the Trash, and search inside files. Renames can be
  undone, and open tabs and drafts follow the file.
- **Files** opens more types: TIFF, Excel, Jupyter notebooks, rendered HTML,
  Word text, audio, and video. Images have zoom and rotation.
- **Files** no longer shows Stata files, which are usually too large to read there.
  They show their size, with **Open externally** and **Reveal in folder**. Saved
  datasets keep their previews.
- Large PDFs, images, and media now open in **Files** without the 32 MB limit.
- Open files update when they change on disk. If you have a draft, Pipeline
  tells you and keeps it.
- **Files** has a cleaner design. One search box replaces the two finders, and
  file actions moved into a **+** button and a menu on each row. **Save** appears
  once you change a file. You can hide the file list, and code colors are easier
  to read in dark mode.
- Code now has the same colors in the editor, previews, and changes, and more of
  it is colored: more Stata commands and macros, C types, and HTML tags. `.htm`
  and `.ltx` files are recognized, and C++ files now have colors in the editor.
- The source editor no longer loses characters when you type quickly. This
  affected **Files** and **Manuscript**.
- Double-clicking a build's PDF, **Find line in PDF**, and error links now work
  when the manuscript builds in a subfolder.
- **Manuscript** is now a writing page. It opens your main TeX file by itself,
  **Build** suggests the settings, and each finished build opens beside the
  source without an extra step.
- Project pages show their main views as tabs, with the rest under **More**.
  Pages open on your work, and setup forms are folded until you need them.
- In the sidebar, the project's name now switches projects and opens
  **Project settings**.
- Dark mode is now neutral gray. Buttons, selected items, and page backgrounds
  no longer have a green tint, and green is kept for status, such as a finished
  step or an installed tool.
- **Open externally** works again for files in a project and in reviews. It
  now declines programs and scripts. Files in excluded folders no longer look
  editable.
- Pipeline stays smoother during long sessions. A streaming response, typing,
  and a running review no longer redraw the whole window, and switching between
  conversations keeps the model list. The window opens sooner with a long review
  history. Long API responses, file open, search, and git refreshes are faster,
  and returning to the app no longer adds to a project's saved data.
- Long conversations open much faster. A conversation now shows its latest 50
  messages, and **Show earlier messages** brings in the rest.
- Returning to Pipeline no longer reads the whole project folder. It now compares
  each file's size and modified time. **Refresh file list** still reads every
  file.
- Research automations and research discovery can send their conversation turns
  again. Before this, every run stopped at its first step.
- Research discovery shows **Needs attention** with a **Retry** button, and
  **Retry** works after a failed turn. Quitting or restarting during a step no
  longer ends the run: reopen Pipeline and choose **Retry**. A run that has ended
  for review shows **Stopped: needs review**.
- Schedules keep running through ordinary project activity and app updates. If a
  conversation's access, research profile, or folder changes, choose **Use
  current conversation settings** on the schedule to accept the change.
- **Review and revise** saves only the assistant's final answer to `paper.md`, not
  its progress messages.
- Opening a folder no longer runs programs named in that folder's own Git
  settings, such as filters, hooks, signature checks, or SSH commands. **Check
  GitHub**, **Check for updates**, and the fetch step of **Pull** ignore those
  settings too. **Commit**, **Push**, and the merge step of **Pull** still use
  them. If a repository needs its own connection setting, such as an SSH command
  for a second account, these checks can fail. The error names the setting, and
  `git fetch` in a terminal still works. They also no longer fetch a remote that's
  a group of remotes or that uses a Git helper program. Use a terminal for those.
- Pipeline refuses to open programs from reviews, conversation links, and **Open
  external editor**, including files marked executable, and points you to the
  file's folder. More Windows and Mac program types are recognized, including
  Python files on a Mac.
- **Load more** on a large text file no longer freezes the app.
- Files you uncheck in **Changes** stay unchecked after you visit **History** or
  choose **Open in Files**, so they can't slip back into a commit.
- Stopping a review now stops parallel reviewers, merges, and model-based PDF
  extraction, including their running processes. Stopping a single running step
  works, usage totals and limits count parallel calls, and these calls appear in
  `run.log`. Two reviews running at once can no longer stop each other's
  processes.
- Folder reviews, including **Replication Package Readiness**, no longer fail at
  launch on large packages, Git repositories, symbolic links, or folders with
  many files. The review reads a private copy, the preview and `check` warn when
  files won't fit, and the log lists everything left out.
- Word documents no longer lose paragraphs after a table with an empty last cell,
  or text after a text box, and text box contents appear once.
- The Reference check now finds bibliographies in Word documents and PDF text,
  and says so when it finds none.
- LaTeX papers with an embedded bibliography, a `.bbl` file, or a `.bib` file that
  can't be fully read no longer get false reports of undefined citations. Large
  and Latin-1 `.bib` files are now read.
- Word comments can be added to earlier reviews after an extraction update, and a
  file that really changed is still refused. An older review with no captured
  copy of the file can still get this refusal.
- Reports no longer lose text between unspaced inequalities such as `if N<T and
  T>0`, in prose or in math.
- Negative values now save correctly in **Manual value** and unit conversions.
- Turning off or removing a step in the review designer no longer leaves hidden
  dependencies that block **Save**. If another step runs once for each item in
  that step's results, the designer says so and asks you to change it first.
- Project views show new command profiles, saved plans, results, and sources when
  you return to them, and your unsaved form entries stay in place.
- In **Review edits** you can accept selected changes from a file more than once.
  The diff shrinks, a confirmation appears beside the button, and finished files
  leave the list. Undo restores what you accepted.
- API conversations with **Internet access for commands** on and **Allowed command
  sites** blank no longer run commands. List the sites you need. ChatGPT and
  Claude Code conversations still treat blank as public internet.
- The conversation browser checks every redirect, so a public page can't send it
  to a private or local address. Links that redirect to another site, such as DOI
  links, open at the real address. Images, scripts, and other parts of a page
  that redirect to a different site don't load.
- ChatGPT conversations keep working after you rename or move their project
  folder.
- Conversations on API or Claude Code connections, and ChatGPT conversations that
  start on a new thread, no longer lose earlier messages after long runs of tool
  activity.
- Anthropic API conversations no longer fail when the assistant uses a tool with
  no inputs, such as listing project files.
- API replies can run much longer. A reply that still hits the model's limit stays
  in the conversation with a clear message.
- Sending a message while another conversation or an automation is running no
  longer hangs. The message stays in the box, and a notice names what's running
  with a button to open it.
- A response waiting for your approval now shows a notice on every page, and the
  Activity row opens that conversation.
- Leaving a conversation just after sending no longer puts the message back in the
  box, so it can't be sent twice. If it wasn't sent, it comes back with an
  explanation.
- Moving or copying your research data folder no longer breaks your stored files.
  Pipeline updates their locations when it opens. If any file is missing, Pipeline
  keeps all your research files and explains why, rather than calling them unused.
  Temporary command files and database backups can still be removed.
- If a saved file is missing, **Export research backup** names what's affected and
  lets you back up everything else. Restoring that backup lists what was left out,
  and those items stay in your projects marked unavailable. A backup that could
  never be restored is no longer written.
- Commands can use input files of any size. Files over the limit keep their hash
  but aren't saved. Declared outputs over 200 MiB are listed as not kept, and the
  run keeps its logs and real result. Input problems now say what's wrong, such as
  not found, a link, or unreadable.
- Restoring a backup refuses any file whose database contains objects Pipeline
  didn't create, so a crafted backup can't keep saved approvals.
- **Files** now opens CSV and TSV files as a table of the first 100 rows,
  exactly as written in the file. **Load more** adds the next 100 below them,
  and large files open at once.
- The table shows the row total only once you reach the end of the file, and
  marks any cell too long to show in full as **Clipped**.
- **Resume** now always runs the review design saved with that run, even if you
  have since selected another. **Regenerate** does the same, and picks up your
  edits only when that design is the one selected now.
- **Resume** and **Regenerate** now work for reviews with a secret variable:
  Pipeline asks for the value again and never saves it. Variables added to a
  review design since the run now use their defaults.
- Long scanned documents with many extraction notes no longer fail at the
  orientation step after three paid attempts.
- Exhausted OpenAI and Gemini credit is now recognized as a usage limit, so
  **Continue after an account usage limit** runs instead of showing "Rate
  limited".
- When the final step fails and a review design names no final step, the report
  now says **Final report unavailable** instead of showing one reviewer's
  analysis.
- Reports no longer lose an opening paragraph that begins with phrases such as
  "Looking at" or "Here are".
- Opening Review history and using **Resume** no longer fail or skip interrupted
  runs when the history is large.
- ChatGPT reviews no longer stop after a few hundred Paper Reviews. Pipeline now
  clears old saved call records on its own.
- The Batch panel now shows a document as failed when any of its steps failed,
  including when all of them did, instead of done.
- A step that runs past its time limit on Claude Code or Codex now says it timed
  out, instead of suggesting the system killed it, and Pipeline no longer starts
  another call in that moment.
- LaTeX files that aren't UTF-8 can now be reviewed. Pipeline reads them as
  Windows-1252 and adds a quality note.
- LaTeX projects over 512 files or 256 MB no longer stop the review. Figures
  past the limit are left out and listed in the quality notes.
- **Stop** and the message box no longer stay locked on a ChatGPT response that
  ended when Pipeline quit or updated. Reopening the conversation marks that
  response as interrupted, or shows how it really ended.
- A ChatGPT conversation whose first message failed can be used again. The next
  message continues on a new thread that carries the conversation's history,
  instead of failing with "no rollout found".
- A ChatGPT conversation now keeps its thread after a reconnect, so the next
  message no longer loses earlier tool output and reasoning.
- API conversations no longer stop after one hour while a tool runs. Long
  research runs keep going, with one hour of model time and four hours of tool
  time per response.
- Claude conversations no longer time out because of long commands. Command time
  now counts toward the four-hour tool allowance, not the 15 minutes of model
  time.
- When Claude can't start, for example after its sign-in expires, Pipeline now
  says **Message not sent** and restores your draft, instead of saying it could
  not confirm delivery.
- A wrong tool call no longer ends an API or Claude response. The assistant sees
  the error and can correct itself.
- **Approval requested** now shows any extra file or network access that **Allow
  once** would grant. **Allow once** is hidden when Pipeline can't describe the
  access or the request doesn't offer it.
- Projects with more than 500 conversations no longer hide the older ones. Use
  **Show older conversations** in the sidebar, and **Search** and
  **Conversations** now cover them all.
- On Windows, a slow WSL start no longer sends Claude conversations down the
  wrong sign-in path. Pipeline waits longer and then asks you to try again.
- **Restore research backup** now moves command folders, run receipts, and saved
  plans with each project you point to a new folder, so commands can be approved
  again and result checks keep working. A project restored without a folder asks
  you to attach one.
- **Project package** imports no longer carry the sender's decisions. Notes and
  claims they accepted and evidence they confirmed arrive as proposals marked
  **Imported**, stay out of future conversations, and don't count as support
  until you accept or confirm them. Imported command runs say they didn't happen
  on your computer.
- **Paper files** replaces **Compiled PDFs** when you share a project, and is on
  by default. Your coauthor gets each paper's own file, so PDF pages, the
  captured file, and Reviews work after import. A file over 64 MB stops the
  export and is named; turn off **Paper files** to send extracted text only.
- **Project package** exports now include every selected source and paper, not
  only the newest 500. A selection over the 5,000-item limit is refused with a
  count of what it holds.
- **Claims & evidence**: comparing results no longer marks stale evidence as
  current, and a comparison must use results from the run the evidence points
  at. A passing check keeps your confirmation; a failing one replaces it so you
  can see it.
- **Claims & evidence** and **Overview**: an accepted claim now counts as
  supported only by a current link you confirmed or a check passed. A model's
  suggestion or a failed check no longer hides the warning.
- **Export research backup** no longer stores a copy of every PDF page the
  assistant reads in the database, and older copies are left out of backups, so
  a long reading history can't push a backup over its size limit.
- **Disk use** now opens however many files a conversation folder holds. Sizes
  it can't finish counting show as "at least", and cleanup still works for
  temporary command files.
- **Export research backup** no longer fails because of conversation working
  files. Files that are unreadable, change during the backup, or don't fit are
  left out and reported.
- **Local jobs**: A running command no longer locks the whole project. Commands
  in the same project now wait in line and run one at a time. Saving,
  annotating, creating tasks, and accepting edits keep working, and only the
  files the job reads or writes are held back, with a message that names the
  job.
- **Accepted file** checks no longer fail while a command is running.
- **latexmk** rebuilds where nothing changed now count as finished, with the PDF
  attached. If a real input change leaves an output unwritten, the run still
  fails, and **Local jobs** names the output.
- **Review edits**: A conversation's old working copy is removed from the
  project folder when the next message replaces it. Copies with edits you
  haven't accepted or reviewed stay, and **Undo** still works.
- **Saving on exFAT and network drives**: Saves, accepted edits, and Undo now
  work on drives without hard links, and they still never overwrite another
  edit.
- **Stored text and datasets**: Pipeline now writes these files in one step, so
  a full disk or a crash can't leave a cut-off copy. A cut-off copy left by an
  earlier version is repaired the next time the same content is imported.
- **Papers**: Importing a file that matches an earlier revision now selects that
  revision, applies the new title and role, and marks evidence on the other
  revisions as out of date.
- **LaTeX folder imports** now read Latin-1 files. Files that can't be read, or
  that don't fit the 16 MB text limit, are named in the import's quality notes,
  and the paper shows as partial instead of complete. Partial papers can still
  be attached.
- **Excel**: Comma and Currency columns are no longer listed as date columns,
  and accounting styles now show their real format.
- **Excel**: In the file viewer, blank rows no longer shift later rows up on
  pages after the 25th row.
- **CSV and TSV**: Imports now work up to the stated limits of 32 MB, one
  million rows, and 2,000 columns, without the hidden two-million-cell cap.
- **Experiments**: A specification grid left with an unknown run after a forced
  quit can now be cancelled. **Continue without this run** keeps that run as
  unknown and pauses at the next specification.
- **Automations:** One automation that can't be saved or read no longer stops
  every other automation. A research automation that outgrows its record limit
  now goes to **Needs attention** on its own.
- **Automations:** A long reply now reaches the next step, even after more than
  100 progress messages. A reply over 512 KiB fails the step with a clear
  message, and the complete reply stays in its conversation.
- **Research automations:** The challenge step no longer stops on a large
  **Additional review** or many sources.
- **Research automations:** A long question in Chinese, Japanese, or Korean no
  longer stops the automation from starting that investigation.
- **Research discovery:** A finished run no longer ends blocked at its last step
  because of an unsent draft or a change in research context in the project
  conversation.
- **Research discovery:** A step that reaches its time limit no longer ends the
  run. Pipeline waits for it to stop, then offers **Retry**.
- **Research discovery:** **Choose projects to develop** now waits as long as
  you need, and **Finish within (hours)** doesn't count that time.
- **Automations:** A step waiting for a free slot in a Side by side automation,
  and a waiting research automation, no longer fill **Activity log** with
  repeated entries or cause "changed; reload" errors.
- **Scheduled automations:** Each scheduled run now keeps the original
  conversation's selected sources.
- **Scheduled automations:** The **Scheduled** tab now shows **Waiting for the
  previous run** with **Open previous run**, instead of a past run time.
- **Scheduled automations:** **Delete schedule** stops future runs and keeps
  past runs in **History**.
- **Automations:** Finished runs now leave **History** 30 days after they end,
  once you've opened them or if they saved no result. Their saved files go too,
  and their scheduled conversations move to the archive. A result you haven't
  opened stays.
- **Wait for input:** Past its deadline, an automation still takes your input
  and continues with a new deadline. **Retry unfinished actions** does the same,
  and the form is hidden when a run can't take input.
- **Research automations:** **Retry interrupted action** no longer charges the
  time Pipeline was closed, so a restart can't use up the budget.
- **Research automations:** **Check saved results and update status** now
  settles a step lost in a restart and says what it found.
- **Research automations:** Retry errors now use plain language.
- **Files** no longer replaces text you type while it checks a changed file.
  Your draft stays, and the tab tells you the file changed on disk.
- **Files** checks folders less often on a busy machine. It stops re-listing
  them while a response is streaming, waits for a slow folder to finish before
  checking again, and no longer ties up conversations and saves on a slow
  network folder.
- A PDF keeps working when a sync client or a branch switch touches it without
  changing it, and you keep your place.
- A tab whose file is gone now closes with a readable message. A file that can't
  be read says so instead of staying on "Opening…". Any retained draft is kept.
- **Move to Trash** on a folder closes every tab inside it.
- Drafts in tabs you haven't opened since restarting now follow a **Rename** or
  **Move**, including when a parent folder moves.
- A file that disappears and comes back, as with a rebuild, a branch switch, or
  a restore from the Trash, is watched again and reloads.
- A CSV or note that starts with "BM", such as a column named `BMI`, no longer
  opens as a broken image.
- **Rename** and **Move** now work for files on exFAT drives and on network
  shares without hard links, and still refuse to replace an existing file.
- On Windows, **Move to Trash** and **Discard changes** refuse on a network or
  removable drive, which has no Recycle Bin, and leave the file in place.
- On Windows, **Open externally** and **Reveal in folder** now work for project
  files, conversation files, and the manuscript's external editor.
- **Open externally** refuses more file types that could run or install
  something, including `.iso`, `.vhd`, and `.inf` files and Windows app
  packages. **Reveal in folder** still works for them.
- **Load more** text from one file no longer appears in the next file, or in a
  newer version of the same file.
- Choosing a found location under **Find between source and PDF** now opens that
  file at that line, so the file menu and the editor agree.
- **Keep this branch's version** and **Keep incoming version** now ask first and
  move your current copy of the file to the Trash, so a hand merge is never
  overwritten by one click.
- **Version control** no longer mistakes new files such as `Final Summary.docx`
  or `Table S1.tex` for submodules, so **Open in Files** and **Discard changes**
  stay available for them.
- **Version control** keeps your commit message when you change tabs or choose
  **Open in Files**. A running push or pull keeps its **Stop** button, and a
  failure that happens while you are away is waiting when you return.
- Choices on **Version control** that change files now wait while a response is
  running, as they do in **Files**, and while a local job is running, naming the
  job. A commit that leaves out the job's files still goes ahead. If a file has
  an unsaved draft, Pipeline asks first, because Git uses the saved copy.
- A repository whose remote is a shared-drive path or another address Pipeline
  can't shorten now shows **Push**, **Pull**, and **Check for updates** instead
  of **Connect remote**.
- When **Publish branch** is refused because the remote already has commits, the
  message now says what you can do: connect an empty repository with **Change
  remote…**, or combine the two from a terminal. A refused **Push** now says to
  check for updates first, so **Pull** appears.
- On a Mac without developer tools, **Version control** now says Git isn't
  installed instead of "Git can't read this folder", and stops raising the
  system install dialog on every refresh.
- **Chat only** layout: file links in a reply, **Open** on a source chip, and
  **Review edits** now bring the project pane into view instead of opening
  behind the chat.
- Opening an empty project during a running response no longer leaves the old
  prompt, partial reply, or "is working…" status on that project's page. The
  response keeps running in its own conversation.
- Returning to a running conversation now shows your message once and the whole
  reply so far, including what arrived while you were away.
- Pipeline now reopens the project, conversation, and project view you were last
  in, even when you moved there from inside the project page.
- A failed model or thinking save no longer stops you leaving the conversation.
  The **Model and thinking** chip goes back to the saved choice, so selecting
  the same model again retries the save.
- Opening a conversation from Home, Search, or **Conversations** can no longer
  leave the sidebar saying "No conversations yet."
- Changing the selected review now shows **Checking system** in the sidebar
  while the check runs, not a false **Setup needed**.
- Importing a paper, accepting a note, creating an outline, or merging records
  from another project now updates **Documents**, **Action items**, and the
  **Overview** straight away.
- **Search & collections** keeps your results when the index changes and tells
  you to search again, instead of clearing the list when you come back to the
  window.
- **Command settings** has a **Time limit (seconds)** field, shows the limit in
  **Review command before running**, and lets you **Edit** a saved profile. New
  commands still default to 300 seconds, and the longest allowed is 7,200.
- **Tables & figures** now accepts a **Python script** profile for figures and
  lists only profiles that start Python, including a project `.venv`
  interpreter.
- **Preview review of selected passages**, the impulse-response **Numeric
  series** list, and **Add exact source** show each passage's quote or series
  name instead of the same label on every row.
- A paper opened from **Search & collections**, a source chip, or the
  **Overview** no longer starts with a stray "Page null region" bar or an old
  passage from another document. Closing a comparison also forgets its position.
- **Open exact passage** in **Result links** now opens the saved passage, and
  citation buttons in **Citation-key navigation** open the manuscript at that
  citation.
- The red project error can now be dismissed. It clears when you start another
  action or change views, and a possible duplicate source now shows as a notice,
  not an error.
- Failed actions in project views show the real message instead of "[object
  Object]", and a failed reload on **Specification grids** can no longer take
  down the project view.
- A refused save in **Theory** and similar long pages now scrolls its message
  into view, and save confirmations appear next to the section you saved.
- **Discard this edit**, **cancel** on a running grid, and **Combine** on a
  finding now ask first and say what is lost.
- Choosing another record in **Response matrix**, **Theory**, **Literature**, or
  **Experiment** now asks before replacing text you haven't saved.
- **Focused re-review** now appears in **Responses** without ticking the
  findings import box.
- **Create action item** keeps you in the reader and offers a link to **Action
  items**. **Propose revision** goes to **Review edits**.
- **Reviews & findings** has a **Track findings** action that starts **Findings
  across reviews** for this project and adds later reviews to it.
- **Add new version** now works from an earlier version, and says why if that
  version's paper is gone.
- **Use this check's results as the Review's findings** no longer adds a format
  the save refuses, so a Pipeline check can publish findings and the review
  designer saves. In **Schemas**, a Pipeline check now reads **Pipeline writes
  this result**, with nothing to edit.
- Choosing **After dependencies (sequential)** for a step with several agents or
  **Run once for each item** now says what will change, keeps the first agent,
  and turns the repeat off, so the review saves. **Run once for each item** is
  unavailable on a sequential step and explains why.
- A long step name now produces a step that saves, and step names stop at 200
  characters. An imported step whose name matches an existing step gets a short
  numbered id.
- **Schemas** now accepts Pipeline's own findings format, so **Use findings
  schema** can be edited and the Grant Proposal Review formats can be changed.
  Reviews that follow Pipeline's current findings format now show **Structure**
  and **Example response**.
- The **Issues** tab now shows **Reviewers:** for Paper, Slide, and Grant
  reviews, and the search box finds a reviewer by name.
- **Evidence** on an issue now shows the web link or DOI of an outside source,
  with its publisher and access date, and **Export all decisions** includes
  them.
- An evidence link to a source file now opens the file at the cited line instead
  of the top.
- **Apply decision** no longer replaces a note: the rationale is added after it.
  It changes only the issues your filters are showing, and it says how many
  changed and how many hidden issues it left alone.
- **Review history** now updates when a review fails or is cancelled, so the row
  stops showing "running" and **Resume** becomes available. Reviews that start
  while the page is open now appear without choosing **Refresh**.
- **Sources** now lists `findings.json` and named products under **Findings and
  products**. Any file in a group Pipeline doesn't recognize appears under
  **Other files** instead of being hidden.
- The prompt editor no longer warns about `{var:…}`, `{input:…}`, and `{item}`,
  or about `{orientation}` and the other placeholders in a parallel step.
  Parallel steps now offer **Insert** chips.
- **Browse…** and **Choose…** for an extra input now follow that input's allowed
  file types, and offer Markdown and text files when none are set, so a rubric
  in `.md` can be picked.
- Issue text and **Compare findings with AI** now show equations and clean
  formatting the way the **Report** tab does.
- Installers now include `THIRD_PARTY_LIBRARIES.txt`, which lists the Rust and
  JavaScript libraries in the app with their licenses and license text.
  **Third-party notices** also name the math fonts, PDF fonts, and PDF decoders
  the app includes.
- The **Privacy** page now describes **Version control**: Pipeline commits,
  merges, and pushes only when you choose to, and **Push** and **Publish
  branch** send your commits to your Git host. It also says when browser and
  command software download.
- The guides now give the real path to **Web search**, **Browser interaction**,
  and command internet settings: **Assistant settings → Manage agent profiles… →
  Access & inheritance**.
- The **Analyze** guide now says that commands the assistant started stop with
  its turn. Only commands you start yourself keep running when you stop the
  assistant.
- The command-line guide now shows step settings and analyzer steps that pass
  `workflow validate`.
- **Reference check:** A reference is reported as retracted only when its
  Crossref record is confirmed to be the cited work. If the record may be a
  different paper or another version, the finding says the retraction could not
  be confirmed.
- **Reference check:** When Crossref lists a journal article and a working paper
  with the same title, the check now picks the one that fits the reference, so
  the cited version's retraction status is the one looked up.
- **Reference check:** The check no longer looks clean when lookups don't
  finish. If Crossref can't be reached, you keep the results that need no
  network, and a finding says which online checks did not run. It also reports
  how many references went unchecked after a partial outage or past the limit,
  along with failed retraction lookups, partial matches, and malformed DOIs,
  including those in Word and PDF bibliographies.
- **Reference check:** Authors with accented names, such as Müller, Sørensen,
  and García, now match Crossref however the bibliography spells them. Style
  commands, macro definitions, and commented-out citations are no longer
  reported as missing citation keys.
- **Replication package check:** **Files read by code** no longer flags files
  found through `../` paths, files the package writes itself, or log and output
  files. If a package is too large to list in full, the check says so instead of
  reporting files as missing.
- **Replication package check:** Random seed warnings no longer appear for
  `e(sample)`, a variable named `sample`, `default_rng(SEED)`, or Julia's
  `seed!(1)`. README keywords match whole words, so "programs" no longer counts
  as a mention of RAM.
- **Replication package check:** LaTeX strings passed to `esttab` are no longer
  reported as backslash paths, and paths inside Stata `/* */` comments are no
  longer reported as hard-coded.
- **Export as HTML** and Word exports: Equations now keep their exact LaTeX
  source in HTML, Word reports, Word comments, and PDF notes.
- **Shareable report:** `report.md` now holds only verified findings, as the
  package says. In **Custom selection…**, the report is labelled **Full report ·
  Includes unverified findings** and asks for confirmation.
- **Settings backup:** Restoring a backup now brings back the default model,
  Review limits, and the other saved settings. API keys and connection modes
  stay as they are on this computer, and settings an older backup never recorded
  are left unchanged.
- **Saved reviews:** A custom review that fails current checks now stays in the
  list, marked **(needs repair)** with the reason. You can open it, fix it,
  export it, and keep it in backups.
- **Saved reviews:** Older review files now read the paper and earlier reports
  as they were designed to, instead of running with nothing to read. A file that
  can't be converted safely is refused with a clear message.
- **History**: A Review that was force-quit before its document was saved, or
  just after its report was saved, no longer stays marked as running forever. It
  now shows as interrupted, so it can be renamed or moved to Trash.
- **History**: Runs are ordered by when they actually started, so a
  daylight-saving or time-zone change no longer puts them out of order. The same
  order decides which runs automatic history limits remove and which earlier
  report a revision comparison uses.
- **Regenerate**: A cancelled or failed Review can now be regenerated while
  another Review is running, instead of failing with "This run predates re-run
  support". Real problems, such as a report over the size limit, now show their
  own message.
- **Resume**: Steps that depend on a skipped step are now reused instead of
  running again at full cost, including after a Review was interrupted.
- **Automatic history limits**: With several Reviews running, these limits no
  longer move a Review that just finished, or a run being re-run from, to Trash.
- **Saved runs**: A very large report or page image that a Review already saved
  is no longer deleted when the Review finishes.
- **Folder Reviews**: An unchanged folder now keeps the same fingerprint from
  one Review to the next, so it is recognized as the same input.
- **Report header**: **LLM**, **Model**, and **Effort** now show what actually
  ran. A Review that used more than one shows "Mixed; see run summary".
- **Supporting files**: When a step's file selection matches more than 500
  files, the step and the log now say the list is incomplete.
- **Multi-agent steps**: If a merge stops unexpectedly, the step now keeps each
  agent's output instead of silently losing them.
- **Revision comparison**: The comparison no longer resubmits a request after an
  uncertain provider result or a sign-in or capability error.
- **Shared context**: The cache warm-up no longer counts as failed when a
  reasoning model reaches its small output limit, so OpenAI Reviews keep their
  prompt cache routing for the whole run.
- The assistant can now read web pages in older encodings such as ISO-8859-1 and
  Windows-1252, which are common on university and statistics sites. These pages
  used to be rejected as binary files.
- Quitting or restarting for an update now stops a running ChatGPT response
  cleanly, so a running command no longer outlasts Pipeline.
- Deleting or moving a conversation while a response runs elsewhere now tells
  you to wait for that response or stop it, instead of mentioning private
  storage.
- An approval is no longer lost when a response produces a burst of activity,
  and the open conversation catches up by itself after live updates are missed.
- A message sent with ChatGPT no longer erases text you typed while it was
  sending, and it no longer returns as a draft that could be sent twice.
- A ChatGPT response no longer waits forever on a request Pipeline can't answer.
  Pipeline now refuses the request at once and the response continues.
- **Sign out** of ChatGPT now also removes the older Conversation and Review
  sign-in files that the shared account was copied from.
- Browser setup in Assistant settings now starts the browser once. If Windows or
  security software blocks it, setup says so instead of reporting ready.
- A Claude message is now saved before its turn starts, so pressing **Stop**
  right after sending can't lose it.
- Slow local models can now answer for as long as their reply keeps arriving. A
  model that goes silent for 10 minutes is now reported as silent, not as a
  dropped connection.
- **Git worktree** task copies no longer fail when your shell prints text at
  startup, such as a greeting or `conda` notes.
- If a **Git worktree** or local task copy can't finish, Pipeline now removes
  the folder and branch it had started.
- CSV files that end in blank lines now import, a file with an uneven row names
  that row, and a file not saved as UTF-8 tells you how to re-save it.
- A failed `.pwex` import into a new project no longer leaves an empty project
  behind.
- Older open import conflicts stay listed, **Keep local** isn't asked again for
  the same two versions, and **Take imported** refuses an imported copy with no
  content.
- A `.pwex` package can no longer store a file under a name that doesn't match
  its contents.
- A command that never started is now recorded as `failed`, not as an unknown
  outcome.
- Follow-up messages marked `attention` now have **Dismiss**, which ends the
  message without resending it and frees its place in the queue.
- **Refresh** no longer fails when a file disappears mid-scan or a subfolder
  can't be read, and missing files leave the list after being reported once.
- Exported transcripts now label replies **Assistant** instead of **ChatGPT**,
  whichever system wrote them.
- A BibTeX field with a stray quote now gives an error naming the field, instead
  of failing with "Database worker ended".
- Comparing a run's results with themselves can no longer pass as a check, and a
  passing check restores a confirmation that a failed check had replaced.
- Staging a publication into a copy with a job running now refuses before
  writing any file.
- If an acceptance of selected changes was interrupted, the message now says to
  choose **Undo** under **Accepted edits**, instead of reporting an external
  change.
- **Maximum reviews** now gets enough actions for every review you allow, so
  **Review and revise** reaches the review limit you set instead of stopping
  early for attention. The automation guide now gives the correct **Action
  limit** of 1,024.
- Research discovery and research automation exports, new project names, and
  error messages now use the names shown on screen. Exports save as
  `research-discovery` and `research-automation`.
- After **Check saved results and update status** recovers a delivered result,
  the next conversation step runs without asking for **Use current conversation
  context**, unless the conversation's settings really changed.
- A step that runs out of time now says "This step reached its time limit"
  rather than reporting that it was stopped.
- Evidence previews now show text for files over 128 KiB that were cut inside a
  character, such as Chinese or Japanese text.
- Research discovery can keep a longer paper history by saving older manuscript
  versions in their files. **Earlier manuscript versions** and the ZIP export
  still show them in full.
- **Stop** now works on an automation that has reached its context budget.
- Automations no longer loses its list when the run you last had open was
  cleaned up. The page shows the list without that run.
- A Review used by an automation is no longer held back from cleanup if Pipeline
  quit just after saving the step, or if history cleanup ran while another
  Review was in progress.
- **Version control**: A push that fails because of a connection timeout now
  says Pipeline could not reach the remote, instead of saying Git took too long.
  A hook that prints unusual characters no longer hides Git's error message.
- **Version control**: A file whose only change is its permissions, or a rename
  with unchanged contents, now says so instead of showing "No changes to show."
- **Files**: Adding a file that already exists, or choosing **New file** with a
  name in use, now says "Already exists" and names the path.
- **Files**: Renaming a file to change only the case of an accented first
  letter, such as `École.tex` to `école.tex`, now works on Mac and Windows.
- **Files**: A folder with more than 5,000 items now lists every subfolder
  first, and an open subfolder shows "First 5,000 items". The notice now
  explains what is missing and no longer suggests a filter that couldn't reach
  it.
- **Files**: `.gitignore`, `README`, `Makefile`, and similar common files can
  now be edited, as can `.htm` and `.ltx` files.
- **Files**: **Load more** no longer repeats or skips text in a large file,
  including files that aren't UTF-8.
- **Reviews**: Images in a Review's rendered Markdown no longer push an open PDF
  out of its viewer, so its pages keep loading.
- **Reviews**: Bare `\input table1` lines in a LaTeX project are now read and
  warned about like `\input{table1}`, and images placed with `\includegraphics*`
  now stay linked to their figures.
- **Reviews**: Word comments and PDF notes no longer show raw Markdown such as
  `**bold**` for structured findings.
- **Reviews**: **Print** no longer stalls on a report with a dollar sign
  followed later by a backslash and an accented character.
- **Reviews**: Saved reports now get a file's usual permissions, so colleagues
  can open them in a shared folder, and a report that replaces another keeps
  that file's permissions.
- **Reviews**: An unreadable configuration from a very old version no longer
  stops Pipeline from starting, and it is kept in the profiles archive instead
  of being deleted.
- **Show earlier messages**: a conversation whose newest saved page holds only
  tool activity no longer shows "What are you working on?" above older messages.
  Pipeline reads back until it reaches a message.
- A conversation that could not be refreshed now keeps "Could not refresh the
  conversation. Showing the saved transcript" on screen, and "Could not load the
  conversation list" no longer vanishes. An error now leaves with the
  conversation it belonged to.
- Opening a conversation while a response finishes in the one you left no longer
  leaves the new conversation blank.
- A message reopened just as it was acknowledged no longer returns to the
  message box, where it could be sent twice. A send that ended without reaching
  the assistant keeps the message box locked until your saved text can be read
  back. If it can't, Pipeline says so, and opening the conversation again
  returns it.
- Opening a conversation from Search, **Conversations**, or the sidebar no
  longer resets that project's tab or changes the layout of the project you
  left. A conversation hidden by **Project only** now opens in view.
- Opening an archived or older conversation in another project now keeps that
  conversation on screen instead of switching to the first one in the list.
- The sidebar's **Conversations** section now keeps up with new conversations,
  titles, renames, and deletions. Opening one that is gone shows a message and
  refreshes the list.
- **New conversation**: answering **Cancel** when asked to discard unsaved work
  no longer leaves an empty conversation behind.
- The message box now keeps your cursor through a response, and takes it when a
  conversation opens, so you can keep typing without clicking back.
- Footnote links now go to the note in their own message.
- If one source can't be read, the other source chips keep their names, and that
  chip says "(could not be read)" in place of an error each time the
  conversation opens.
- Assistant questions that allow a free-text answer now offer **Other…** with a
  field to type in. **Submit** stays unavailable until every question has an
  answer.
- **Move to project…** now holds keyboard focus inside the dialog and returns it
  when the dialog closes.
- Cmd/Ctrl+K no longer opens Search behind a confirmation. Pressing Escape to
  cancel an input-method composition no longer closes the dialog it is typed in.
- Failed file links and **Conversation titles** saves now show the actual reason
  instead of "[object Object]".
- Review settings keep autosaving after a failed save: changing a value back and
  making the same edit again now saves it, and the sidebar no longer stays on
  **Saving…**.
- ChatGPT sign-in, **Sign out**, and **Models & usage** errors stay on screen
  until you try another action instead of vanishing within five seconds.
- The **Reviews connection** settings search result is hidden when OpenAI uses
  an API key, since that control only exists with a subscription.
- Moving to another Settings section through a link or the sidebar while an API
  key is half typed now asks first, and stays put if you decline.
- The review console now shrinks with the window and always leaves room for the
  review above it.
- Long dropdown lists open with the current choice in view.
- Help popups inside subsection headings no longer appear in capitals.
- After one finished report fails to display, other finished reports open
  normally without needing **Try Again**.
- When something you are tracking changes, Pipeline now shows a notification
  that names the project, with **View changes**, instead of a card that covered
  the message box until dismissed.
- **Install update**, **Restart now**, **Show earlier messages**, and the newer
  messages button are readable in dark mode.
- Long file paths now wrap inside notifications and dialogs, and a notification
  stays while your pointer or keyboard focus is on it.
- Conversation labels now say **New conversation**, **Conversation only**, and
  **Project + conversation**, and the project header reads "1 request needs
  attention".
- **Dependencies** now talks about running a review rather than "this workflow".
- Deleting a conversation now warns about "provider history" rather than
  "ChatGPT history", which was wrong for Claude and API conversations.
- The context view now points to **+ (Add to message)**, and **Recent
  responses** shows plain states and readable times.
- **Help** now uses the standard page layout, with a page title and a short
  description.
- **Escape** in a field on a project page no longer jumps to the Overview.
- Links such as **Next: …** on Projects now land on the page they name, even if
  a research item was left open there.
- **Other review collections** buttons open the collection they name.
- **More results** keeps using the search that produced the list, and **Papers &
  sources** clears search hits when you choose another paper.
- **Export** on the response letter disappears once a saved reply, the
  selection, or the format changes, so an old preview can't be exported.
- **Save link** now records the sources you chose even if **Refresh sources and
  impact** reorders the list, and it won't save a source that has left the
  conversation.
- Dataset and source titles taken from a file on Windows now use the file name,
  not the full path.
- Search no longer keeps checking the index while its page is hidden. A failed
  check now says so and tries again.
- Row actions such as **Edit / checks**, **Revise definition**, **Edit as new
  version**, **Preview local rows** and **Review exact plan** now bring the form
  or preview into view.
- **Next source segment** opens each segment at the top and stops at the end of
  the source.
- **Review collections** starts each collection at the top, and a long **Trash**
  list scrolls on its own.
- Specification grids have **Remove factor**, plainly named controls, and only
  the controls that apply to the grid's state.
- **Reading inbox** shows how many items are hidden and adds **Show older**.
- Disabled buttons in **Saved analysis plans**, **Import a data dictionary** and
  **Create review brief** now say what they need, and the dictionary import has
  its own title field.
- The toolchain list under **Local commands** no longer offers a personal shell
  alias. Choose Stata and enter its command under **Advanced argv**.
- The **Drafts & templates** hint now points to **Open externally** under
  **Files**.
- A source, document or project that fails to open now says so and offers **Try
  again**. **Findings across reviews** no longer claims there is nothing to show
  after a failed sync, and filters reset for each collection.
- Pages now use plain labels for stored values such as note types, evidence
  checks and source types, and names such as **Enable importing review
  findings**, **Draft review** and **All review types**. A queued run is no
  longer coloured as a failure.
- Research panel fields now have names that screen readers can announce.
- Warnings and errors that were hard to read in dark mode now use the theme's
  colours.
- A failed job or build list now shows its message instead of "[object Object]".
- **Sources**: An evidence link to a page with no saved image now says "Page 412
  has no saved image." instead of opening an empty panel. When a review saved no
  page images, its page citations are plain text, and a saved report opened
  without a run says when a cited page is missing from the extracted text.
- **Sources**: An evidence link now shows the cited page or file even if you
  left **Open in file tabs** on.
- **Issues**: Issues with no severity, or an unusual one, now have an
  **Unrated** filter. When filters hide everything, the list says "No issues
  match these filters."
- **Issues** and **Compare reports**: Wide tables and long links stay inside
  their cards instead of pushing the page sideways.
- **Compare reports**: **Export diff…** and **Compare findings with AI** are off
  when a report could not be opened, and **Export diff…** is off when neither
  report has any output. A step output that contains a code block now exports as
  one intact block.
- **Report details**: The field that showed when a run began is now labelled
  **Started**. It was labelled "Completed".
- **Export**: The **Export** menu and the custom selection close on Escape or a
  click elsewhere, and the menu works with arrow keys. The package result floats
  over the report and has a dismiss button, so it no longer takes height from
  the header.
- **Report**: A jump from **Contents** or an issue no longer lands under the
  text-size toolbar. **Show failure details** now scrolls instead of pushing the
  report off the screen. Long text wraps inside the report.
- **Sources**: In a report with no saved run, the source list scrolls and each
  source opens at the top.
- **CSV previews**: A table that cannot be shown, such as one with 60 or more
  columns or a stray quote, now shows its text under the notice.
- **Progress**: A stage in which a reviewer failed now shows as failed, not with
  a green check, and screen readers announce it.
- **Report options**: A long form now scrolls, so the title and **Continue**
  stay on screen.
- **Designer**: Clearing a step's JSON schema no longer switches the step to
  Markdown or stops its findings. Use **Return to Markdown output** for that.
- **Designer**: The provider schema preview disappears when you edit the schema,
  and a slow preview can no longer appear under another step.
- **Designer**: Values in **Run only if…** and **Max items** are now read when
  you leave the field, so typing `1.05` or retyping `50` works. A quoted value
  such as `"5"` stays text.
- **Designer**: Discarded edits are no longer offered back on your next visit,
  and switching to a review with a saved draft now offers that draft instead of
  deleting it.
- **Designer**: The overview no longer counts Pipeline checks as provider calls,
  and a step that repeats over an earlier result now names that step.
- **Designer**: In dark mode, section headers no longer show through each other,
  and the **Export** list and bottom buttons are readable.
- **Reviews**: The app now uses one vocabulary for saved reviews, in the
  designer, progress, and export messages. **Start review** is the button in
  both the sidebar and the preview, and the Export menu's archive is the
  **complete archive** in every message.
- **Specialist findings**: The readable form of a specialist's findings now
  keeps cited URLs and DOIs.
- **`pipeline-cli run`** exits with code 2 for a missing input, an unknown
  profile, or an undeclared `--var`, as **`check`** does, so a script can tell
  bad input from a failed run. **`profiles show`** also exits 2 for an unknown
  profile.
- `pipeline-cli --help` and `pipeline-cli help batch` say runs are saved in your
  research data folder, not always in `~/.pipeline/`.
- **Help** explains that **Start review** opens the preview, and that you choose
  **Start review** again to begin.
- **Overview** and the project file list: a folder that can't be read, such as
  an offline cloud folder or an unmounted drive, now keeps its files as they
  were and is reported as not read. Its files no longer show as missing, then
  changed, then added.
- **Check saved results and update status**: a recovered result now continues
  the automation only if nothing else changed while the app was closed. If
  research settings changed too, the next step asks you to review the new
  context.
- Word comments and PDF notes keep text in angle brackets, such as `<treated>`
  or `n<N>`, in a finding's problem, consequence, and recommended action.
- The review progress view keeps an earlier failed reviewer marked as failed
  after you stop the run later on, and only the stage you stopped reads
  cancelled. Screen readers now hear every failed stage.
- The assistant is told when a web page's text may not be exact. A page that
  names no encoding stays readable when it contains one bad byte, instead of
  having its accented characters garbled.
- Model discovery stops after 15 seconds when a provider sends its model list
  very slowly.
- File search in a project folder now stops at its time limit even inside one
  very large folder, and the search result shows it was cut short. Listing a
  very large folder on a slow drive also stops looking for more folders after a
  few seconds and shows **First 5,000 items**.
- Browser setup on Linux now says when the system does not allow the browser's
  sandbox, instead of pointing at security software.

## 1.3.2 — 2026-10-31

- The sidebar is simpler. **Home**, **Conversations**, **Projects**, **Reviews**,
  and **Automations** now sit in one list, and each opens what it holds when you
  need it. **History** and **Designer** moved inside **Reviews**. One **New**
  button starts a conversation, review, project, or automation.
- A project's areas and conversations now appear in the sidebar when you open
  it. They replace the panel behind the project name. **Switch project** moves
  to another project.
- **Activity** at the bottom of the sidebar collects running and queued reviews,
  conversations waiting for approval, and automation updates.
- **Conversations** is a new page that lists every conversation, with or without
  a project.
- Command/Ctrl+K and **Search** now find projects, conversations, reviews,
  automations, and project views from anywhere in the app.
- The sidebar folds to icons instead of hiding, and does so by itself in
  narrower windows.
- **Settings → Connections** now has one **Default model**: a provider, model,
  and thinking level for reviews and new conversations. It replaces **Preferred
  provider**. Each review stage follows it, and a stage you changed can go back
  to it. A new conversation copies it when it starts, so existing conversations
  keep their provider. Until you choose a default, new conversations start on
  ChatGPT as before.
- A usage-limit backup that isn't connected no longer stops reviews from
  starting. The run preview warns you instead, and Pipeline now suggests a
  connected provider when you turn the backup on.
- Provider menus and buttons show which connections aren't ready, so you can see
  the problem before a review won't start.
- **New review** now shows the providers a workflow will really use, including
  when it sets its own or its steps differ, and it hides **Parallel agents** for
  workflows with no parallel steps. Model menus there stay readable instead of
  shrinking to "Prov". The run preview names each provider instead of saying
  "default provider".
- The review designer shows ChatGPT and Gemini by name, and Review settings show
  a saved model role by its real name.
- Settings is tighter and quieter. Rows take less height, **Connections** shows
  each provider's status beside its name, and API keys save from one line.
  **Reviews** execution settings now use the same rows as the rest of Settings.
- The layout model for the PaddleOCR-VL Full Parser now downloads from Hugging
  Face, which fixes installs that crawled at a few dozen KB/s. If Hugging Face
  can't supply it, Pipeline uses the same model from PaddlePaddle's own server.
- **Projects**, **Activity**, and the workflow gallery now look like
  **Automations**, with the same heading, toolbar, and rows. Automations no
  longer shows "PIPELINE" above its title, because the sidebar already shows the
  name.
- Switching between light and dark now fades the whole window at once. Text used
  to change at different speeds and could flicker.
- Settings now uses plainer wording throughout, and storage categories have
  names you can read. Several controls were renamed: **Simultaneous Reviews** is
  now **Reviews at once**, **Review cleanup…** is **Preview cleanup…**,
  **Automatic revision reconciliation** is **Compare with the previous report**,
  and **Restore archive** is **Restore research backup**. Settings search still
  finds the old names.
- On Windows, choosing **Subscription** or **API** under **Connection mode** in
  Settings no longer pushes the window contents up and leaves an empty space
  below them.

## 1.3.1 — 2026-10-31

- The PaddleOCR-VL Full Parser installs on Windows with Smart App Control on, as
  long as Windows allows its Python tools. New Windows installs read pages with
  PaddlePaddle directly and no longer download `llama.cpp`. This runs on the
  processor, so it's slower and needs about 6 GB of free memory, or about 10 GB
  for a few seconds as it starts. Long papers get more time to finish, and
  existing Windows installs keep working as they are.
- On Windows, the separate title bar is gone. The sidebar reaches the top of the
  window, and the minimize, maximize, and close buttons sit in the top-right
  corner. Drag the top of the window to move it.
- A button at the top of the sidebar hides it, so a page can use the full width
  of the window. A dot on the button shows when something is running or needs
  your attention. Pipeline remembers your choice.
- **See an example review** now opens the real report screen on a finished
  review of an invented paper, with the report, report details, issues, and each
  reviewer's output. Its paper and comments are rewritten, and page links in its
  evidence open the cited page.
- The text of each issue is easier to read. It was shown in pale gray.
- Issues name their reviewers in plain words and have **Accept**, **Reject**,
  and **Done** buttons in place of symbols.
- A report is no longer called "degraded" because of a misquotation or a retry
  that succeeded. It shows as **Completed report**, with a note such as "2 quotes
  need checking". **Completed with limitations** now means that part of the
  document may be incomplete. Review history and exports use the same wording,
  and saved reports keep the text they were written with.
- Automations no longer run extra reviews or revisions because of a misquotation
  or a retry that succeeded. A review still counts as incomplete if part of the
  document may be missing or a comment needs manual review.
- The macOS and Windows installers have a new look. On macOS, the disk image
  opens straight to a window showing where to drag Pipeline, with no license
  agreement first. On Windows, the installer uses the system font, shows the
  Pipeline logo, and has fewer steps.
- Pipeline checks for updates every time it starts, rather than once a day. To
  turn this off, clear **Check automatically** in the update banner or use the
  setting under **Advanced & About**.
- **Install update** now installs a new version from inside Pipeline. It
  downloads while you keep working, waits for you to choose **Restart now**, and
  warns you first if restarting would stop running work. Versions before 1.3.1
  need to download the installer one last time.

## 1.3.0 — 2026-10-30

- Installing the PaddleOCR-VL Full Parser works again. In 1.2.5 and 1.2.6 the
  install stopped with `HTTP 302 Found`.
- Engine installs show each step, a progress bar, the download size, speed, and
  time left. The installer log moved behind **Show details**.
- Engine downloads survive dropped connections: they reconnect and continue from
  the last byte received. If one source for Python tools fails, they use the
  other.
- PDF Reviews keep reference lists as structured entries. The PaddleOCR-VL Full
  Parser previously lost them during layout analysis, so reference pages fell
  back to plain page text, and entries on the page where the list began could
  be dropped without warning. Reviews extract these PDFs again automatically.
- PDF Reviews no longer read made-up numbers from plots. When the PaddleOCR-VL
  Full Parser mistook a plot for a picture, it could write out a table of values
  the plot does not print, and Reviews treated that table as text from the paper.
  That output now stays out of the document, and quotes from it no longer verify.
  The figure keeps its image, with a note to read it from there. Text in real
  pictures, such as a scanned press release, is unchanged, though a picture of a
  real table is occasionally treated the same way. Reviews extract these PDFs
  again automatically.
- Pages that hold only figures keep their chart images instead of being replaced
  with rearranged label text.
- Text between math inequalities such as `ρ < 0` and `ρ > 0` is no longer removed
  from extracted blocks.
- The PaddleOCR-VL full parser keeps more captions, table notes, and equation
  labels. Its layout confidence threshold now defaults to 0.30, and a saved
  0.50 moves to 0.30 once. Other saved values stay as they are, and you can
  choose 0.50 again in Settings. Each paper is parsed again on its next run.
- Reviews now warn when a page without figures recognizes less than 80 percent of
  the text in the PDF.
- **Quality and limitations** lists each extraction note once.
- Evidence marked **Not checkable** no longer makes a Review Degraded, and
  **Quality and limitations** counts it separately.
- A quote from an outside source is marked **Not checkable** instead of **Quote
  not found** when its evidence names a URL or DOI.

## 1.2.6 — 2026-10-30

- Conversation command and browser setup uses a managed runtime, with automatic
  downloads, progress, cancellation, and retry. A separate Node.js installation
  is not required.
- Edit conversations prepare working copies automatically and collect changes
  for **Review edits**. Accept, discard, conflict checks, and undo remain
  available. **Direct edits** supports configured research commands and projects
  that are too large to copy.
- Keep the README title unversioned while checking the version badge against
  the application version.
- Clarify conversation file editing, internet settings, and command setup in
  the README and connection guide.

## 1.2.5 — 2026-10-30

- ChatGPT keeps recent conversation history when settings, project moves, or
  recovery require a new native thread. Research automations recognize final
  structured answers from Claude Code and API connections after tool updates.
- Automations require refreshed context after a connection change and stop
  before sending a prompt when cancelled during setup.
- Web page reads preserve paragraphs, lists, and table columns. Release
  qualification binds evidence to the candidate version and each platform's
  installer name and checksum.

- Connection names and model information are shared across reviews and
  conversations. Gemini review steps apply the selected thinking level,
  and Anthropic review steps use the model's listed thinking options.

- Declaude reviews formulaic prose with one to three topic reviewers and
  produces a Markdown report with original passages, suggested replacements,
  and explanations suited to the document's audience and style.

- Reviews support web search and public page reads through OpenAI API and
  local/custom servers with tool-capable models. Search follows each step's
  access setting, time limit, and cancellation.

- Conversations support OpenAI, Anthropic, and Gemini API keys, plus local or
  custom OpenAI-compatible servers. Choose the connection and model in the
  conversation picker; enabled project and research tools use the same access
  settings as subscription conversations.
- API conversations stream replies, offer model-specific thinking controls,
  and support web search and an optional sandboxed command engine on macOS
  and Linux. Automations keep saved model choices, and discovery authors can
  edit their isolated paper folders through project file tools.

## 1.2.2 — 2026-10-29

- Correct Windows release verification for installation paths with mixed separators.

## 1.2.1 — 2026-10-29

- Windows release builds sign the application and installer with Azure Artifact
  Signing and verify the installed files before uploading the release.

## 1.2.0 — 2026-10-29

### Added

- ChatGPT and Claude conversations share recoverable file creation, public page
  reads and downloads, and optional browser interaction. Assistant settings
  separately control web search, command internet access, allowed command sites,
  and browser interaction. Commands can write inside isolated working copies.
- Claude can use the command sandbox inside a configured WSL2 distribution on
  Windows. Usage reports for both providers are saved per account and show
  their reporting time and separate usage windows.

- Reviews check every quoted passage against your document or the cited source
  file and label it **Quote found**, **Partial match**, **Quote not found**, or
  **Not checkable**. The quality summary counts each label. **Quote found**
  means the passage is where the comment says; whether the comment is right is
  still your call.
- The review designer can add **Pipeline checks**, steps Pipeline runs itself
  without a model. **Reference check** reports undefined citation keys,
  duplicates, and malformed DOIs. With lookups on, it checks each reference in
  Crossref for missing records, mismatched details, other versions,
  retractions, expressions of concern, and corrections. **Replication package
  check** reads a package folder against data-editor requirements without
  running it. The gallery templates **Reference Check** and **Replication
  Package Readiness** use these checks, and older versions of Pipeline refuse
  workflow files that contain them.
- The review preview estimates model calls, tokens, and cost at API list
  prices. **Settings → Reviews → Review limits** caps cost, calls, or time per
  review. At a limit, running steps finish, later steps are marked stopped, and
  the partial report is saved. The CLI's `run` and `batch` accept `--max-cost`,
  `--max-calls`, and `--max-minutes`, and `check` prints the estimate.
- Export a review as an offline HTML file or a Word document. You can also
  export a copy of the reviewed `.docx` or PDF with each finding added as a
  comment or highlight on the passage it quotes. The original file is
  unchanged, and findings that can't be placed are listed separately.
- Local checks can run Stata do-files with an installed Stata on macOS,
  Windows, and Linux, and a shell alias such as `stata` still works. New check
  types for R, Python, Julia, Jupyter notebooks, and Quarto or R Markdown
  record the runtime version and lockfile hashes and leave notebooks unchanged.
  **Detect installed tools** finds Stata, R, Python, Julia, Quarto, Jupyter,
  and MATLAB without installing anything or starting licensed software.
- **Data & samples** imports Stata `.dta` files (Stata 8 and later) and Excel
  `.xlsx` worksheets with their variable labels, value labels, and formats.
  Stata extended missing values stay separate codes, and Stata files up to 2 GB
  can be catalogued without keeping a copy.
- New downloads from DBnomics (IMF, OECD, Eurostat, ECB, central banks, and
  more) and the World Bank need no key and record each dataset's license,
  required notice, and suggested citation. FRED data shows the notice its API
  terms require, with a link to the terms.
- A **Get started** guide opens on a new installation. It shows which model
  connections are ready, checks this computer for Stata, R, Python, Jupyter,
  Julia, Quarto, and MATLAB, and suggests a first step for your kind of work.
  It changes no settings; reopen it from Home or **Settings → General → Guided
  setup**.
- **See an example Review** shows sample findings, evidence, and quote checks
  on a fictional paper, without calling a model or adding to review history.
- An optional **Crossref contact email** in the **Reading inbox** sends your
  own address with literature lookups, never a shared one.
- New built-in **Slide Review** (`slide-review`) uses Paper Review's reviewers
  on a slide deck exported as PDF and cites slides by page number. **Paper
  Review** also recognizes a deck and applies the same guidance.
- Project conversations can use **Claude Code** with your own Claude
  subscription, chosen per conversation in the **Model and thinking** picker.
  Existing conversations stay on ChatGPT. The picker shows your latest Claude
  usage, and new Claude conversations are titled automatically.
- Claude can use the project context, the research tools you turned on, and
  the project's files. Edits need the **Allow edits** access mode. On macOS and Linux
  it can run shell commands in a sandbox that reads the project but can't
  change it or reach the network.
- On Windows, projects can use working copies, and you can accept edits in
  **Review edits**.

### Changed

- Updated dependencies used for document rendering, downloads, encrypted
  connections, and archive handling to address security advisories.

- Automatic Paper Review (Full) is now **Paper Review** (`auto-paper-review`),
  and Automatic Paper Review (Quick) is now **Paper Review (Fast)**
  (`auto-paper-review-fast`). Your copies and edits move over on first launch
  and keep any custom name. Automations and scripts that use `auto-review` or
  `auto-review-quick` keep working.
- Reviews queue instead of blocking. **Settings → Reviews → Simultaneous
  Reviews** sets how many run at once (default 2, at most 4). The sidebar lists
  running, queued, and recent reviews with their stage and elapsed time, and a
  footer indicator shows active reviews from any page.
- Selecting several documents in **New review**, or re-running from
  **History**, queues one review each. Each review keeps its own log and token
  totals, and cancelling one never stops another. Batches, automations, and
  history purges still run alone.
- Pipeline checks for updates about once a day while it runs, not only at
  startup, and the banner's **Download** button opens the installer for your
  computer. Turn this off with **Check for updates automatically** in
  **Settings → Advanced & About**. If GitHub limits a shared network, Pipeline
  waits as long as GitHub asks.
- The review designer's workflow graph runs top to bottom, frames parallel
  stages together, and wraps wide stages onto new rows.
- Labels, help text, and messages are clearer throughout the app. Saved
  settings and behavior are unchanged.
- The README and public guides are rewritten in plain language. The privacy
  and security pages cover the shared ChatGPT sign-in, the recoverable Trash,
  and current release checks.

### Fixed

- In the review designer, **Restore adaptive router prompt & schema** restores
  the open built-in's own version. It no longer removes **Paper Review
  (Fast)**'s limit of two method reviewers, and **Slide Review** keeps its
  slide-deck setup.
- The automation builder's **Action limit** goes up to 1,024 actions, the
  limit Pipeline accepts when it checks an automation. It previously stopped
  at 64.

### Security

- Before Claude subscription calls, Pipeline confirms that Claude Code is
  signed in to a Claude subscription: once per review and before each
  conversation turn. These calls ignore API keys inherited from your
  environment and never fall back to a saved API key. API mode stays a separate
  choice for reviews.

## 1.1.1 — 2026-10-22

### Added

- Reviews accept Markdown and plain-text files, including drafts written by
  research automations.

### Changed

- Permission requests in conversations show the full access requested, the
  working folder, and how long the permission lasts.

### Fixed

- Failed approvals refresh pending requests and can't be answered twice.
  Discarding an unreadable API key keeps unsaved settings, and a late new
  conversation no longer overrides where you navigated.
- Feedback validation keeps findings past its call limit. Merges keep every
  original finding and its decision, even after a usage-limit fallback.
- Cancelling an experiment as it starts works reliably, and recovery records
  the exact plan or settings that were tested. Replication packages keep
  executable permissions without letting files run.
- A failed computation can't complete a research automation, and a failed
  automation turn can be retried instead of being marked unknown.
- Conversations no longer hang on shutdown or while answering a permission
  request. `pipeline-cli` exits with an error when it can't write its output,
  but stays quiet when the output pipe closes.
- LaTeX extraction follows `\subfile`, `\import`, and includes with extra
  spaces, and attaches figures only when a label or page matches. Word
  extraction keeps tabs, breaks, special characters, and whole equations, and
  notes any equation too large to keep.
- The installed `pipeline-cli` finds its bundled PDF tools. Research commands
  find programs on your PATH consistently, including Windows `.exe` LaTeX
  engines.
- Project Markdown previews show images and follow absolute links within the
  project folder, including on Windows.
- Automation deadlines use the chosen date's daylight-saving offset and reject
  local times that don't exist.
- Both conversation **Stop** controls and **Export conversation** report
  failures and allow a retry. A finished export shows where it was saved.

### Security

- Prompt placeholders are filled once, so placeholder text quoted in a document
  can't reveal other inputs or variables.
- Excluded sources stay hidden from the assistant's tools, including cached
  reads.
- On Windows, project file reads can't escape the opened folder through
  junctions. Mixed-case names and Windows path aliases can't bypass file-access
  rules or excluded folders.

## 1.1.0 — 2026-10-15

### Changed

- Automation step IDs can't be `done`, `enter`, or a plain number, which caused
  steps to be skipped. Upgraded installations get the current Grant Review
  suite.

### Fixed

- A message rejected after sending returns to the composer instead of being
  erased. Reconnected conversations keep their message order, keep up with
  automation turns, and unlock the composer when the connection drops.
- With a custom data folder, opening History or a project during a review no
  longer marks it interrupted or lets it be deleted. Restoring a review from
  Trash works on Windows default storage and symlinked data folders.
- Re-running or resuming a review whose source is over 50 MB or 500 files no
  longer fails its integrity check.
- Parallel review steps on ChatGPT recover when the connection lags during a
  tool call. Steps that continue a Claude CLI session can write supporting
  files again.
- Quality reports count only real retries, so steps that succeed first time
  aren't labeled degraded.
- An invalid built-in workflow no longer blocks every launch, and resetting it
  repairs it. Settings backups from 0.9.x import again, and an API key that
  can't be decrypted can be discarded in Settings.
- Staged LaTeX projects keep figures found through `\graphicspath` or in
  chapter files.
- Cancelling PaddleOCR-VL verification no longer blocks the next run, and
  reinstalling a current parser no longer rebuilds it.
- Importing the same Zotero item or findings into a second project no longer
  overwrites the first project's record. Items imported by 1.0.2 and earlier
  aren't duplicated on re-import.
- Commands that start `/bin/zsh`, including Stata, work when `~/.zshrc` or
  `pyvenv.cfg` is a symlink.
- Saving a file in a folder that task capture skips, such as `node_modules`, is
  refused instead of deleting the original on acceptance.
- In Sources, clicking another page after opening a citation no longer jumps
  back.
- Research discovery runs ask for attention and offer **Retry** instead of
  stopping as blocked when a conversation or setting changes or one turn fails.
- Research automation retries work after you steer the planner. Long check
  instructions and `{{input:…}}` text in research content no longer block
  results.
- A review that finished while its automation lost track of it can be
  reconciled instead of re-run. Automations waiting for a slot log the wait
  once, not every two seconds.
- The manuscript reader counts the last PDF page, and plain editor saves no
  longer leave a task and a copy folder behind.
- Project packages (`.pwex`) re-import without false evidence conflicts and
  include every selected note and retired claim.
- Paper and project search handle single-line files and punctuation-only query
  words.
- `pipeline-cli` exits cleanly on a closed pipe, accepts a bare `.tex` name,
  and resolves relative input paths so the app can file those runs under a
  project.

### Security

- Research backups (`.pwrx`) and project packages (`.pwex`) reject entries
  whose stated size doesn't match, so a crafted file can't fill the disk or
  memory. Restores reject references outside the research store.
- Steps that continue a Codex CLI session set their own sandbox and approval
  policy instead of inheriting your global Codex settings.
- The file viewer no longer offers to open programs written during a review,
  such as `.exe` or `.dmg` files.

## 1.0.2 — 2026-10-08

First stable release, with installers for macOS (Apple Silicon and Intel),
Windows x86-64, and Linux x86-64. The `v1.0.0` and `v1.0.1` tags were never
released because a complete set of installers couldn't be built from them.

### Added

- Projects, where you work in ChatGPT conversations alongside your reviews.
  Conversations have streamed replies, approvals, saved drafts, recovery after a crash, and model,
  effort, and usage controls.
- Optional research tools for papers and sources, local LaTeX and Stata runs,
  results, research notes, and claims with evidence you confirm. Project tools
  cover manuscript builds, referee responses, experiments, literature, and a
  **Theory** panel that keeps numerical checks separate from proofs.
- Project packages (`.pwex`) share chosen research items with a coauthor,
  without conversations, credentials, or command settings, and show conflicts
  on import. Research backups (`.pwrx`) restore a whole store.
- A storage report previews cleanup you can undo. A project can export a draft
  workflow or send a fixed copy of a paper to a new review.
- **Agent profiles** are searchable built-in and custom profiles with editable
  system prompts and tools, plus writing, code review, and economics research
  starters. A profile can add to or replace Codex's base prompt.
- Projects in a Git repository show the branch, uncommitted work, recent
  commits, and commits waiting on the remote. Pipeline fetches with your own
  credentials only when you ask, and never commits, merges, or pushes.
- LaTeX extraction reads files the project references up to three folders
  above the selection, such as `\input{../output/numbers.tex}`, and notes each
  one.

### Changed

- Reviews and conversations share one ChatGPT sign-in and model list. Existing
  sign-ins move over when that's unambiguous, you choose between conflicting
  accounts in Settings, and account changes wait for active work.
- The composer is rebuilt. One picker sets model and thinking and shows your
  remaining usage, Send and Stop share a button, and the **+** menu adds files,
  a project document, or an automation. Attached sources appear as chips with
  roles such as "Main paper".
- **Projects** opens a searchable index with pins, recent activity, and links
  to conversations. A new project can start with a conversation, and Home opens
  straight to its action cards.
- Project overviews report the paper's state: version, last build, open and
  regressed findings, numbers and claims that no longer match results, pending
  decisions, and changes since your last visit. Setup controls and an optional
  target date are in **Project settings**.
- Settings has seven searchable categories and separate conversation and
  review connections. New preferences cover text size, editor wrapping,
  spacing, message shortcuts, startup, and notifications with optional sound.
- Auto Paper Review is the default and replaces Paper Review (Full) and Paper
  Review (Quick). Retired profiles and your edits are archived in
  `~/.pipeline/profiles/.retired-builtins/`, and their step prompts stay in
  the workflow editor.
- Research backups (`.pwrx` format 2) include conversation working files, and
  older versions can't read them. Backups leave out external project folders,
  review and automation history, Trash, and credentials.
- Google models run only through the Gemini API with a Google API key.
  Workflows that name `gemini` must switch to `antigravity`, and the `pro`,
  `flash`, and `flash-lite` aliases no longer work.
- macOS builds from source can be unsigned for local use. Official releases
  are signed.

### Fixed

- On Windows, conversations can read project files, and Windows backups pass
  their integrity check.
- Opened from the Dock or a desktop launcher on macOS and Linux, Pipeline finds
  command-line tools more reliably, including with fish, csh, tcsh, nushell,
  and shells that print startup messages.
- A provider CLI that crashes or runs out of memory is reported as a retryable
  failure, not "Pipeline cancelled".
- One unreadable file no longer stops a folder input, and folders with
  multi-gigabyte data files run.
- PDF extraction works despite denied macOS folder permissions, reports
  encrypted PDFs, retries failed chunks, accepts PDFs up to 256 MB, and opens
  OneDrive placeholders on Windows. Provider limits (100 pages or 24 MB) are
  stated up front.
- Steps fall back to ordinary calls when shared context or a shared Claude
  session is unavailable, instead of failing the review.
- Auto Paper Review rejects a folder chosen as a source tree and resumes after
  the reviewer count changes. The results view no longer crashes when a
  paper's authors are missing.
- The dependency check requires every provider a workflow uses.
- Finished parallel steps are saved as each completes, and retention can't
  delete a just-finished review. Windows builds support long paths.
- LaTeX extraction skips commented-out and circular `\input{}` lines and notes
  unreadable includes instead of dropping chapters.
- A PaddleOCR-VL install survives an unrelated review finishing or being
  cancelled.
- Workflow drafts survive a damaged recovery cache, and moving between projects
  or automations protects unsaved work.

### Security

- Updated rustls and several development dependencies to patched versions.

## 0.9.0 — 2026-08-10

### Added

- **Paper Review (Auto)** reads the paper, picks one or two subject reviewers
  and up to four method reviewers from a catalog covering 29 disciplines, and
  checks every combined comment against the paper. It also identifies the kind
  of paper, such as a replication or null result, so reviewers judge it on its
  own terms.
- Projects group related runs and revisions, with an issue list that tracks
  issues, decisions, and regressions across runs and exports to Markdown.
- A run preview shows the timeline, provider and file access, and expected
  model work before launch.
- Issue cards link their evidence to pages and files in the run.
- A Workflow Gallery offers editable templates for revision responses,
  literature positioning, thesis chapters, and rubric-based review.
- Settings and workflows warn about unsaved changes, and deleting stored data
  previews what will be removed.
- Supported-platform and maintenance policies, contribution guidelines, and
  issue forms.

### Changed

- Local OCR uses PaddleOCR-VL Full Parser only, and settings that chose the
  fast Q8 extractor switch automatically.
- The source-build CLI can pick a workflow per run and install, repair, or
  uninstall the PaddleOCR bundle.
- Update checks offer stable releases only. The main window has a minimum
  size, and the Windows installer includes WebView2 and refuses downgrades.

### Fixed

- A PaddleOCR-VL parser installed by an older version keeps working after an
  update.
- Unsupported inputs fail before the run, and readiness checks include the
  exact PDF tools each extraction method needs.
- Stopping a folder watch shows at once. Readable text is kept when a provider
  can't accept a large structured bundle.
- Saved-run export is complete and all-or-nothing, with a new destination each
  time.
- Math renders offline, PDF printing waits for fonts, and saves handle edits
  made during a save.

### Security

- Models see only a private copy of the selected document, and LaTeX projects
  expose only the files the document uses.
- Re-runs recheck saved inputs and the run budget before contacting a model.
  The app console shows at most 16 KB of each request.
- Added a data-sharing notice before each run, privacy documentation, and a
  private vulnerability-reporting policy.
- Updated frontend dependencies to clear every npm audit finding.
- Each installer is built from a verified tag and comes with a SHA-256
  checksum, a GitHub build attestation, and software bills of materials.
