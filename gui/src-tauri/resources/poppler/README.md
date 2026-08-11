# Bundled Poppler binaries

Release CI replaces the contents of this directory with the two Poppler
executables used by Pipeline, their dynamic-library closure, Poppler data, and
the applicable license files. The generated files are ignored by Git.

- macOS packages contain Homebrew's `pdftoppm` and `pdftotext`, with library
  paths relocated into the app bundle. Those Mach-O files are signed before
  Tauri builds and notarizes the DMG.
- Windows packages use the checksum-locked archive in
  `scripts/release/poppler-lock.json` and are intentionally unsigned.
- Linux packages contain the distribution binaries and non-system shared
  libraries needed by the AppImage.

The release workflow caches the generated directory by platform and rebuilds
it only when the bundling script or lock file changes.
