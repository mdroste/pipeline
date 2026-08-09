# Third-party software notices

Pipeline itself is distributed under the MIT License (see `LICENSE`). The
desktop bundle also contains compiled Rust/JavaScript dependencies and a
platform-specific Poppler command-line distribution. This notice describes the
release inputs; it does not replace the license text supplied by any project.

Every release contains an offline `notices/` directory with:

- this notice and Pipeline's MIT license;
- `THIRD_PARTY_SBOM.cdx.json`, a CycloneDX inventory of locked Cargo/npm inputs,
  the exact Poppler package inventory available for that platform, and SHA-256
  hashes for every bundled Poppler file;
- `LICENSE_INVENTORY.json`, which maps every resolved Cargo/npm package to its
  declared license and any copied offline license/NOTICE files;
- `POPPLER_PROVENANCE.json` and `POPPLER_INPUT_LOCK.json`; and
- license/copyright files shipped by the Cargo, npm, and Poppler package inputs.

This offline document is the platform build-input SBOM. The GitHub release
attaches it beside a separate artifact SBOM and Poppler provenance. The artifact
SBOM records the exact installer SHA-256 and the hashes of both evidence files,
so an input inventory cannot be silently substituted between platform builds.

Release preparation fails when a resolved Cargo/npm package has neither a
declared license nor a copied offline license file. Native provenance likewise
requires a nonempty, unique package inventory and bundled license evidence:
Linux requires a version and Debian copyright file for every copied package;
Homebrew requires a version plus declared or offline license evidence for every
formula; and Windows requires an exact version, build, license declaration,
source-package hash, and file-hash attribution for every shipped DLL or
executable. These mechanical checks help detect omissions; they are not legal
determinations that every obligation has been satisfied.

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

These URLs and hashes are provenance for obtaining and checking source; source
archives are not embedded in the installed application. Poppler's COPYING,
GPL, Adobe data, and README notices supplied by each binary package are bundled
offline. The macOS build additionally gathers installed Homebrew license files;
the Linux build gathers Debian copyright files and the common GPL texts.

Before a public release is published, the maintainer must complete the legal
review in `RELEASING.md`, including confirmation of the required corresponding-
source distribution or offer for each platform. This project does not assert
that upstream URLs alone satisfy a particular GPL distribution option.

## Windows Poppler DLL closure

The provider ran an unpinned conda solve and did not preserve `conda-meta` in
its ZIP. Pipeline reconstructed the closure by matching the SHA-256 of every
shipped DLL and executable to the payload in an exact conda-forge `win-64`
package archive. `scripts/release/poppler-lock.json` pins each package name,
version, build, declared license, download URL, package-archive SHA-256, and
owned PE-file hashes. Release validation fails if a PE file is unattributed,
has more than one owner, or differs from the pinned payload. The build-input
SBOM emits those records as Anchore CondaPkg identities for version-level
vulnerability matching.

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
Their exact names, versions, package checksums, and declared license expressions
are generated from `Cargo.lock`, Cargo metadata, and
`package-lock.json` into the CycloneDX SBOM. Release preparation also copies
the license, COPYING, copyright, and NOTICE files that installed crate/npm
packages provide. This avoids a manually maintained dependency list drifting
from the binaries. The build-input SBOM marks npm production and development
inputs separately. Cargo and npm advisories are checked directly from their
locks; every platform SBOM is additionally scanned for high or critical native
findings before the draft release can complete.

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
  (MIT OR Apache-2.0), uses it to provision app-owned CPython 3.12 (Python
  Software Foundation License), and installs exact top-level
  `paddleocr==3.7.0` and `paddlepaddle==3.2.1` packages (Apache-2.0), including
  PaddleOCR's document-parser dependencies and PP-DocLayoutV3 model files.
  These components are stored in a private version directory and removed with
  the Full Parser. The installer writes a `packages.txt` inventory and binds
  its SHA-256 into `install.json` so the resolved wheel closure is auditable.
  License metadata and notices supplied inside those installed distributions
  remain with the private environment; they are not part of Pipeline's bundled
  Cargo/npm/Poppler SBOM.

Operating-system frameworks (for example WebView2, WebKit, and macOS system
frameworks) are not redistributed in Pipeline's resource bundle and are not
listed as bundled components here.
