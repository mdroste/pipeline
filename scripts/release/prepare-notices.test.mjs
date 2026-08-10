import test from "node:test";
import assert from "node:assert/strict";

import {
  buildSbom,
  cargoComponents,
  npmComponents,
  popplerComponents,
  pythonRuntimeComponents,
  validateLicenseInventory,
} from "./prepare-notices.mjs";

test("adds checksum-locked optional Python runtime packages to the SBOM", () => {
  const components = pythonRuntimeComponents({
    locks: {
      "linux-x86_64": `lock-version = "1.0"
[[packages]]
name = "Example_Package"
version = "1.2.3"
wheels = [{ url = "https://files.pythonhosted.org/example.whl", hashes = { sha256 = "${"a".repeat(64)}" } }]
`,
    },
    runtimeLock: {
      python: {
        version: "3.12.13",
        buildRelease: "20260623",
        license: "PSF-2.0",
        artifacts: {
          "linux-x86_64": {
            target: "x86_64-unknown-linux-gnu",
            sha256: "b".repeat(64),
            lock: "pylock.linux-x86_64.toml",
            lockSha256: "c".repeat(64),
          },
        },
      },
      uv: {
        version: "0.11.26",
        license: "Apache-2.0 OR MIT",
        artifacts: {
          "linux-x86_64": {
            target: "x86_64-unknown-linux-gnu",
            sha256: "d".repeat(64),
          },
        },
      },
      layoutModel: {
        name: "PP-DocLayoutV3",
        version: "paddle3.0.0",
        license: "Apache-2.0",
        url: "https://example.test/model.tar",
        sha256: "e".repeat(64),
      },
    },
    licenseInventory: {
      packages: [{
        name: "example-package",
        version: "1.2.3",
        declaredLicense: "MIT",
      }],
    },
  });
  const pythonPackage = components.find((item) => item.group === "pypi");
  assert.equal(pythonPackage.purl, "pkg:pypi/example-package@1.2.3");
  assert.equal(pythonPackage.licenses[0].license.name, "MIT");
  assert.ok(pythonPackage.properties.some(
    (property) => property.name === "pipeline:wheel-sha256"
      && property.value.startsWith("a".repeat(64)),
  ));
  assert.ok(components.some((item) => item.type === "machine-learning-model"));
  assert.ok(components.some((item) => item.name === "CPython"));
});

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

test("marks npm build-only inputs separately from production inputs", () => {
  const components = npmComponents({ packages: {
    "node_modules/runtime": { name: "runtime", version: "1.0.0" },
    "node_modules/build": { name: "build", version: "2.0.0", dev: true },
  } });
  const scopes = new Map(components.map((item) => [
    item.name,
    item.properties.find((property) => property.name === "pipeline:npm-install-scope").value,
  ]));
  assert.equal(scopes.get("runtime"), "production");
  assert.equal(scopes.get("build"), "development");
});

test("npm purl deduplication gives production scope precedence", () => {
  const components = npmComponents({ packages: {
    "node_modules/tool/node_modules/parse5": {
      name: "parse5",
      version: "7.3.0",
      dev: true,
    },
    "node_modules/runtime/node_modules/parse5": {
      name: "parse5",
      version: "7.3.0",
    },
  } });
  assert.equal(components.length, 1);
  assert.deepEqual(components[0].properties, [{
    name: "pipeline:npm-install-scope",
    value: "production",
  }]);
});

test("adds scanner-compatible package URLs to native package inventory", () => {
  const components = popplerComponents({
    platform: "linux",
    version: "22.02.0",
    license: "GPL-2.0-only",
    binaryProvider: "Ubuntu",
    source: { url: "https://example.test/poppler.tar.xz" },
    files: [],
    packageInventory: {
      manager: "dpkg",
      distribution: "Ubuntu 22.04 LTS (jammy)",
      packages: [{ name: "libpoppler118:amd64", version: "22.02.0-2ubuntu0.13" }],
    },
  });
  assert.equal(
    components[1].purl,
    "pkg:deb/ubuntu/libpoppler118@22.02.0-2ubuntu0.13?distro=ubuntu-22.04&arch=amd64",
  );
});

test("encodes Windows native packages as exact Anchore conda identities", () => {
  const components = popplerComponents({
    platform: "windows",
    version: "25.12.0",
    license: "GPL-2.0-only",
    binaryProvider: "provider",
    source: { url: "https://example.test/poppler.tar.xz" },
    files: [],
    packageInventory: {
      manager: "conda",
      channel: "conda-forge",
      subdir: "win-64",
      packages: [{
        name: "openssl",
        version: "3.6.0",
        build: "h725018a_0",
        license: "Apache-2.0",
        sourceUrl: "https://conda.anaconda.org/conda-forge/win-64/openssl-3.6.0-h725018a_0.conda",
        archiveSha256: "a".repeat(64),
      }],
    },
  });
  assert.equal(
    components[1].purl,
    "pkg:generic/conda@3.6.0?build=h725018a_0&channel=conda-forge&name=openssl&subdir=win-64",
  );
  assert.equal(components[1].version, "3.6.0");
  assert.equal(components[1].name, "openssl");
  assert.equal(components[1].group, undefined);
  assert.equal(components[1].hashes[0].content, "a".repeat(64));
  assert.ok(components[1].properties.some(
    (property) => property.name === "syft:package:type" && property.value === "conda",
  ));
});

test("uses Anchore's Homebrew package type without prefixing the package name", () => {
  const components = popplerComponents({
    platform: "macos",
    version: "26.07.0",
    license: "GPL-2.0-only",
    binaryProvider: "Homebrew",
    source: { url: "https://example.test/poppler.tar.xz" },
    files: [],
    packageInventory: {
      manager: "homebrew",
      packages: [{ name: "openssl@3", version: "3.6.0", license: "Apache-2.0" }],
    },
  });
  assert.equal(components[1].purl, "pkg:homebrew/openssl%403@3.6.0");
  assert.equal(components[1].name, "openssl@3");
  assert.equal(components[1].group, undefined);
});

test("builds a deterministic CycloneDX document", () => {
  const input = { version: "1.2.3", cargoLock: "", packageLock: { packages: {} } };
  assert.deepEqual(buildSbom(input), buildSbom(input));
  assert.equal(buildSbom(input).bomFormat, "CycloneDX");
});

test("build-input SBOM graph covers every unique component from the Pipeline root", () => {
  const sbom = buildSbom({
    version: "1.2.3",
    cargoLock: '[[package]]\nname = "serde"\nversion = "1.0.0"\nsource = "registry+x"\n',
    packageLock: {
      packages: {
        "node_modules/react": { name: "react", version: "19.0.0" },
      },
    },
  });
  const rootReference = sbom.metadata.component["bom-ref"];
  const componentReferences = sbom.components.map((component) => component["bom-ref"]);
  assert.equal(new Set(componentReferences).size, componentReferences.length);
  assert.deepEqual(
    sbom.dependencies.find((entry) => entry.ref === rootReference)?.dependsOn,
    componentReferences,
  );
  assert.deepEqual(
    new Set(sbom.dependencies.map((entry) => entry.ref)),
    new Set([rootReference, ...componentReferences]),
  );
  assert.ok(
    sbom.dependencies
      .filter((entry) => entry.ref !== rootReference)
      .every((entry) => entry.dependsOn.length === 0),
  );
});

test("license inventory accepts a declaration or an offline license file", () => {
  const result = validateLicenseInventory([
    {
      ecosystem: "cargo",
      name: "declared",
      version: "1.0.0",
      declaredLicense: "MIT",
      licenseFiles: [],
    },
    {
      ecosystem: "npm",
      name: "copied",
      version: "2.0.0",
      licenseFiles: ["licenses/npm/copied-2.0.0/LICENSE"],
    },
  ], true);
  assert.equal(result.complete, true);
  assert.deepEqual(result.incomplete, []);
});

test("release license inventory rejects packages with no license evidence", () => {
  assert.throws(
    () => validateLicenseInventory([{
      ecosystem: "npm",
      name: "unknown",
      version: "1.0.0",
      licenseFiles: [],
    }], true),
    /npm:unknown@1\.0\.0/,
  );
});
