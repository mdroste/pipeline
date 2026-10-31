// Content and pure status derivation for the Get started sheet. Persona
// choices only select which tips and tools are shown first.

import type { AccountStatus } from "../../lib/chatgptAccountClient";
import type { GetStartedPersona } from "../../lib/getStarted";
import type { DepsReport, DepStatus, Settings } from "../../lib/types";
import type {
  DetectedToolchain,
  ToolchainKind,
  ToolchainReport,
} from "../../lib/workbench/toolchains";

export const STARTER_TEMPLATES = [
  "reference-check",
  "replication-package-readiness",
] as const;
export type StarterTemplate = (typeof STARTER_TEMPLATES)[number];

export interface PersonaGuide {
  label: string;
  description: string;
  /** Tools listed first, with a note when they were not found. Empty lists
   *  every detected tool without emphasis. */
  tools: ToolchainKind[];
  reviewHint: string;
  templates: StarterTemplate[];
  tips: string[];
}

export const PERSONAS: Record<GetStartedPersona, PersonaGuide> = {
  "stata-latex": {
    label: "Stata and LaTeX",
    description: "Do-files, .tex manuscripts, and replication packages",
    tools: ["stata"],
    reviewHint:
      "Choose the .tex file or the LaTeX project folder. Reviews read the source, so equations and structure are kept.",
    templates: ["replication-package-readiness", "reference-check"],
    tips: [
      "In a project, add a Stata do-file as a local check. Pipeline reads the log and fails the check on an error code such as r(198).",
      "LaTeX projects compile with latexmk, pdflatex, xelatex, lualatex or tectonic from your PATH, with shell escape off. Reviews of .tex source need no TeX installation.",
      "Run Replication Package Readiness on the package folder before you submit it.",
    ],
  },
  "r-word": {
    label: "R and Word",
    description: "R scripts, R Markdown or Quarto, and .docx drafts",
    tools: ["r", "quarto"],
    reviewHint: "Choose your Word (.docx) draft or a PDF.",
    templates: ["reference-check", "replication-package-readiness"],
    tips: [
      "In a project, add an R script as a local check. Pipeline records the R version and the hash of renv.lock.",
      "Quarto and R Markdown documents can be rendered as a check; list the rendered file as an expected output.",
      "Reference Check looks up each reference in Crossref before you send a draft.",
    ],
  },
  "python-notebooks": {
    label: "Python and notebooks",
    description: "Scripts, Jupyter notebooks, and virtual environments",
    tools: ["python", "jupyter"],
    reviewHint:
      "Choose a PDF, a Word file, or a folder of source files for the paper.",
    templates: ["replication-package-readiness", "reference-check"],
    tips: [
      "Pipeline finds a project's .venv interpreter and records the hashes of uv.lock, requirements.txt and similar files.",
      "A notebook check runs every cell into a new file. Your original notebook is never overwritten.",
      "Run Replication Package Readiness on a package folder before you share it.",
    ],
  },
  "word-zotero": {
    label: "Word and Zotero",
    description: "Word manuscripts and a Zotero library",
    tools: [],
    reviewHint: "Choose your Word (.docx) manuscript or a PDF.",
    templates: ["reference-check"],
    tips: [
      "Reference Check looks up each reference in Crossref for existence, matching details, retractions and corrections.",
      "With Zotero open and its local API turned on, a project's Library → Literature notes can preview and import selected collection metadata. Pipeline only reads from Zotero.",
    ],
  },
  other: {
    label: "Something else",
    description: "Show every tool and tip",
    tools: [],
    reviewHint:
      "Choose a PDF, Word file, .tex file, or folder. LaTeX source keeps equations and structure.",
    templates: ["reference-check", "replication-package-readiness"],
    tips: [
      "Open the example review first to see what a finished Review contains.",
      "Projects keep conversations, files, notes, and local checks together.",
    ],
  },
};

export function personaGuide(persona: GetStartedPersona | null): PersonaGuide {
  return PERSONAS[persona ?? "other"];
}

export const TOOL_NOTES: Record<ToolchainKind, { name: string; use: string }> =
  {
    stata: {
      name: "Stata",
      use: "Run do-files as local checks in a project. Pipeline reads the log for error codes and records the Stata version.",
    },
    r: {
      name: "R",
      use: "Run R scripts as local checks. Pipeline records the R version and the hash of renv.lock.",
    },
    python: {
      name: "Python",
      use: "Run Python scripts with the project's environment. Pipeline records the version and lockfile hashes.",
    },
    jupyter: {
      name: "Jupyter",
      use: "Run notebooks into an executed copy; the original is never overwritten.",
    },
    julia: {
      name: "Julia",
      use: "Run Julia scripts with the project's environment. Pipeline records Project.toml and Manifest.toml.",
    },
    quarto: {
      name: "Quarto",
      use: "Render Quarto or R Markdown documents as a check.",
    },
    matlab: {
      name: "MATLAB",
      use: "Run it as a general command check. Pipeline checks only the expected outputs.",
    },
  };

export const REPLICATION_NOTE =
  "A captured check can be replayed later in a fresh folder, which shows whether the results still reproduce.";

export type Tone = "ok" | "attention" | "error" | "idle";

export interface ConnectionRow {
  id: "chatgpt" | "claude" | "api" | "local";
  name: string;
  use: string;
  status: string;
  tone: Tone;
  detail?: string;
  /** Settings → Connections anchor. */
  target: string;
}

export interface ConnectionInputs {
  dependencies: DepsReport | null;
  dependenciesLoading: boolean;
  /** undefined while loading; null when no status was returned. */
  account: AccountStatus | null | undefined;
  accountError: string | null;
  /** undefined while loading; null when settings could not be read. */
  settings: Settings | null | undefined;
}

const dep = (report: DepsReport | null, name: string): DepStatus | undefined =>
  report?.deps.find((item) => item.name === name);

function chatgptRow({
  account,
  accountError,
}: ConnectionInputs): ConnectionRow {
  const row = {
    id: "chatgpt" as const,
    name: "ChatGPT",
    use: "Conversations and Reviews · your ChatGPT subscription",
    target: "workspace-provider",
  };
  if (accountError)
    return {
      ...row,
      status: "Could not check",
      tone: "error",
      detail:
        "Install the Codex command-line tool, then choose Sign in to ChatGPT in Settings.",
    };
  if (account === undefined)
    return { ...row, status: "Checking…", tone: "idle" };
  if (!account?.account)
    return {
      ...row,
      status: "Could not check",
      tone: "idle",
      detail: "Open Settings to see the ChatGPT account.",
    };
  if (account.account.status === "chatgpt")
    return { ...row, status: "Signed in", tone: "ok" };
  if (account.loginInProgress)
    return { ...row, status: "Sign-in in progress", tone: "attention" };
  return {
    ...row,
    status: "Not signed in",
    tone: "idle",
    detail: "Choose Sign in to ChatGPT in Settings.",
  };
}

function claudeRow({
  dependencies,
  dependenciesLoading,
}: ConnectionInputs): ConnectionRow {
  const row = {
    id: "claude" as const,
    name: "Claude Code",
    use: "Conversations and Reviews · your Claude subscription",
    target: "anthropic-provider",
  };
  const claude = dep(dependencies, "Claude CLI");
  if (!claude)
    return {
      ...row,
      status: dependenciesLoading ? "Checking…" : "Not yet checked",
      tone: "idle",
    };
  if (!claude.found)
    return {
      ...row,
      status: "Not installed",
      tone: "idle",
      detail: "Install Claude Code, then run claude auth login.",
    };
  if (claude.cli_auth_status === "signed_in")
    return { ...row, status: "Installed and signed in", tone: "ok" };
  if (claude.cli_auth_status === "signed_out")
    return {
      ...row,
      status: "Installed, not signed in",
      tone: "attention",
      detail: "Run claude auth login in a terminal.",
    };
  return {
    ...row,
    status: "Installed; sign-in not confirmed",
    tone: "attention",
    detail: "Run claude auth status in a terminal.",
  };
}

function apiRow({ settings }: ConnectionInputs): ConnectionRow {
  const row = {
    id: "api" as const,
    name: "API keys",
    use: "Conversations and reviews · the provider bills each use",
    target: "anthropic-provider",
  };
  if (settings === undefined)
    return { ...row, status: "Checking…", tone: "idle" };
  if (!settings)
    return {
      ...row,
      status: "Could not check",
      tone: "idle",
      detail: "Open Settings to see saved keys.",
    };
  const saved = [
    ["Anthropic", settings.anthropic_api_key],
    ["OpenAI", settings.openai_api_key],
    ["Google", settings.google_api_key],
  ]
    .filter(([, key]) => key?.trim())
    .map(([label]) => label);
  return saved.length
    ? { ...row, status: `Saved: ${saved.join(", ")}`, tone: "ok" }
    : { ...row, status: "None saved", tone: "idle" };
}

function localRow({
  dependencies,
  dependenciesLoading,
}: ConnectionInputs): ConnectionRow {
  const row = {
    id: "local" as const,
    name: "Local model server",
    use: "Conversations and reviews · a server you run, such as Ollama",
    target: "local-provider",
  };
  const local = dep(dependencies, "Local LLM server");
  if (!local)
    return {
      ...row,
      status: dependenciesLoading ? "Checking…" : "Not yet checked",
      tone: "idle",
    };
  return local.found
    ? { ...row, status: "Running", tone: "ok" }
    : { ...row, status: "Not running", tone: "idle" };
}

export function connectionRows(inputs: ConnectionInputs): ConnectionRow[] {
  return [
    chatgptRow(inputs),
    claudeRow(inputs),
    apiRow(inputs),
    localRow(inputs),
  ];
}

export function connectionSummary(rows: ConnectionRow[]) {
  const ready = (id: ConnectionRow["id"]) =>
    rows.some((row) => row.id === id && row.tone === "ok");
  const conversations =
    ready("chatgpt") || ready("claude") || ready("api") || ready("local");
  if (conversations) return "You can start Conversations and Reviews now.";
  return "Connect at least one provider before your first Review. The example review needs no connection.";
}

export interface PdfRow {
  name: string;
  status: string;
  tone: Tone;
  detail: string;
}

export function pdfRows(report: DepsReport | null): PdfRow[] {
  if (!report) return [];
  const poppler = ["pdftoppm", "pdftotext"].map((name) => dep(report, name));
  const full = dep(report, "PaddleOCR-VL Full Parser");
  const rows: PdfRow[] = [];
  if (poppler.some(Boolean))
    rows.push(
      poppler.every((item) => item?.found)
        ? {
            name: "PDF text tools",
            status: "Ready",
            tone: "ok",
            detail:
              "Bundled with Pipeline. Extracts text locally; equations and tables may be incomplete.",
          }
        : {
            name: "PDF text tools",
            status: "Unavailable",
            tone: "error",
            detail: "The bundled PDF tools are missing. Reinstall Pipeline.",
          },
    );
  if (full)
    rows.push(
      full.found
        ? {
            name: "Full Parser",
            status: "Installed",
            tone: "ok",
            detail:
              "Reads text, layout, formulas, tables, and figures on this computer.",
          }
        : {
            name: "Full Parser",
            status: "Not installed",
            tone: full.required ? "attention" : "idle",
            detail:
              "Optional. Reads formulas, tables, and figures on this computer; needs several gigabytes. Install it from Settings → Reviews → PDF extraction.",
          },
    );
  return rows;
}

export interface ToolGroups {
  /** Found tools the persona asked about, in persona order. */
  featured: DetectedToolchain[];
  /** Persona tools that were not found. */
  missing: ToolchainKind[];
  others: DetectedToolchain[];
}

export function groupTools(
  report: ToolchainReport,
  persona: GetStartedPersona | null,
): ToolGroups {
  const wanted = personaGuide(persona).tools;
  const rank = (tool: DetectedToolchain) => {
    const index = wanted.indexOf(tool.kind);
    return index < 0 ? wanted.length : index;
  };
  const featured = report.tools
    .filter((tool) => wanted.includes(tool.kind))
    .sort((a, b) => rank(a) - rank(b));
  return {
    featured,
    missing: wanted.filter(
      (kind) => !report.tools.some((tool) => tool.kind === kind),
    ),
    others: report.tools.filter((tool) => !wanted.includes(tool.kind)),
  };
}
