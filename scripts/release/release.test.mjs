// Cross-platform test entry point. npm uses cmd.exe on Windows, which does not
// expand the `*.test.mjs` glob used by POSIX shells.
import "./library-licenses.test.mjs";
import "./validate-release-identity.test.mjs";
import "./smoke-packaged-app.test.mjs";
import "./validate-macos-machos.test.mjs";
import "./validate-release-assets.test.mjs";
import "./workflow-hardening.test.mjs";
import "./npm-overrides.test.mjs";
import "./paddle-parser-lock.test.mjs";
import "./python.test.mjs";

import "./draft-release.test.mjs";
import "./update-check-assets.test.mjs";

import "./release-fixes.test.mjs";
import "./windows-signing.test.mjs";
import "./installer-presentation.test.mjs";
import "../source-size-report.test.mjs";
import "../check-doc-style.test.mjs";
import "../command-surface.test.mjs";
