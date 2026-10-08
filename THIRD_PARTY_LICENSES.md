# Third-party software notices

Pipeline itself is distributed under the MIT License (see `LICENSE`). The
desktop bundle also contains compiled Rust/JavaScript dependencies and a
platform-specific Poppler command-line distribution. This notice describes the
release inputs; it does not replace the license text supplied by any project.

Every installer contains this notice, Pipeline's MIT license, and the Poppler
license text supplied with that platform's Poppler distribution. The GitHub
release page publishes the platform installers themselves.

## Bundled Poppler

Pipeline invokes `pdftoppm` and `pdftotext` as separate programs. Poppler is
licensed under GPL terms; consult the license text bundled beside these tools.

The active build uses the installed Homebrew Poppler distribution on macOS and
the installed Ubuntu packages on Linux. These inputs are not pinned to an exact
formula commit or APT snapshot. Windows uses the archive URL and SHA-256 in
`scripts/release/poppler-lock.json`; the build verifies that archive before
copying its tools and dependency DLLs. It does not independently match each DLL
to a conda package archive.

`resources/poppler/BUNDLE_INVENTORY.json` records the observed tool version and
SHA-256 of every file in the bundled Poppler tree. The build verifies cached
resources before use and regenerates the inventory after macOS signing. This
inventory is not a full dependency SBOM or evidence of exact source provenance.
The source repository's Poppler lock and provenance utilities are review inputs;
they do not establish pins that the active bundler does not enforce.

Upstream source and distribution records are available from the
[Poppler project](https://poppler.freedesktop.org/),
[Homebrew formula repository](https://github.com/Homebrew/homebrew-core),
[Ubuntu source archive](https://launchpad.net/ubuntu/+source/poppler), and
[Windows distribution](https://github.com/oschwartz10612/poppler-windows).
Source archives are not embedded in the installed application.

Before public release, the maintainer must review the actual bundled closure,
its licenses and notices, and its corresponding-source distribution or offer
under `RELEASING.md`. Upstream links and file hashes alone do not establish that
this requirement is satisfied. The Windows provider ZIP does not include a
separate license file for every dependency; any additional notices must be
supplied as part of that review.

## Rust and JavaScript dependencies

The application links Rust crates and ships a compiled JavaScript frontend.
Their exact versions and package checksums are locked in `Cargo.lock` and
`package-lock.json`; their upstream license declarations remain the governing
terms.

## Optional managed extraction engines

These components are not bundled in Pipeline installers. They are downloaded
only after the user selects Install in Settings, are stored under
`~/.pipeline/`, and can be removed there:

- PaddlePaddle's PaddleOCR-VL 1.6 GGUF model and vision projector are licensed
  under Apache-2.0. Pipeline pins model revision
  `511b09642bb324401f15f97cc23bc67e8f0a291d` and verifies both files with
  SHA-256 before activation.
- The managed Paddle engine downloads a pinned `llama.cpp` b9637 platform
  runtime, licensed under MIT, from the project's official GitHub release and
  verifies the release asset's published SHA-256.
- The optional PaddleOCR-VL Full Parser downloads checksum-pinned uv 0.11.26
  (MIT OR Apache-2.0), verified CPython 3.12.13 (Python Software Foundation
  License), and platform-specific wheel locks rooted at `paddleocr==3.7.0` and
  `paddlepaddle==3.2.1` (Apache-2.0). Every Python artifact URL and SHA-256 is
  fixed in a committed PEP 751 lock. PP-DocLayoutV3 is installed from a fixed
  Paddle model artifact under Apache-2.0 and verified with a release-owned
  SHA-256 before local initialization. These components are stored in a private
  version directory and removed with the Full Parser. Its runtime and wheel
  locks remain in the source repository.

Operating-system frameworks (for example WebView2, WebKit, and macOS system
frameworks) are not redistributed in Pipeline's resource bundle and are not
listed as bundled components here.
