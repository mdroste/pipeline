# Support policy

Pipeline is a local desktop application. Support covers the latest published
patch release on the official binary platforms below. Development builds,
prereleases, older Pipeline releases, and source builds on other platforms are
best effort.

## Supported release platforms

| Platform | Official binary | Minimum and support boundary |
|---|---|---|
| macOS | Apple Silicon and Intel `.dmg` | macOS 15.0 or later. This floor matches the complete bundled Poppler library closure and is enforced by release validation. |
| Windows | x86-64 NSIS installer | Windows 11 on a Microsoft-supported servicing release. Windows 10 22H2 is the compatibility floor, but is supported only on devices receiving Microsoft security updates, such as eligible ESU-managed systems. ARM64 Windows is not an official binary target. |
| Linux | x86-64 AppImage | Ubuntu 22.04 LTS or a compatible newer distribution with glibc 2.35 or later and FUSE 2. Other distributions and extract-and-run AppImage use are compatibility paths rather than separately tested release targets. |

The Windows installer includes the WebView2 offline installer. Microsoft
documents the underlying [WebView2 operating-system support
matrix](https://learn.microsoft.com/microsoft-edge/webview2/) separately; that
matrix is broader than Pipeline's tested support policy.

CI compiles and opens the application on Ubuntu 22.04 x86-64, Windows Server
2022 x86-64, macOS 15 Apple Silicon, and macOS 15 Intel. Before publication,
the release checklist also requires clean-machine installation and launch
checks on supported client operating systems. Passing CI is not a substitute
for those release checks.

## Update policy

Only the latest stable Pipeline release receives security and maintenance
fixes. Pipeline checks GitHub's stable-release channel and links to a newer
release when one exists; it never installs an update automatically.
Prereleases are not offered by the in-app update check, and the public release
workflow rejects prerelease or build-metadata tags until a separate publishing
channel is deliberately designed.

The application identifier is intentionally fixed at `com.pipeline.report`,
the identifier used by v1.0.0. Changing it would create a different
application identity and can break upgrades, settings continuity, signing
expectations, and operating-system permissions.

## Getting help

Before filing a bug, reproduce it on the latest release and review the
[README](README.md), [privacy notice](PRIVACY.md), and known limitations.
Use the repository's bug-report form for ordinary defects. Do not attach real
papers, API keys, provider tokens, or unredacted run artifacts.

Security vulnerabilities must follow [SECURITY.md](SECURITY.md) and must not be
reported in a public issue.
