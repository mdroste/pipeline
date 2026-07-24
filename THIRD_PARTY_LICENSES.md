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
- `POPPLER_PROVENANCE.json` and `POPPLER_INPUT_LOCK.json`; and
- license/copyright files shipped by the Cargo, npm, and Poppler package inputs.

The platform-specific SBOM and Poppler provenance are also attached to the
GitHub release beside each installer. The generated records are authoritative
for the exact files in a particular build.

## Poppler release inputs

Pipeline invokes `pdftoppm` and `pdftotext` as separate programs. The Poppler
project is licensed under GPL terms; exact declarations vary by packaged
version and are recorded in the platform provenance.

| Platform | Binary input | Poppler version | Reproducibility / integrity anchor |
|---|---|---:|---|
| macOS arm64 and x86-64 | Homebrew bottles selected from `homebrew/core` | 26.07.0 | core commit `7777a34439b22630233caa93268c175864ee66ec`; formula source SHA-256 `304832f48f8a47fdca90c6b6d1f684e68f37c10c9a0726f345f4ca9df4ca01e2` |
| Linux x86-64 | Ubuntu 22.04 `poppler-utils` / `libpoppler118` | 22.02.0-2ubuntu0.13 | exact apt package version; copied-file hashes and exact dpkg closure in the provenance |
| Windows x86-64 | `oschwartz10612/poppler-windows` `Release-25.12.0-0.zip` | 25.12.0 | archive SHA-256 `9499c7474e4deb41c80ef5ea4a18cc1f3843695fbfa3c247db5c46c6eab2e26f` |

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

## Windows Poppler DLL closure

The Windows provider's archive supplies Poppler's license/data notices but does
not include a conda environment lock or separate license text for every copied
dependency DLL. Pipeline therefore records the archive hash, every DLL hash,
and the component-level notices below without inventing unavailable package
versions. This limitation is explicit in the Windows provenance.

| Component | License family | Upstream |
|---|---|---|
| cairo | LGPL-2.1-only OR MPL-1.1 | <https://cairographics.org/> |
| Expat | MIT | <https://libexpat.github.io/> |
| Fontconfig | MIT-style | <https://fontconfig.org/> |
| FreeType | FTL OR GPL-2.0-only | <https://freetype.org/> |
| GNU libiconv / libcharset | LGPL-2.1-or-later | <https://www.gnu.org/software/libiconv/> |
| libdeflate | MIT | <https://github.com/ebiggers/libdeflate> |
| libjpeg-turbo | BSD-3-Clause, IJG, and zlib terms | <https://libjpeg-turbo.org/> |
| Little CMS (lcms2) | MIT | <https://www.littlecms.com/> |
| Lerc | Apache-2.0 | <https://github.com/Esri/lerc> |
| OpenJPEG | BSD-2-Clause | <https://www.openjpeg.org/> |
| OpenSSL 3 | Apache-2.0 | <https://www.openssl.org/> |
| curl | curl license | <https://curl.se/> |
| liblzma / xz | 0BSD and component-specific terms | <https://tukaani.org/xz/> |
| libpng | libpng license | <http://www.libpng.org/pub/png/libpng.html> |
| libssh2 | BSD-3-Clause | <https://www.libssh2.org/> |
| libtiff | libtiff license | <https://libtiff.gitlab.io/libtiff/> |
| pixman | MIT | <https://pixman.org/> |
| zlib | zlib license | <https://zlib.net/> |
| zstd | BSD-3-Clause OR GPL-2.0-only | <https://github.com/facebook/zstd> |

## Rust and JavaScript dependencies

The application links Rust crates and ships a compiled JavaScript frontend.
Their exact names, versions, package checksums, and declared license expressions
are generated from `Cargo.lock`, Cargo metadata, and
`package-lock.json` into the CycloneDX SBOM. Release preparation also copies
the license, COPYING, copyright, and NOTICE files that installed crate/npm
packages provide. This avoids a manually maintained dependency list drifting
from the binaries.

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
- marker-pdf and its downloaded model weights retain their upstream GPL-3.0
  and model-license terms, shown on the install card.

Operating-system frameworks (for example WebView2, WebKit, and macOS system
frameworks) are not redistributed in Pipeline's resource bundle and are not
listed as bundled components here.
