# Security policy

Security fixes go into the latest stable Pipeline release. See
[Support](SUPPORT.md) for supported operating systems and update instructions.

## Report a vulnerability

Use [GitHub's private vulnerability-reporting form](https://github.com/mdroste/pipeline/security/advisories/new).
If private reporting is unavailable, contact the maintainer through a verified
contact method on their GitHub profile. Keep an unpatched vulnerability out of
public issues.

Include:

- the Pipeline version, operating system, and installation type;
- the affected feature and provider connection, if relevant;
- steps to reproduce the problem, ideally with a small synthetic example; and
- what the problem allows someone to read, change, or interrupt.

Do not include real papers, credentials, access tokens, or signing keys. The
maintainer will assess the report and coordinate disclosure when a fix is
needed. This project has no guaranteed response or fix deadline.

Use the ordinary [bug form](https://github.com/mdroste/pipeline/issues/new?template=bug_report.yml)
for provider outages or inaccurate model responses that do not expose data or
bypass permissions.

## Protecting local research

Pipeline stores research files and transcripts without application-level
encryption. API keys are encrypted, with the encryption key held under the
same operating-system account. Protect your account, backups, and exports.
See [Privacy](PRIVACY.md) for data locations and provider transmission.

Review the files and permissions assigned to assistants and imported
workflows. Document text and model responses can contain misleading
instructions. Pipeline checks tool requests and file access, and asks for
approval where the selected permission mode requires it.

Research backups and project packages are inspected before import. Credentials
and execution permissions are excluded. Restoring a backup requires an empty
research store and a decision about each external project folder. Imported
commands do not run automatically.

## Release checks

The current [release workflow](.github/workflows/release.yml) builds from a
signed, annotated version tag after the automated quality checks pass. It
signs and notarizes macOS installers, publishes installer checksums, and
leaves a draft for review before publication. Windows installers are unsigned.

The workflow verifies the bundled Poppler file inventory and the locked
Windows download. It does not currently publish software bills of materials
or build attestations, run automated vulnerability scans, or launch packaged
apps. Maintainers perform the additional checks described in
[Releasing](RELEASING.md) before publication.

Dependency-advisory exceptions are recorded in
[the Cargo audit configuration](gui/src-tauri/.cargo/audit.toml). The
[Workspace release checks](docs/workbench/release-qualification.md) cover
provider tools, permissions, archives, and crash recovery.
