import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import fs from "node:fs";
import { join } from "node:path";
import { it } from "vitest";

const root = join(process.cwd(), "public/fonts");
const manifest = JSON.parse(
  fs.readFileSync(join(root, "manifest.json"), "utf8"),
) as {
  fonts: { family: string; file: string; bytes: number; sha256: string }[];
  licenses: { family: string; file: string; sha256: string }[];
};
const css = fs.readFileSync(
  join(process.cwd(), "src/ui/font-faces.css"),
  "utf8",
);
const checksum = (bytes: Buffer) =>
  createHash("sha256").update(bytes).digest("hex");

it("bundles every declared WOFF2 asset intact with its license and no remote font URLs", () => {
  const paths = [...css.matchAll(/url\(["']?\/fonts\/([^)'"\s]+)/g)].map(
    (match) => match[1],
  );
  assert.equal(paths.length, manifest.fonts.length);
  assert.doesNotMatch(css, /url\(["']?https?:/);
  assert.deepEqual(
    [...paths].sort(),
    manifest.fonts.map((font) => font.file).sort(),
  );
  assert.equal(new Set(manifest.fonts.map((font) => font.family)).size, 5);
  for (const font of manifest.fonts) {
    const bytes = fs.readFileSync(join(root, font.file));
    assert.equal(bytes.subarray(0, 4).toString(), "wOF2", font.file);
    assert.equal(bytes.length, font.bytes, font.file);
    assert.equal(checksum(bytes), font.sha256, font.file);
    assert.ok(
      manifest.licenses.some((license) => license.family === font.family),
      font.family,
    );
  }
  for (const license of manifest.licenses) {
    const bytes = fs.readFileSync(join(root, license.file));
    assert.equal(checksum(bytes), license.sha256, license.file);
    assert.match(bytes.toString(), /SIL OPEN FONT LICENSE/);
  }
});
