// Content and pure status derivation for the Get started sheet.

import type { AccountStatus } from "../../lib/chatgptAccountClient";
import type { DepsReport, DepStatus, Settings } from "../../lib/types";
import type { ToolchainKind } from "../../lib/workbench/toolchains";

export const STARTER_TEMPLATES = [
  "reference-check",
  "replication-package-readiness",
] as const;
export type StarterTemplate = (typeof STARTER_TEMPLATES)[number];

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
