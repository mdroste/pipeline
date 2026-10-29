# Support

The latest stable release is supported on the platforms below. Help with
older versions, prereleases, and source builds is given as time allows.

## Supported platforms

| Platform | Installer | Requirements |
| --- | --- | --- |
| macOS | Apple Silicon or Intel `.dmg` | macOS 15 or later |
| Windows | x86-64 `.exe` | Windows 11, or Windows 10 22H2 with Extended Security Updates |
| Linux | x86-64 AppImage | Ubuntu 22.04 or a similar distribution with glibc 2.35 or later and FUSE 2 |

There's no Windows on ARM build. Other Linux distributions may run the
AppImage, but only those in the table are supported.

On Windows, the installer downloads WebView2 if it's missing, so it needs an
internet connection. The Windows installer isn't signed, so SmartScreen may
warn you before it runs. On Windows, working copies and accepting edits in
**Review edits** are newer and less mature than on macOS.

## Updates

Pipeline checks GitHub for new stable releases and shows a banner when one is
available. Download and run the installer to update. Only the latest stable
release gets security and maintenance fixes.

## Common problems

| Problem | What to check |
| --- | --- |
| A model connection doesn't work | Open **Settings → Connections** and check the account, the provider's command-line tool, or the API key |
| ChatGPT works in your terminal but not in Pipeline | Sign in inside Pipeline. It uses its own ChatGPT sign-in, separate from the terminal's |
| Claude subscription access fails | Run `claude auth login` and choose a Claude subscription account. See [Connections](docs/connections.md) |
| A PDF review fails during extraction | Read the error and check the parser in **Settings → Reviews**. See [PDF extraction](docs/reviews.md#pdf-extraction) |
| A review stops early | See [When a review can't finish](docs/reviews.md#when-a-review-cant-finish) |
| Research is missing after changing the data folder | Choose the previous folder and restart. Changing folders doesn't move data |
| A scheduled automation didn't run | Keep Pipeline running and the computer awake. See [Automations](docs/automations.md) |

## Report a bug or request a feature

Search [existing issues](https://github.com/mdroste/pipeline/issues) first.
For a new bug, use the
[bug report form](https://github.com/mdroste/pipeline/issues/new?template=bug_report.yml)
and include:

- the Pipeline version, from **Settings → Advanced & About**;
- your operating system, and whether you used an installer or built from
  source;
- what you did, what you expected, and what happened; and
- the provider, connection type, and a short error message, if relevant.

Use a small example document you can share. Remove confidential papers, API
keys, tokens, and private information from logs and screenshots.

For a feature request, describe the research task you want to do and what
makes it hard now. Report security problems privately, as described in
[Security](SECURITY.md).
