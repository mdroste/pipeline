import { useEffect, useState } from "react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { getVersion } from "@tauri-apps/api/app";

interface Props {
  onClose: () => void;
  showBack?: boolean;
}

export default function AboutPage({ onClose, showBack = true }: Props) {
  return (
    <div className="p-8 max-w-3xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-lg font-bold text-gray-900 dark:text-gray-100">
          Help
        </h2>
        {showBack && (
          <button
            onClick={onClose}
            className="text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
          >
            Back
          </button>
        )}
      </div>

      <HelpContent />
      <AboutFooter />
    </div>
  );
}

function HelpContent() {
  return (
    <div className="space-y-8">
      {/* Overview */}
      <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
        Pipeline runs a set of AI-assisted steps as a workflow. A step can
        examine part of an input, check a specific kind of problem, or combine
        earlier findings. You can use a built-in workflow or create one for a
        document, a folder of files, or a task that needs no input.
      </p>

      {/* Pipeline stages */}
      <Section title="How a run works">
        <div className="grid gap-3">
          <StageCard
            number="1"
            title="Choose a workflow and input"
            description="On New run, select a workflow and the document or folder it should examine. A workflow can also run from its instructions alone."
          />
          <StageCard
            number="2"
            title="Prepare the material"
            description="Pipeline extracts document text or inventories a folder. If the workflow uses an orientation map, it also creates a short guide to the material so later steps can find relevant sections, files, tables, and other details."
          />
          <StageCard
            number="3"
            title="Run the workflow"
            description="Pipeline starts each step after its prerequisites finish. Steps that do not depend on one another can run at the same time. Each step receives only the input and earlier results allowed by the workflow."
          />
          <StageCard
            number="4"
            title="Review the result"
            description="Later steps can combine overlapping findings into one report. When a workflow produces structured issues, you can accept, reject, or annotate them. The Sources tab contains the material saved with the run."
          />
        </div>
      </Section>

      {/* Profiles */}
      <Section title="Profiles">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed mb-3">
          A profile saves a complete workflow: its steps, prompts, and settings.
          Choose a profile on the New run screen. Pipeline includes these
          profiles:
        </p>
        <div className="grid gap-2">
          <ProfileCard
            name="Paper Review (Full)"
            description="Five focused reviews of a paper, followed by a consolidated report. It also includes an optional validation step."
          />
          <ProfileCard
            name="Paper Review (Quick)"
            description="Two broad review passes followed by a consolidated report."
          />
          <ProfileCard
            name="Codebase Review"
            description="Seven passes covering correctness, security, design, concurrency, error handling, performance, and tests, followed by consolidation and verification."
          />
          <ProfileCard
            name="Replication Package Audit"
            description="Checks a replication package the way a journal data editor would: exhibit completeness, code–paper consistency, portability, and data documentation."
          />
          <ProfileCard
            name="Grant Proposal Review"
            description="Reviews a proposal's aims and novelty, feasibility, readability, and internal consistency. The aims step can search the web."
          />
        </div>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-2">
          You can also create, duplicate, import, and export profiles.
        </p>
      </Section>

      {/* Customization */}
      <Section title="Customize a workflow">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed mb-3">
          Open <span className="font-medium text-gray-700 dark:text-gray-300">Workflows</span> from
          the left navigation to edit the active profile. You can:
        </p>
        <ul className="space-y-1.5">
          <CheckItem text="Add, remove, reorder, or rewrite steps" />
          <CheckItem text="Choose which steps must finish first and which earlier results a step may use" />
          <CheckItem text="Choose a provider and model, or run the same step with several providers" />
          <CheckItem text="Allow web search on supported providers" />
          <CheckItem text="Add conditions, repeated file checks, named inputs, and values entered before a run" />
          <CheckItem text="Set the input type and PDF extraction method" />
          <CheckItem text="Import or export a step, a profile, or all profiles and settings" />
        </ul>
      </Section>

      <Section title="Run several documents">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
          Open <span className="font-medium text-gray-700 dark:text-gray-300">Batch</span> to
          apply the active document workflow to a list of files, one at a time.
          You can add individual files, add every supported document in a folder,
          or watch a folder and run new files as they arrive. Batch and folder
          watching are available for document-input workflows.
        </p>
      </Section>

      {/* Text extraction */}
      <Section title="Document extraction">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed mb-3">
          Pipeline prepares a text version of each document before the workflow
          begins. LaTeX and Word files are read directly. For PDFs, choose a
          method in the workflow or inherit the choice from Settings.
        </p>
        <div className="space-y-2">
          <TierCard
            tier="Source"
            tierColor="text-green-700 bg-green-50"
            title="LaTeX source"
            description="Reads a .tex file directly, including safe local files referenced with \input{}. If a matching PDF is available, Pipeline also keeps its page images."
          />
          <TierCard
            tier="Built in"
            tierColor="text-green-700 bg-green-50"
            title="Word (.docx)"
            description="Reads the Word file directly and keeps equations, table structure, and embedded images."
          />
          <TierCard
            tier="Provider"
            tierColor="text-blue-700 bg-blue-50"
            title="LLM PDF extraction"
            description="Uses the active provider to transcribe the PDF in page ranges. This adds model calls, but usually preserves equations and layout better than plain-text extraction. Pipeline stops if its page checks fail."
          />
          <TierCard
            tier="Local add-on"
            tierColor="text-blue-700 bg-blue-50"
            title="PaddleOCR-VL Full Parser"
            description="Uses the optional local engine to recover reading order, document regions, headings, formulas, and tables. Install and configure it under Settings → PDF Extraction."
          />
          <TierCard
            tier="Local add-on"
            tierColor="text-blue-700 bg-blue-50"
            title="PaddleOCR-VL Fast"
            description="Uses the same managed local model for faster page transcription, with fewer structural details than the Full Parser."
          />
          <TierCard
            tier="Built in"
            tierColor="text-amber-700 bg-amber-50"
            title="pdftotext"
            description="Extracts plain text locally with bundled PDF tools. It is quick and works well for simple text, but equations and complex layouts may not survive."
          />
        </div>
      </Section>

      {/* Results */}
      <Section title="Results, history, and exports">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
          Finished runs open in a workspace with Report, Issues, and Sources
          tabs. You can save the report as Markdown or PDF, or export the whole
          run with its reports, source material, page images, figures, and logs.
          History lets you rename, tag, delete, resume, rerun, and compare saved
          runs. Automatic revision reconciliation in Settings can add a model-written
          comparison when Pipeline finds an earlier run for the same document.
        </p>
      </Section>

      <Section title="Data and privacy">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
          Each step receives only the source material and earlier results allowed
          by its workflow. When you use a cloud provider, that material is sent
          through the provider's CLI or API and is subject to the provider's
          account, plan, and data-use terms. If a workflow enables web search and
          the provider supports it, search queries are also sent to an external
          service. Pipeline stores run records and artifacts on this computer under{" "}
          <code className="text-xs bg-gray-100 px-1.5 py-0.5 rounded font-mono">
            ~/.pipeline/runs/
          </code>{" "}
          until you delete them or set a retention limit in Settings.
        </p>
      </Section>

      {/* Requirements */}
      <Section title="Requirements">
        <div className="space-y-2">
          <ReqCard
            name="An AI provider"
            tag="Required"
            tagColor="text-red-700 bg-red-50"
            description="Sign in with the Claude Code, Codex, or Gemini CLI; add a supported API key; or connect an OpenAI-compatible local server. A CLI subscription can work without a separate API key. Use the status button in the lower-left corner to check setup."
          />
          <ReqCard
            name="PDF tools"
            tag="Included"
            tagColor="text-green-700 bg-green-50"
            description="Pipeline includes the tools it needs to render PDF pages, check extraction completeness, and run pdftotext. There is nothing else to install."
          />
        </div>
      </Section>
    </div>
  );
}

// --- Help page components ---

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100 uppercase tracking-wide mb-3">
        {title}
      </h3>
      {children}
    </div>
  );
}

function StageCard({ number, title, description }: { number: string; title: string; description: string }) {
  return (
    <div className="flex gap-3 items-start p-3 rounded-lg bg-gray-50 dark:bg-gray-800 border border-gray-100 dark:border-gray-700">
      <span className="w-6 h-6 rounded-full bg-gray-900 text-white text-xs font-bold flex items-center justify-center shrink-0 mt-0.5">
        {number}
      </span>
      <div>
        <p className="text-sm font-medium text-gray-800 dark:text-gray-200">{title}</p>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5 leading-relaxed">{description}</p>
      </div>
    </div>
  );
}

function ProfileCard({ name, description }: { name: string; description: string }) {
  return (
    <div className="flex items-baseline gap-2 p-2.5 rounded-lg bg-gray-50 dark:bg-gray-800 border border-gray-100 dark:border-gray-700">
      <span className="text-sm font-medium text-gray-800 dark:text-gray-200 shrink-0">{name}</span>
      <span className="text-xs text-gray-500 dark:text-gray-400">{description}</span>
    </div>
  );
}

function CheckItem({ text }: { text: string }) {
  return (
    <li className="flex items-start gap-2 text-sm text-gray-600">
      <svg className="w-4 h-4 text-green-700 dark:text-green-400 mt-0.5 shrink-0" viewBox="0 0 20 20" fill="currentColor">
        <path fillRule="evenodd" d="M16.704 4.153a.75.75 0 01.143 1.052l-8 10.5a.75.75 0 01-1.127.075l-4.5-4.5a.75.75 0 011.06-1.06l3.894 3.893 7.48-9.817a.75.75 0 011.05-.143z" clipRule="evenodd" />
      </svg>
      {text}
    </li>
  );
}

function TierCard({ tier, tierColor, title, description }: { tier: string; tierColor: string; title: string; description: string }) {
  return (
    <div className="flex items-start gap-3 p-3 rounded-lg bg-gray-50 dark:bg-gray-800 border border-gray-100 dark:border-gray-700">
      <span className={`text-[10px] font-semibold uppercase px-1.5 py-0.5 rounded shrink-0 mt-0.5 ${tierColor}`}>
        {tier}
      </span>
      <div>
        <p className="text-sm font-medium text-gray-800 dark:text-gray-200">{title}</p>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5 leading-relaxed">{description}</p>
      </div>
    </div>
  );
}

function ReqCard({ name, tag, tagColor, description }: { name: string; tag: string; tagColor: string; description: string }) {
  return (
    <div className="flex items-start gap-3 p-3 rounded-lg bg-gray-50 dark:bg-gray-800 border border-gray-100 dark:border-gray-700">
      <span className={`text-[10px] font-semibold uppercase px-1.5 py-0.5 rounded shrink-0 mt-0.5 ${tagColor}`}>
        {tag}
      </span>
      <div>
        <p className="text-sm font-medium text-gray-800 dark:text-gray-200">{name}</p>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5 leading-relaxed">{description}</p>
      </div>
    </div>
  );
}

// --- About footer ---

function AboutFooter() {
  const [version, setVersion] = useState<string>("");
  const [linkError, setLinkError] = useState<string>("");

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion(""));
  }, []);

  const openExternal = (event: React.MouseEvent<HTMLAnchorElement>, url: string) => {
    event.preventDefault();
    setLinkError("");
    void openUrl(url).catch((error) => {
      const detail = error instanceof Error ? error.message : String(error);
      setLinkError(
        `Pipeline could not open the link in your browser${detail ? `: ${detail}` : "."} You can copy the address from the link instead.`,
      );
    });
  };

  return (
    <div className="mt-10 pt-6 border-t border-gray-200 dark:border-gray-800">
      <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
        <span className="font-medium text-gray-700 dark:text-gray-300">
          Pipeline{version ? ` v${version}` : ""}
        </span>
        {" · "}Michael Droste{" · "}MIT license{" · "}
        <a
          href="https://github.com/mdroste/pipeline"
          onClick={(event) => openExternal(event, "https://github.com/mdroste/pipeline")}
          className="text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300 hover:underline cursor-pointer"
        >
          github.com/mdroste/pipeline
        </a>
        {" · "}
        <a
          href="https://github.com/mdroste/pipeline/blob/main/THIRD_PARTY_LICENSES.md"
          onClick={(event) =>
            openExternal(
              event,
              "https://github.com/mdroste/pipeline/blob/main/THIRD_PARTY_LICENSES.md",
            )
          }
          className="text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300 hover:underline cursor-pointer"
        >
          third-party licenses
        </a>
      </p>
      {linkError && (
        <p
          role="alert"
          className="mt-2 text-xs leading-relaxed text-red-700 dark:text-red-300"
        >
          {linkError}
        </p>
      )}
    </div>
  );
}
