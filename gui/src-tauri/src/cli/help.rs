//! Exact command help text for the Pipeline CLI.

pub(super) const MAIN_HELP: &str = "\
Pipeline CLI

USAGE:
  pipeline-cli run [OPTIONS]
  pipeline-cli check [OPTIONS]
  pipeline-cli batch --input-dir <DIR> [OPTIONS]
  pipeline-cli workflow <schema|template|validate|install> ...
  pipeline-cli profiles [--json]
  pipeline-cli profiles show <ID>
  pipeline-cli engines [status] [--json]
  pipeline-cli engines install paddle
  pipeline-cli engines uninstall paddle --yes

COMMANDS:
  run       Run one workflow
  check     Check dependencies for one prospective run
  batch     Run one workflow over each supported document in a folder
  workflow  Author, validate, or explicitly install portable workflow JSON
  profiles  List or inspect installed workflows
  engines   Inspect or manage the PaddleOCR-VL Full Parser bundle
  help      Show help for Pipeline or one command

Run `pipeline-cli help <COMMAND>` for command-specific help.
The CLI shares settings, profiles, and run history with the app. Every run is
saved in your research data folder, ~/.pipeline/ by default.
";

pub(super) const RUN_HELP: &str = "\
Run one Pipeline workflow

USAGE:
  pipeline-cli run [--input <PATH>] [--interpret-as <KIND>]
                       [--profile <ID> | --workflow <FILE|->]
                       [--var <KEY=VALUE>]...
                       [--extra-input <KEY=PATH>]... [--out <FILE>] [--force]
                       [--max-cost <USD>] [--max-calls <N>] [--max-minutes <N>]

OPTIONS:
  -i, --input <PATH>          Primary document or folder. Omit only for a
                              workflow configured with no primary input.
      --interpret-as <KIND>   document, latex-project, or source-tree.
  -p, --profile <ID>          Workflow for this run. Does not change the
                              desktop app's active workflow.
  -w, --workflow <FILE|->     Portable workflow JSON file, or '-' for stdin.
                              Runs ephemerally and is not installed.
      --var <KEY=VALUE>       Workflow variable; repeat for multiple values.
      --extra-input <KEY=PATH>
                              Named input; repeat for multiple inputs.
  -o, --out <FILE>            Write Markdown to FILE instead of stdout.
      --force                 Replace an existing --out file.
      --max-cost <USD>        Stop starting new steps once estimated direct-API
                              spend reaches USD (API list prices). 0 = no limit.
      --max-calls <N>         Stop starting new steps after N model calls.
      --max-minutes <N>       Stop starting new steps after N minutes.
  -h, --help                  Show this help.

Progress goes to stderr. A non-zero exit status indicates invalid arguments,
failed preflight, interruption, or one or more failed workflow steps.
";

pub(super) const BATCH_HELP: &str = "\
Run one Pipeline workflow over a folder of documents

USAGE:
  pipeline-cli batch --input-dir <DIR> [--profile <ID> | --workflow <FILE|->]
                     [--var <KEY=VALUE>]...
                     [--extra-input <KEY=PATH>]... [--out-dir <DIR>] [--force]
                     [--max-cost <USD>] [--max-calls <N>] [--max-minutes <N>]

OPTIONS:
  -i, --input-dir <DIR>       Folder containing PDF, TeX, or DOCX inputs.
  -p, --profile <ID>          Workflow for this batch. Does not change the
                              desktop app's active workflow.
  -w, --workflow <FILE|->     Portable workflow JSON file, or '-' for stdin.
      --var <KEY=VALUE>       Workflow variable; repeat for multiple values.
      --extra-input <KEY=PATH>
                              Named input shared by every run; repeatable.
  -o, --out-dir <DIR>         Also write each Markdown report to DIR.
      --force                 Replace existing reports in --out-dir.
      --max-cost <USD>        Stop starting new steps once estimated direct-API
                              spend reaches USD (API list prices). 0 = no limit.
      --max-calls <N>         Stop starting new steps after N model calls.
      --max-minutes <N>       Stop starting new steps after N minutes.
  -h, --help                  Show this help.

Folder discovery is non-recursive and skips hidden files. Every run is also
saved in your research data folder, under ~/.pipeline/runs/ by default. Limits
apply to each document's run and override Settings for this batch only.
";

pub(super) const CHECK_HELP: &str = "\
Check dependencies for one prospective Pipeline run

USAGE:
  pipeline-cli check [--input <PATH>] [--interpret-as <KIND>]
                     [--profile <ID> | --workflow <FILE|->]
                     [--var <KEY=VALUE>]...
                     [--extra-input <KEY=PATH>]... [--json]

OPTIONS:
  -i, --input <PATH>          Primary document or folder. For a PDF, this
                              checks the workflow's selected PDF parser.
      --interpret-as <KIND>   document, latex-project, or source-tree.
  -p, --profile <ID>          Workflow to check without changing the desktop
                              app's active workflow.
  -w, --workflow <FILE|->     Portable workflow JSON file, or '-' for stdin.
      --var <KEY=VALUE>       Workflow variable; repeatable.
      --extra-input <KEY=PATH>
                              Named input; repeatable. Named PDFs also trigger
                              the selected PDF-parser dependency check.
      --json                  Print the complete dependency report as JSON.
  -h, --help                  Show this help.

No extraction or model call is made. With --json, output includes the
normalized workflow hash, scheduler waves, work bounds, and dependencies.
Exit status 0 means the exact run is
ready; exit status 1 means one or more required dependencies are unavailable.
";

pub(super) const WORKFLOW_HELP: &str = "\
Author and use portable Pipeline workflow JSON

USAGE:
  pipeline-cli workflow schema
  pipeline-cli workflow template
  pipeline-cli workflow validate <FILE|-> [--json]
  pipeline-cli workflow install <FILE|-> [--json]

COMMANDS:
  schema    Print the current JSON Schema.
  template  Print a small valid workflow agents can modify.
  validate  Strictly parse and semantically validate without installing.
  install   Validate, then save as a new installed profile.

Unknown fields are rejected. '-' reads at most 10 MB of UTF-8 JSON from
stdin. `run`, `check`, and `batch` use --workflow ephemerally; only this
explicit install command changes the profile store.
";

pub(super) const PROFILES_HELP: &str = "\
List or inspect Pipeline workflows

USAGE:
  pipeline-cli profiles [--json]
  pipeline-cli profiles show <ID>

OPTIONS:
      --json  Print the profile-summary list as JSON.
  -h, --help  Show this help.

The active desktop workflow is marked with `*` in the human-readable list.
`profiles show` prints the workflow's current export JSON.
";

pub(super) const ENGINES_HELP: &str = "\
Inspect or manage Pipeline's local PaddleOCR-VL Full Parser bundle

USAGE:
  pipeline-cli engines [status] [--json]
  pipeline-cli engines install paddle
  pipeline-cli engines uninstall paddle --yes

COMMANDS:
  status      Show availability, installation state, version, and disk usage.
  install     Install or repair the checksum-pinned managed bundle.
  uninstall   Remove both app-owned Paddle bundle roots. Requires --yes.

`paddle` is an alias for the registry ID `paddleocr-vl-parser`. Installation
downloads roughly 2.9 GB and requires roughly 3.8 GB after installation. All
files remain under ~/.pipeline/native/; no system Python, pip, Conda, Docker,
or llama.cpp installation is used.
";
