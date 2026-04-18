# Bundled poppler binaries

This directory is populated by CI before each platform build. It is empty in source control.

CI drops in:
- `pdftoppm` (used by Claude Code's Read tool to render PDF pages)
- `pdftotext` (used by Pipeline's native PDF extraction fallback)
- The dynamic libraries each binary needs

Layout per platform:

- **Windows**: `pdftoppm.exe`, `pdftotext.exe`, and all required DLLs flat in this directory.
- **macOS**: `pdftoppm`, `pdftotext`, and a `Frameworks/` subdirectory with relocated dylibs (install names rewritten to `@executable_path/../Frameworks/`).
- **Linux AppImage**: `pdftoppm`, `pdftotext`, and a `lib/` subdirectory with required `.so` files.
- **Linux .deb / .rpm**: not populated — `poppler-utils` is declared as a package dependency instead.

For local development, this directory may be empty. Pipeline falls back to system poppler if found on PATH; otherwise PDF features that require it are unavailable until you `brew install poppler` (macOS) or `apt-get install poppler-utils` (Linux).

See `THIRD_PARTY_LICENSES.md` at the repo root for license obligations.
