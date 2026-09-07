# Security policy

## Supported versions

Security fixes are made only for the latest published stable Pipeline release.

| Version | Security fixes |
|---|---|
| Latest stable release | Yes |
| Older stable releases | No |
| Prereleases and development builds | No |

Upgrade before reporting a problem that may already be fixed. Supported
operating systems and architectures are listed in [SUPPORT.md](SUPPORT.md);
running Pipeline on an operating system that no longer receives vendor
security updates is not a supported security configuration.

## Reporting a vulnerability

Use [GitHub's private vulnerability-reporting
form](https://github.com/mdroste/pipeline/security/advisories/new) from this
repository's **Security** tab. If GitHub says private reporting is unavailable,
contact the repository owner privately through the contact method on their
GitHub profile before disclosing the issue publicly. Do not send sensitive
details to an address or account that you have not independently verified as
belonging to the maintainer.

Do not open a public issue for an unpatched vulnerability. Do not include real
papers, API keys, provider tokens, signing material, or other confidential data
in a report. A minimal synthetic reproducer is preferred.

Please allow time for the maintainer to reproduce and assess the report before
public disclosure. The maintainer will coordinate a disclosure date when a fix
is required; this project does not promise a fixed response or remediation SLA.
If a report is only a provider outage, model-quality problem, unsupported
platform failure, or prompt result with no security boundary impact, use the
ordinary bug form instead.

Useful reports identify:

- the Pipeline version, operating system, and installation type;
- whether Workspace or a Workflow is affected, including the provider,
  transport, tool, archive, or extraction engine involved;
- exact reproduction steps and expected versus observed behavior;
- whether untrusted document content, imported profiles, URLs, archives, or
  local files are involved; and
- the likely confidentiality, integrity, or availability impact.

## Release-security controls

Public releases are built from a signed immutable version tag in GitHub Actions.
The release workflow requires a protected `release` environment, limits token
permissions by job, pins third-party Actions to reviewed commits, signs macOS
and Windows artifacts, verifies bundled Poppler inputs, publishes separate
hash-bound artifact and build-input SBOMs plus installer checksums/provenance,
fails on high or critical findings from the platform build-input scan, creates
GitHub build-provenance attestations for the complete evidence set, and leaves
the release as a draft for final review. Cargo/npm audits include development
and build dependencies. Native lower-severity findings and scanner coverage
gaps require explicit human review; a versionless or unattributed Windows DLL
fails the release before scanning. The scanner is pinned to Grype v0.110.0,
and the release runs its corresponding Syft v1.42.3 CycloneDX decoder against
all four platform inventories to reject lost, renamed, or invented native
package identities.

Documented Cargo advisory exceptions are in
`gui/src-tauri/.cargo/audit.toml`. They are not blanket suppressions: new audit
warnings fail CI.

## Workspace security boundaries

Workspace runs Codex App Server in an isolated home with its own managed
ChatGPT credentials and strips inherited provider keys and endpoint overrides.
Its native process tree, cancellation, SQLite store, and writable roots are
separate from Workflow runs. A compatibility failure disables the affected
capability; it must not be concealed by falling back to `codex exec`, another
provider, or broader permissions.

Treat model output and imported research material as untrusted. Dynamic tools
derive scope from the active durable binding, validate and bound every request,
and record a receipt before side effects. Model-created notes, claims, and
evidence remain proposals until the user explicitly accepts or confirms them.
Pending approvals and questions are scoped to the live connection epoch,
request ID, and method so stale requests cannot authorize a later connection.

Treat `.pwrx` files as untrusted archives. Inspection/import reject traversal,
links, directories, duplicates, unmanifested entries, resource-limit excesses,
newer schemas, corrupt databases, broken foreign keys, and missing or
hash-mismatched blobs. Restore requires an empty store and an explicit
remap-or-detach decision for every archived filesystem root, retires native
bindings, never restores Codex credentials, and never executes imported tools.
See `docs/workbench/release-qualification.md` for the live security and crash
gates that deterministic tests do not replace.
