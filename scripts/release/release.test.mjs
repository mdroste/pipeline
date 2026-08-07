// Cross-platform test entry point. npm uses cmd.exe on Windows, which does not
// expand the `*.test.mjs` glob used by POSIX shells.
import "./poppler-provenance.test.mjs";
import "./prepare-notices.test.mjs";
import "./validate-release-identity.test.mjs";
import "./smoke-packaged-app.test.mjs";
import "./artifact-integrity.test.mjs";
import "./validate-release-assets.test.mjs";
import "./workflow-hardening.test.mjs";
import "./npm-overrides.test.mjs";
