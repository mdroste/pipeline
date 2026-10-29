# Security policy

Security fixes go into the latest stable release. See [Support](SUPPORT.md)
for supported systems and updates.

## Report a vulnerability

Use [GitHub's private vulnerability reporting form](https://github.com/mdroste/pipeline/security/advisories/new).
If that isn't available, contact the maintainer through a verified contact
method on their GitHub profile. Please don't post an unfixed vulnerability in
a public issue.

Include:

- the Pipeline version, operating system, and how you installed it;
- the affected feature and provider connection, if relevant;
- steps to reproduce the problem, ideally with a small made-up example; and
- what the problem lets someone read, change, or disrupt.

Leave out real papers, credentials, access tokens, and signing keys. The
maintainer will assess the report and coordinate disclosure if a fix is
needed. There's no guaranteed response time.

Report provider outages and wrong model answers with the
[bug report form](https://github.com/mdroste/pipeline/issues/new?template=bug_report.yml).
If one exposes data or bypasses a permission, report it privately instead.

## Keeping your research safe

Pipeline encrypts saved API keys but stores research files and conversations
as ordinary files. Protect your computer account, backups, and exports. See
[Privacy](PRIVACY.md) for what's stored where.

A document or model response can contain text that tries to steer the
assistant. Give an assistant or an imported workflow only the files and
permissions it needs. Your permission mode decides which actions need your
approval. Imported backups and project packages never include credentials or
permission to run commands.

## How releases are built

Releases are built by GitHub Actions from a signed version tag, after the
automated checks pass. macOS installers are signed and notarized. Windows
installers aren't signed. Every installer has a published checksum. See
[Releasing](RELEASING.md) for the full process. Accepted dependency advisories
are listed in the [Cargo audit configuration](gui/src-tauri/.cargo/audit.toml).
