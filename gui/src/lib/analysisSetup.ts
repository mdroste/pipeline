import { adapterTemplate, type ExecutionAdapter } from "./workbench/toolchains";

export function scriptAdapter(path: string): ExecutionAdapter | null {
  const extension = path.split(".").at(-1)?.toLowerCase();
  return (
    (
      {
        py: "python",
        r: "r",
        do: "stata",
        jl: "julia",
        ipynb: "notebook",
        qmd: "quarto",
        rmd: "quarto",
      } as Record<string, ExecutionAdapter>
    )[extension ?? ""] ?? null
  );
}

export function scriptSetup(path: string, platform: string) {
  const adapter = scriptAdapter(path);
  if (!adapter)
    throw new Error(
      "Choose a Python, R, Stata, Julia, Jupyter, or Quarto source.",
    );
  const template = adapterTemplate(adapter, platform);
  const base = path.replace(/\.[^.]+$/, "");
  const replacements: Record<string, string> = {
    "analysis.py": path,
    "analysis.R": path,
    "analysis.jl": path,
    "main.do": path,
    "analysis.ipynb": path,
    "analysis.executed.ipynb": `${base}.executed.ipynb`,
    "report.qmd": path,
    "report.html": `${base}.html`,
    // Stata writes its batch log in the working directory.
    "main.log": `${base.split(/[\\/]/).at(-1)}.log`,
  };
  const argv = template.argv.map((value) => replacements[value] ?? value);
  if (adapter === "notebook") {
    argv[argv.indexOf("--output") + 1] =
      `${base.split("/").at(-1)}.executed.ipynb`;
    argv.splice(
      argv.length - 1,
      0,
      "--output-dir",
      path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : ".",
    );
  }
  return {
    adapter,
    argv,
    outputs: template.outputs.map((value) => replacements[value] ?? value),
  };
}

export function declaredFileHashes(manifest: Record<string, unknown>): string {
  const files = Array.isArray(manifest.files) ? manifest.files : [];
  return JSON.stringify(
    files
      .map((file) => [
        file.path,
        file.contentHash ?? null,
        Boolean(file.missing),
        file.unavailable ?? null,
      ])
      .sort((a, b) => String(a[0]).localeCompare(String(b[0]))),
  );
}
