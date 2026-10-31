# Third-party software notices

Pipeline is released under the [MIT License](LICENSE). Its installers also
contain Rust and JavaScript libraries and the Poppler PDF tools, which keep
their own licenses as described below.

Every installer includes this notice, Pipeline's MIT license, and the Poppler
license text that came with that platform's Poppler build.

## Bundled Poppler

Pipeline runs Poppler's `pdftoppm` and `pdftotext` as separate programs.
Poppler is licensed under the GPL. Its license text is bundled beside these
tools.

| Platform | Source of the bundled tools |
| --- | --- |
| macOS | The Homebrew Poppler installed on the build machine |
| Linux | The Ubuntu Poppler packages installed on the build machine |
| Windows | The archive pinned by URL and SHA-256 in `scripts/release/poppler-lock.json`, including its libraries |

The macOS and Linux builds aren't pinned to an exact Homebrew formula or
Ubuntu package snapshot. `resources/poppler/BUNDLE_INVENTORY.json` in each
build records the Poppler version and the SHA-256 of every bundled file.

Source code for Poppler and its distributions is available from:

- the [Poppler project](https://poppler.freedesktop.org/);
- the [Homebrew formula repository](https://github.com/Homebrew/homebrew-core);
- the [Ubuntu source archive](https://launchpad.net/ubuntu/+source/poppler); and
- the [Windows distribution](https://github.com/oschwartz10612/poppler-windows).

The installed app doesn't include these source archives.

## Rust and JavaScript libraries

The app includes compiled Rust crates and a compiled JavaScript interface.
`Cargo.lock` and `package-lock.json` record their exact versions and
checksums. Each library's own license applies. PDF highlight export uses the
`lopdf` crate (MIT). Its dependencies use the MIT or Apache-2.0 licenses, and
`unicode-normalization` also carries the Unicode-3.0 data license.

## Optional PDF parser

These components aren't in the installer. Pipeline downloads them only when
you choose **Install** in Settings, stores them in `~/.pipeline/`, and removes
them when you uninstall the parser.

| Component | Version | License |
| --- | --- | --- |
| PaddleOCR-VL 1.6 GGUF model and vision projector | Model revision `511b09642bb324401f15f97cc23bc67e8f0a291d` | Apache-2.0 |
| `llama.cpp` runtime, from the project's GitHub releases | b9637 | MIT |
| uv | 0.11.26 | MIT or Apache-2.0 |
| CPython | 3.12.13 | Python Software Foundation License |
| `paddleocr` and `paddlepaddle`, with their locked dependencies | 3.7.0 and 3.2.1 | Apache-2.0 |
| PP-DocLayoutV3 layout model | Fixed Paddle model artifact | Apache-2.0 |

Pipeline checks the SHA-256 of every download before using it. The Python
packages are pinned by URL and hash in a PEP 751 lock file in this
repository.

## Optional conversation tools

Pipeline downloads these tools when command or browser setup is needed. Their packages and
license notices stay under `~/.pipeline/native/` on this computer.

| Component | License |
| --- | --- |
| Node.js, including npm | MIT and other component licenses; see the runtime's LICENSE and npm package notices |
| Playwright | Apache-2.0 |
| Chromium and its bundled libraries | BSD-style and other component licenses; see the downloaded distribution’s notices |
| Anthropic Sandbox Runtime (`@anthropic-ai/sandbox-runtime`) | Apache-2.0 |

The command engine runs locally and requires no Anthropic account.

Operating-system components, such as WebView2, WebKit, and macOS frameworks,
aren't redistributed with Pipeline.
