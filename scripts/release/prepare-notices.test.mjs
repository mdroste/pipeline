import test from "node:test";
import assert from "node:assert/strict";

import { buildSbom, cargoComponents, npmComponents } from "./prepare-notices.mjs";

test("extracts locked Cargo packages and their archive checksum", () => {
  const components = cargoComponents('[[package]]\nname = "serde"\nversion = "1.0.0"\nsource = "registry+x"\nchecksum = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"\n');
  assert.equal(components[0].purl, "pkg:cargo/serde@1.0.0");
  assert.equal(components[0].hashes[0].content, "a".repeat(64));
});

test("deduplicates npm packages by package URL", () => {
  const lock = { packages: {
    "node_modules/a": { version: "1.2.3", integrity: `sha512-${Buffer.from("hash").toString("base64")}` },
    "node_modules/x/node_modules/a": { version: "1.2.3" },
  } };
  assert.equal(npmComponents(lock).length, 1);
});

test("builds a deterministic CycloneDX document", () => {
  const input = { version: "1.2.3", cargoLock: "", packageLock: { packages: {} } };
  assert.deepEqual(buildSbom(input), buildSbom(input));
  assert.equal(buildSbom(input).bomFormat, "CycloneDX");
});

