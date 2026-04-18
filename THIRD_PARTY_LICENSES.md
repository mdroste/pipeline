# Third-party licenses

Pipeline is distributed under the MIT License (see `LICENSE`).
The compiled application bundles the following third-party software.
This file documents the licenses and the source of each bundled component.

---

## poppler

`pdftoppm` and `pdftotext` from the [poppler](https://poppler.freedesktop.org/) project are bundled and invoked as separate processes (no linking).

- **License**: GNU General Public License, version 2 or later (with some files under GPL-3 or later).
- **Source**: <https://poppler.freedesktop.org/poppler-25.12.0.tar.xz>
  (or replace the version suffix with the bundled version reported in the app's About page).
- **Project page**: <https://poppler.freedesktop.org/>
- **License text**: <https://www.gnu.org/licenses/old-licenses/gpl-2.0.txt>

The Windows binaries are taken unmodified from the [`oschwartz10612/poppler-windows`](https://github.com/oschwartz10612/poppler-windows) prebuilt distribution. macOS and Linux binaries are built by the upstream Homebrew and Debian packagers; Pipeline's CI downloads them and adjusts dynamic library paths so they load relative to the application bundle.

In accordance with GPL-2 §3, full corresponding source code for each bundled poppler version is available at the URL above for at least three years from the release date of the corresponding Pipeline version.

---

## Bundled dynamic libraries

The poppler binaries link against the following shared libraries, which are also bundled with Pipeline on macOS, Linux (AppImage), and Windows.

| Library | License | Upstream |
|---|---|---|
| FreeType | FreeType License (BSD-style) or GPL-2 | <https://freetype.org/> |
| Fontconfig | MIT-style | <https://fontconfig.org/> |
| libpng | libpng License (permissive) | <http://www.libpng.org/pub/png/libpng.html> |
| libjpeg-turbo | BSD-style + IJG | <https://libjpeg-turbo.org/> |
| OpenJPEG | BSD-2-Clause | <https://www.openjpeg.org/> |
| Little CMS (lcms2) | MIT | <https://www.littlecms.com/> |
| zlib | zlib License | <https://zlib.net/> |
| libxml2 (Linux/Windows) | MIT | <https://gitlab.gnome.org/GNOME/libxml2> |
| libstdc++ (Linux/Windows) | GPL-3 with GCC Runtime Library Exception | <https://gcc.gnu.org/onlinedocs/libstdc++/> |
| libgcc (Linux/Windows) | GPL-3 with GCC Runtime Library Exception | <https://gcc.gnu.org/> |

Each library's license text is available at its upstream URL above. Pipeline distributes these libraries unmodified.

---

## Notes

Pipeline itself does not statically or dynamically link against any of the above libraries; the bundled binaries are invoked as separate processes (mere aggregation under GPL §0). Pipeline's own source code remains under the MIT License.

For the full text of any license referenced here, follow the linked upstream URL. Source archives for the GPL-licensed components are available from the upstream project pages and will be provided on request for three years from the date of any Pipeline release that bundles them. Contact: see the repository on GitHub.
