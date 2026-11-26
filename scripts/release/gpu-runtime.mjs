// Signed optional Windows recognition runtime. Its manifest binds both the
// download and the installed inventory; the app trusts the updater public key.
import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { updaterPublicKey, verifyUpdateSignature } from "./update-manifest.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
export const gpuRuntimeName = version => `Pipeline-${version}-GPU-Windows-x64.zip`;
export const gpuRuntimeManifestName = version => `Pipeline-${version}-GPU-Windows-x64.json`;
export const gpuRuntimeAssets = version => [gpuRuntimeName(version), gpuRuntimeManifestName(version), `${gpuRuntimeManifestName(version)}.sig`];
const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");

export function prepareGpuRuntime(source, destination) {
  if (fs.existsSync(destination)) throw new Error("GPU runtime staging must be a fresh directory");
  const entries = [];
  function copy(directory, relative = "") {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const name = path.posix.join(relative, entry.name);
      if (!/^[\w./-]+$/.test(name)) throw new Error(`Unsafe runtime filename: ${name}`);
      if (entry.isDirectory()) copy(path.join(directory, entry.name), name);
      else if (entry.isFile()) {
        const bytes = fs.readFileSync(path.join(directory, entry.name));
        const target = path.join(destination, "runtime", name);
        fs.mkdirSync(path.dirname(target), { recursive: true });
        fs.writeFileSync(target, bytes);
        entries.push({ path: `runtime/${name}`, kind: "file", size: bytes.length, sha256: sha256(bytes), permissions: 0 });
      } else throw new Error(`Unsupported runtime object: ${name}`);
    }
  }
  copy(source);
  for (const name of ["llama-server.exe", "llama-server-impl.dll", "ggml-vulkan.dll"])
    if (!entries.some(entry => entry.path === `runtime/${name}`)) throw new Error(`Missing GPU runtime file: ${name}`);
  entries.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  fs.writeFileSync(path.join(destination, "integrity.json"), JSON.stringify({ schema_version: 1, scopes: ["runtime"], entries }));
}

export function gpuRuntimeManifest(version, runtimeVersion, archive, integrity) {
  if (!/^\d+\.\d+\.\d+$/.test(version) || !/^b\d+$/.test(runtimeVersion)) throw new Error("Invalid runtime release identity");
  const inventory = JSON.parse(integrity);
  const server = inventory.entries.find(entry => entry.path === "runtime/llama-server.exe");
  if (!server) throw new Error("Runtime inventory lacks its server");
  return { schema_version: 1, engine: "llama-vulkan", runtime_version: runtimeVersion,
    app_version: version, platform: "windows-x86_64", archive: gpuRuntimeName(version),
    archive_bytes: archive.length, archive_sha256: sha256(archive),
    integrity_sha256: sha256(integrity), server_sha256: server.sha256 };
}

export function verifyGpuRuntime(archive, manifestBytes, signature, version, key = updaterPublicKey()) {
  verifyUpdateSignature(manifestBytes, signature, key, version);
  const manifest = JSON.parse(manifestBytes);
  const pinned = JSON.parse(fs.readFileSync(path.join(root, "scripts/release/llama-vulkan-lock.json"), "utf8"));
  if (manifest.schema_version !== 1 || manifest.engine !== "llama-vulkan" || manifest.platform !== "windows-x86_64"
      || manifest.app_version !== version || manifest.runtime_version !== pinned.version
      || manifest.archive !== gpuRuntimeName(version) || manifest.archive_bytes !== archive.length
      || manifest.archive_sha256 !== sha256(archive)
      || !/^[a-f0-9]{64}$/.test(manifest.integrity_sha256) || !/^[a-f0-9]{64}$/.test(manifest.server_sha256))
    throw new Error("GPU runtime does not match its signed release manifest");
  return manifest;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [mode, a, b, c] = process.argv.slice(2);
  if (mode === "prepare") prepareGpuRuntime(a, b);
  else if (mode === "manifest") {
    const version = JSON.parse(fs.readFileSync(path.join(root, "gui/package.json"), "utf8")).version;
    const runtime = JSON.parse(fs.readFileSync(path.join(root, "scripts/release/llama-vulkan-lock.json"), "utf8")).version;
    fs.writeFileSync(c, `${JSON.stringify(gpuRuntimeManifest(version, runtime, fs.readFileSync(a), fs.readFileSync(b)), null, 2)}\n`);
  } else if (mode === "verify") {
    const version = JSON.parse(fs.readFileSync(path.join(root, "gui/package.json"), "utf8")).version;
    verifyGpuRuntime(fs.readFileSync(a), fs.readFileSync(b), fs.readFileSync(c, "utf8"), version);
  } else throw new Error("Expected prepare, manifest, or verify");
}
