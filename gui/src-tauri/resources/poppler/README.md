# Bundled poppler binaries

This directory is populated by CI before each platform build. It is empty in source control.

CI drops in:
- `pdftoppm` (used by Claude Code's Read tool to render PDF pages)
- `pdftotext` (used by Pipeline's native PDF extraction fallback)
- The dynamic libraries each binary needs
- Poppler's data files, package license/copyright files, and an exact
  `PROVENANCE.json` file-hash manifest

Layout per platform:

- **Windows**: `pdftoppm.exe`, `pdftotext.exe`, and all required DLLs flat in this directory.
- **macOS**: `pdftoppm`, `pdftotext`, and a `lib/` subdirectory with relocated
  dylibs (dependencies rewritten to `@executable_path/lib/`). Every Mach-O is
  architecture, loader, minimum-OS, and Developer ID validated even on a cache hit.
- **Linux AppImage**: `pdftoppm`, `pdftotext`, and a `lib/` subdirectory with required `.so` files.

The pinned inputs are in `scripts/release/poppler-lock.json`. Release CI runs a
real PDF text/render smoke test and verifies all provenance hashes after cache
restore and again from the final macOS DMG, Linux AppImage, or Windows NSIS
payload.

For local development, this directory may be empty. Pipeline falls back to system poppler if found on PATH; otherwise PDF features that require it are unavailable until you `brew install poppler` (macOS) or `apt-get install poppler-utils` (Linux).

See `THIRD_PARTY_LICENSES.md` at the repo root for license obligations.
