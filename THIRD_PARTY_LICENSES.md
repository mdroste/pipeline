# Third-party software notices

Pipeline itself is distributed under the MIT License (see `LICENSE`). The
desktop bundle also contains compiled Rust/JavaScript dependencies and a
platform-specific Poppler command-line distribution. This notice describes the
release inputs; it does not replace the license text supplied by any project.

Every installer contains this notice, Pipeline's MIT license, and the Poppler
license text supplied with that platform's Poppler distribution. The GitHub
release page publishes the platform installers themselves.

## Poppler release inputs

Pipeline invokes `pdftoppm` and `pdftotext` as separate programs. The Poppler
project is licensed under GPL terms; exact declarations vary by packaged
version and are recorded in the platform provenance.

| Platform | Binary input | Poppler version | Reproducibility / integrity anchor |
|---|---|---:|---|
| macOS arm64 and x86-64 | Homebrew bottles selected from `homebrew/core` | 26.07.0 | core commit `7777a34439b22630233caa93268c175864ee66ec`; formula source SHA-256 `304832f48f8a47fdca90c6b6d1f684e68f37c10c9a0726f345f4ca9df4ca01e2` |
| Linux x86-64 | Ubuntu 22.04 `poppler-utils` / `libpoppler118` | 22.02.0-2ubuntu0.13 | Ubuntu archive snapshot `20260723T000000Z`; exact apt package version, copied-file hashes, and dpkg closure in provenance |
| Windows x86-64 | `oschwartz10612/poppler-windows` `Release-25.12.0-0.zip` | 25.12.0 | archive SHA-256 `9499c7474e4deb41c80ef5ea4a18cc1f3843695fbfa3c247db5c46c6eab2e26f`; every shipped PE file matched by SHA-256 to an exact conda-forge package archive |

Corresponding source and packaging provenance:

- macOS upstream source: `https://poppler.freedesktop.org/poppler-26.07.0.tar.xz`
  (SHA-256 `304832f48f8a47fdca90c6b6d1f684e68f37c10c9a0726f345f4ca9df4ca01e2`),
  with the formula and dependency bottle checksums at the pinned Homebrew commit.
- Linux upstream source: `https://poppler.freedesktop.org/poppler-22.02.0.tar.xz`
  (SHA-256 `e390c8b806f6c9f0e35c8462033e0a738bb2460ebd660bdb8b6dca01556193e1`),
  plus Ubuntu's patched source package at
  `https://launchpad.net/ubuntu/+source/poppler/22.02.0-2ubuntu0.13`.
- Windows upstream source: `https://poppler.freedesktop.org/poppler-25.12.0.tar.xz`
  (SHA-256 `c18b40eb36b1a0c5b86e29ca054bf0770304583da4f2cdd42fe86eca6a20de48`).
  Binary packaging recipe: `https://github.com/oschwartz10612/poppler-windows/tree/v25.12.0-0`;
  it repackages conda-forge's Poppler build and dependency DLLs.

These URLs and hashes identify corresponding source; source archives are not
embedded in the installed application. Poppler's COPYING or GPL text and the
required character-map data supplied by each binary package are bundled.

Before a public release is published, the maintainer must complete the legal
review in `RELEASING.md`, including confirmation of the required corresponding-
source distribution or offer for each platform. This project does not assert
that upstream URLs alone satisfy a particular GPL distribution option.

## Windows Poppler DLL closure

The provider ran an unpinned conda solve and did not preserve `conda-meta` in
its ZIP. `scripts/release/poppler-lock.json` records the reviewed package
closure, including package versions, builds, licenses, source URLs, and hashes.

| Conda package | Exact version / build | Declared license |
|---|---|---|
| cairo | 1.18.4 / `h5782bbf_0` | LGPL-2.1-only OR MPL-1.1 |
| expat | 2.7.3 / `hac47afa_0` | MIT |
| fontconfig | 2.15.0 / `h765892d_1` | MIT |
| lcms2 | 2.17 / `hbcf6048_0` | MIT |
| lerc | 4.0.0 / `h6470a55_1` | Apache-2.0 |
| libcurl | 8.17.0 / `h43ecb02_0` | curl |
| libdeflate | 1.25 / `h51727cc_0` | MIT |
| libfreetype6 | 2.14.1 / `hdbac1cb_0` | GPL-2.0-only OR FTL |
| libiconv | 1.18 / `hc1393d2_2` | LGPL-2.1-only |
| libjpeg-turbo | 3.1.2 / `hfd05255_0` | IJG AND BSD-3-Clause AND Zlib |
| liblzma | 5.8.1 / `h2466b09_2` | 0BSD |
| libpng | 1.6.51 / `h7351971_0` | zlib-acknowledgement |
| libssh2 | 1.11.1 / `h9aa295b_0` | BSD-3-Clause |
| libtiff | 4.7.1 / `h8f73337_1` | HPND |
| libzlib | 1.3.1 / `h2466b09_2` | Zlib |
| openjpeg | 2.5.4 / `h24db6dd_0` | BSD-2-Clause |
| openssl | 3.6.0 / `h725018a_0` | Apache-2.0 |
| pixman | 0.46.4 / `h5112557_1` | MIT |
| poppler | 25.12.0 / `hb0e4504_0` | GPL-2.0-only |
| zstd | 1.5.7 / `h534d264_6` | BSD-3-Clause |

The provider ZIP still does not supply a separate license file for every
dependency package. Declared conda metadata and the provider's included notices
are inventory evidence, not a substitute for the pre-publication legal review
and any additional notices or source offer that review requires.

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
