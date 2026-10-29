// Execution adapters, their argument templates, and detected research tools.
// Mirrors gui/src-tauri/src/workbench/research/execution/{adapters,toolchains}.rs.

export type ExecutionAdapter =
  | "command"
  | "latex"
  | "stata"
  | "r"
  | "python"
  | "julia"
  | "notebook"
  | "quarto";

export type ToolchainKind =
  "stata" | "r" | "python" | "julia" | "quarto" | "jupyter" | "matlab";

export interface DetectedToolchain {
  kind: ToolchainKind;
  adapter: ExecutionAdapter | null;
  label: string;
  path: string;
  source: "standardLocation" | "path" | "projectEnvironment";
  version: string | null;
  versionText: string | null;
  versionSource: string;
  sha256: string | null;
  sizeBytes: number | null;
  note: string | null;
}

export interface ToolchainReport {
  platform: "macos" | "windows" | "linux" | string;
  tools: DetectedToolchain[];
  project: {
    root: string;
    files: {
      path: string;
      ecosystem: string;
      sha256: string | null;
      sizeBytes: number;
    }[];
    pythonEnvironments: {
      path: string;
      interpreter: string | null;
      version: string | null;
      configurationSha256: string | null;
    }[];
    stataAdoFolder: boolean;
  } | null;
  boundary: string;
}

export const ADAPTER_OPTIONS: { value: ExecutionAdapter; label: string }[] = [
  { value: "command", label: "Command" },
  { value: "latex", label: "LaTeX" },
  { value: "stata", label: "Stata do-file" },
  { value: "r", label: "R script" },
  { value: "python", label: "Python script" },
  { value: "julia", label: "Julia script" },
  { value: "notebook", label: "Jupyter notebook" },
  { value: "quarto", label: "Quarto or R Markdown document" },
];

export const ADAPTER_HELP: Record<ExecutionAdapter, string> = {
  command:
    "Runs any program with the arguments you list. Pipeline checks only the expected outputs.",
  latex:
    "Compiles a LaTeX document with latexmk, pdflatex, xelatex, lualatex or tectonic. Shell escape stays off. List the PDF among expected outputs.",
  stata:
    "Runs a do-file with your installed Stata in batch mode (-b on macOS and Linux, /e on Windows). List the log, such as main.log, among expected outputs; a log containing an error code such as r(198) fails the check. To use a shell alias instead, enter /bin/zsh -lic followed by the alias command as a custom launcher.",
  r: "Runs an R script with Rscript. Add --vanilla to ignore personal startup files; leave it out when the project uses renv, which starts through .Rprofile. Pipeline records the R version and the hash of renv.lock.",
  python:
    "Runs a Python script. Choose the project's virtual environment interpreter, such as .venv/bin/python, when there is one. Pipeline records the Python version and the hashes of uv.lock, requirements.txt, environment.yml and similar files.",
  julia:
    "Runs a Julia script. Use --project=. to load the project's environment. Pipeline records the Julia version and the hashes of Project.toml and Manifest.toml.",
  notebook:
    "Runs every cell of a notebook with jupyter nbconvert and saves the executed copy under a new name. Your original notebook is never overwritten. List the executed copy among expected outputs.",
  quarto:
    "Renders a Quarto (.qmd) or R Markdown (.Rmd) document with quarto render. List the rendered file, such as report.html, among expected outputs.",
};

interface Template {
  argv: string[];
  outputs: string[];
}

export function adapterTemplate(
  adapter: ExecutionAdapter,
  platform: string,
): Template {
  const windows = platform === "windows";
  switch (adapter) {
    case "command":
      return { argv: ["/usr/bin/true"], outputs: [] };
    case "latex":
      return { argv: ["latexmk", "-pdf", "main.tex"], outputs: ["main.pdf"] };
    case "stata":
      return {
        argv: windows
          ? ["StataMP-64.exe", "/e", "do", "main.do"]
          : ["stata-mp", "-b", "do", "main.do"],
        outputs: ["main.log"],
      };
    case "r":
      return { argv: ["Rscript", "analysis.R"], outputs: [] };
    case "python":
      return {
        argv: [windows ? "python" : "python3", "analysis.py"],
        outputs: [],
      };
    case "julia":
      return { argv: ["julia", "--project=.", "analysis.jl"], outputs: [] };
    case "notebook":
      return {
        argv: [
          "jupyter",
          "nbconvert",
          "--execute",
          "--to",
          "notebook",
          "--output",
          "analysis.executed.ipynb",
          "analysis.ipynb",
        ],
        outputs: ["analysis.executed.ipynb"],
      };
    case "quarto":
      return {
        argv: ["quarto", "render", "report.qmd", "--to", "html"],
        outputs: ["report.html"],
      };
  }
}

const PLATFORMS = ["macos", "windows", "linux"];

/** Whether text is still an unedited template (any platform) for an adapter. */
export function isUntouchedTemplate(
  value: string,
  adapter: ExecutionAdapter,
  part: "argv" | "outputs",
): boolean {
  return PLATFORMS.some((platform) => {
    const template = adapterTemplate(adapter, platform);
    return part === "argv"
      ? value.trim() === JSON.stringify(template.argv) ||
          value.trim() === JSON.stringify(template.argv, null, 2)
      : value.trim() === template.outputs.join("\n");
  });
}

const KINDS: Record<ExecutionAdapter, ToolchainKind[]> = {
  command: ["stata", "r", "python", "julia", "quarto", "jupyter", "matlab"],
  latex: [],
  stata: ["stata"],
  r: ["r"],
  python: ["python"],
  julia: ["julia"],
  notebook: ["jupyter"],
  quarto: ["quarto"],
};

export function toolsForAdapter(
  report: ToolchainReport,
  adapter: ExecutionAdapter,
): DetectedToolchain[] {
  const kinds = KINDS[adapter];
  return report.tools.filter((tool) => kinds.includes(tool.kind));
}

function parseArgv(value: string): string[] | null {
  try {
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed) &&
      parsed.length > 0 &&
      parsed.every((item) => typeof item === "string")
      ? parsed
      : null;
  } catch {
    return null;
  }
}

export const STATA_CLEANUP_NOTICE =
  "Stata stopped before it finished. Check that no Stata process is still running. If your launcher changes system settings, such as the clock, confirm it restored them before continuing.";

/** Whether a Stata command runs through a shell alias (custom launcher). */
export function stataUsesLauncher(command: string[]): boolean {
  return (
    command.length === 3 &&
    command[0].startsWith("/") &&
    /\/(zsh|bash)$/.test(command[0])
  );
}

/**
 * Put a detected program into the argument list. An unedited template or an
 * unusable list is replaced by the adapter's template; otherwise only the
 * program changes, so the researcher's script and options stay as typed.
 */
export function withDetectedProgram(
  value: string,
  adapter: ExecutionAdapter,
  tool: DetectedToolchain,
  platform: string,
): string {
  const current = parseArgv(value);
  const keep =
    current &&
    !isUntouchedTemplate(value, adapter, "argv") &&
    !isUntouchedTemplate(value, "command", "argv") &&
    !(adapter === "stata" && stataUsesLauncher(current));
  const argv = keep ? [...current] : adapterTemplate(adapter, platform).argv;
  if (adapter === "command" && !keep) argv.splice(1);
  argv[0] = tool.path;
  return JSON.stringify(argv);
}

export function guessPlatform(userAgent = navigator.userAgent): string {
  if (userAgent.includes("Windows")) return "windows";
  if (userAgent.includes("Mac")) return "macos";
  return "linux";
}
