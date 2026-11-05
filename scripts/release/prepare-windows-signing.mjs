// Generate a Windows-only Tauri overlay. Object-form arguments preserve spaces
// in checkout paths; local builds retain the ordinary unsigned configuration.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const LLAMA_VULKAN_RESOURCES = "resources/llama-vulkan/**/*";

export function windowsSigningConfig(root, baseResources) {
  return {
    bundle: {
      createUpdaterArtifacts: true,
      // Tauri replaces arrays when it merges a configuration. The Vulkan bundle
      // is Windows-only, so it joins the shared list here.
      resources: [...baseResources, LLAMA_VULKAN_RESOURCES],
      windows: {
        signCommand: {
          cmd: "pwsh",
          args: ["-NoProfile", "-File", path.join(root, "scripts/release/windows-sign.ps1"), "-FilePath", "%1"],
        },
      },
    },
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
  const output = process.argv[2];
  if (!output) throw new Error("Supply the output configuration path");
  const { bundle } = JSON.parse(fs.readFileSync(path.join(root, "gui/src-tauri/tauri.conf.json"), "utf8"));
  fs.writeFileSync(output, JSON.stringify(windowsSigningConfig(root, bundle.resources), null, 2) + "\n");
}
