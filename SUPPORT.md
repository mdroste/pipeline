# Support

Pipeline maintains the latest stable release on the platforms below. Older
versions, prereleases, and source builds on other platforms receive help as
time permits.

## Supported platforms

| Platform | Installer | Requirements |
| --- | --- | --- |
| macOS | Apple Silicon or Intel `.dmg` | macOS 15 or later |
| Windows | x86-64 `.exe` | Windows 11 on a supported servicing release, or Windows 10 22H2 while the device receives security updates |
| Linux | x86-64 AppImage | Ubuntu 22.04 or a compatible distribution with glibc 2.35 or later and FUSE 2 |

Windows ARM64 is not an official build target. Other Linux distributions and
extracted AppImages may work but are not separately tested release targets.

On Windows, the installer supplies WebView2 if it is missing. That download
requires an internet connection. The Windows installer is unsigned, so
SmartScreen may warn before it runs. macOS releases are signed and notarized.

## Updates

Pipeline checks GitHub for stable releases and links to a newer version when
one is available. Download and install the update yourself; the app does not
replace itself automatically. Prereleases are not offered by this check.

Only the latest stable release receives security and maintenance fixes.

## Common problems

| Problem | What to check |
| --- | --- |
| A model connection is unavailable | Open **Settings → Connections** and check the account, installed provider tool, or API key |
| ChatGPT works in your terminal but not in Pipeline | Sign in inside Pipeline; its managed account is separate from the terminal's Codex account |
| Claude subscription access fails | Run `claude auth login` and choose a Claude subscription account; see the [connection guide](docs/claude-code.md) |
| A PDF review fails during extraction | Inspect the extraction error and the selected parser in **Settings → Reviews**; see [PDF extraction](docs/reviews.md#pdf-extraction) |
| Research disappears after changing the data folder | Select the previous folder and restart; changing folders does not move existing records |
| A scheduled automation did not run | Keep Pipeline running and the computer awake; see [Automations](docs/tasks.md) |
| Project file editing is unavailable on Windows | Reading is supported; accepting file edits and creating isolated task copies are not yet available |

## Report a bug or request a feature

Search [existing issues](https://github.com/mdroste/pipeline/issues) first.
For a new bug, use the
[bug-report form](https://github.com/mdroste/pipeline/issues/new?template=bug_report.yml)
and include:

- the Pipeline version from **Settings → Advanced & About**;
- your operating system and whether you used an installer or built from source;
- the steps that caused the problem, what you expected, and what happened; and
- the provider, connection type, and a short error excerpt, if relevant.

Use a small example document that you can share. Remove confidential papers,
API keys, tokens, and private information from logs and screenshots.

For a feature request, describe the research task you want to complete and
what makes it difficult now. Report vulnerabilities privately under
[SECURITY.md](SECURITY.md).

## Testing and release checks

CI runs native tests on Ubuntu 22.04, Windows Server 2022, and macOS 15.
Release builds cover all four installer targets. Packaged installation,
launch, and provider behavior require separate checks on client machines;
CI does not launch the packaged apps. See [Releasing](RELEASING.md) for the
release checklist and [current limitations](README.md#current-limitations).
