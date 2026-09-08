import type { Plugin } from "vite";
import { readFileSync, readdirSync } from "node:fs";
import { resolve, join } from "node:path";
const groups = ["cmaps", "standard_fonts", "wasm", "images"];
export function pdfAssetsPlugin(): Plugin {
  const root = resolve(import.meta.dirname, "node_modules/pdfjs-dist");
  const assetRoot = (group: string) =>
    join(root, group === "images" ? "web/images" : group);
  return {
    name: "pipeline-pdf-assets",
    configureServer(server) {
      server.middlewares.use("/pdf-assets", (req, res, next) => {
        const match = /^\/([a-z_]+)\/([\w.-]+)$/.exec(
          (req.url ?? "").split("?")[0],
        );
        if (!match || !groups.includes(match[1])) return next();
        try {
          const bytes = readFileSync(join(assetRoot(match[1]), match[2]));
          res.setHeader(
            "Content-Type",
            match[2].endsWith(".wasm")
              ? "application/wasm"
              : "application/octet-stream",
          );
          res.end(bytes);
        } catch {
          res.statusCode = 404;
          res.end("PDF asset unavailable");
        }
      });
    },
    generateBundle() {
      for (const group of groups)
        for (const name of readdirSync(assetRoot(group), {
          withFileTypes: true,
        })) {
          if (name.isFile())
            this.emitFile({
              type: "asset",
              fileName: `pdf-assets/${group}/${name.name}`,
              source: readFileSync(join(assetRoot(group), name.name)),
            });
        }
    },
  };
}
