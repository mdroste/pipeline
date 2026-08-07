# Generated release notices

`npm run prepare:release-resources` populates this directory from the canonical
repository license, locked Rust/npm dependencies, and the platform-specific
Poppler provenance produced by release CI. Tauri bundles the generated files so
license terms, the CycloneDX build-input SBOM, source locations, and exact
Poppler hashes are available offline in every installed application. Public
releases additionally publish a separate artifact SBOM that binds the installer
hash to this build-input SBOM and the Poppler provenance file.

Release builds also generate `LICENSE_INVENTORY.json`, which maps every resolved
Cargo/npm package to its declared license and copied offline license/NOTICE
files. Notice preparation fails when a release package has neither form of
license evidence. Poppler provenance independently enforces the native package
inventory and platform-specific offline license-evidence policy.
