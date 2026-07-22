# Generated release notices

`npm run prepare:release-resources` populates this directory from the canonical
repository license, locked Rust/npm dependencies, and the platform-specific
Poppler provenance produced by release CI. Tauri bundles the generated files so
license terms, the CycloneDX SBOM, source locations, and exact Poppler hashes are
available offline in every installed application.

