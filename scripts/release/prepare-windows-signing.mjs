// Generate a Windows-only Tauri overlay. Object-form arguments preserve spaces
// in checkout paths; local builds retain the ordinary unsigned configuration.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function windowsSigningConfig(root, baseResources) {
  return {
    bundle: {
      createUpdaterArtifacts: true,
      // GPU recognition is a separately signed engine download.
      resources: [...baseResources],
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
