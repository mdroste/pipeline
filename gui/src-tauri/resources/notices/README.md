# Generated release notices

`npm run prepare:release-resources` copies Pipeline's license and third-party
notice into this directory and writes a short versioned notice. Tauri bundles
only those three files. Poppler's own license text is stored beside the bundled
Poppler executables.

The same preparation step records the observed Poppler version and file hashes
in `resources/poppler/BUNDLE_INVENTORY.json` when the tools are present. Official
release builds require them. This inventory is not a complete dependency SBOM.
