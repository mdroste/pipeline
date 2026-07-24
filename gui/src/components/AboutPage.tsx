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
        Pipeline chains LLM prompts into multi-step workflows. Each step is a
        prompt. Steps run in parallel or one after another, and later steps can
        build on earlier results. The built-in workflows review academic papers
        in multiple passes, but you can write workflows for any document, a
        folder of files, or a task with no input at all.
      </p>

      {/* Pipeline stages */}
      <Section title="How a run works">
        <div className="grid gap-3">
          <StageCard
            number="1"
            title="Read the input"
            description="Documents are converted to text. Folders are indexed so steps can open individual files as needed. Some workflows take no input."
          />
          <StageCard
            number="2"
            title="Build an orientation map (optional)"
            description="Before the first step, one LLM call catalogs the input. For a paper, this lists the sections, results, tables, and notation. Steps use the map to stay grounded — for example, to avoid flagging something that is covered in the appendix. You can turn this off in the profile."
          />
          <StageCard
            number="3"
            title="Run the steps"
            description="Steps run from top to bottom. Steps marked parallel run at the same time, each with a fresh context. Steps marked sequential wait for all steps above them and can use their outputs."
          />
          <StageCard
            number="4"
            title="Merge and consolidate"
            description="If a step ran on more than one model, such as Claude and Gemini, Pipeline merges the outputs into one. A final step usually removes duplicate findings and sorts the rest by severity."
          />
        </div>
      </Section>

      {/* Profiles */}
      <Section title="Profiles">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed mb-3">
          A profile saves a complete workflow: its steps, prompts, and settings.
          Switch profiles from the sidebar. Built in:
        </p>
        <div className="grid gap-2">
          <ProfileCard
            name="Paper Review (Full)"
            description="Five parallel review steps on a paper, then consolidation and validation. The default."
          />
          <ProfileCard
            name="Paper Review (Quick)"
            description="Two review steps and consolidation. Fast."
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
            description="Panel-style review of a proposal: aims and novelty (with web search), feasibility, readability, and internal consistency."
          />
        </div>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-2">
          You can create your own profiles for any task, and export them to
          share with others.
        </p>
      </Section>

      {/* Customization */}
      <Section title="Customize a workflow">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed mb-3">
          Click <span className="font-medium text-gray-700">Customize pipeline steps</span> in
          the sidebar to open the editor. From there you can:
        </p>
        <ul className="space-y-1.5">
          <CheckItem text="Edit any step's prompt, or add new steps" />
          <CheckItem text="Make a step parallel or sequential, and reorder steps by dragging" />
          <CheckItem text="Give a step tools (Read, WebSearch) or its own model" />
          <CheckItem text="Run one step on several models and merge the results" />
          <CheckItem text="Set the profile's input type and PDF extraction method" />
          <CheckItem text="Export and import steps, profiles, or a full backup" />
        </ul>
      </Section>

      {/* Text extraction */}
      <Section title="Document extraction">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed mb-3">
          Pipeline converts document inputs to text before the first step runs.
          From best to worst:
        </p>
        <div className="space-y-2">
          <TierCard
            tier="Best"
            tierColor="text-green-700 bg-green-50"
            title="LaTeX source"
            description="Select a .tex file to get exact equations and tables. Files referenced with \input{} are included automatically."
          />
          <TierCard
            tier="Native"
            tierColor="text-green-700 bg-green-50"
            title="Word (.docx)"
            description="Reads OOXML directly, retaining Word equation markup, table grids, and embedded figure images."
          />
          <TierCard
            tier="Default"
            tierColor="text-blue-700 bg-blue-50"
            title="LLM extraction"
            description="The model reads the PDF and rewrites it as Markdown, with equations in LaTeX. Works out of the box."
          />
          <TierCard
            tier="Optional"
            tierColor="text-blue-700 bg-blue-50"
            title="marker-pdf"
            description="Converts PDFs locally and preserves equations. Requires a separate install."
          />
          <TierCard
            tier="Fallback"
            tierColor="text-amber-700 bg-amber-50"
            title="pdftotext"
            description="Plain text only. Equations come out garbled, so technical findings carry a warning."
          />
        </div>
      </Section>

      {/* Results */}
      <Section title="Results and revisions">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
          Every run is saved to <code className="text-xs bg-gray-100 px-1.5 py-0.5 rounded font-mono">~/.pipeline/runs/</code>,
          including a readable document, canonical DocumentBundle, and visual page and figure assets that can be inspected in the run viewer.
          It also retains the report and any files the steps produced. Browse them
          in the artifact explorer after the run finishes. To compare revised
          papers automatically, enable revision reconciliation in Settings:
          Pipeline finds the earlier report for the same paper and lists which
          issues were addressed, which remain, and which are new.
        </p>
      </Section>

      {/* Requirements */}
      <Section title="Requirements">
        <div className="space-y-2">
          <ReqCard
            name="An LLM provider"
            tag="Required"
            tagColor="text-red-700 bg-red-50"
            description="Sign in to the Claude Code, Codex, or Gemini CLI, or enter an API key in Settings. A subscription plan works through the CLI. No API key needed."
          />
          <ReqCard
            name="poppler (pdftoppm + pdftotext)"
            tag="Bundled"
            tagColor="text-green-700 bg-green-50"
            description="Included with Pipeline. Renders PDFs for the model and provides the pdftotext fallback."
          />
          <ReqCard
            name="marker-pdf"
            tag="Optional"
            tagColor="text-gray-600 bg-gray-100"
            description="Install it for local PDF equation extraction."
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
      <svg className="w-4 h-4 text-green-500 mt-0.5 shrink-0" viewBox="0 0 20 20" fill="currentColor">
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

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion(""));
  }, []);

  return (
    <div className="mt-10 pt-6 border-t border-gray-200 dark:border-gray-800">
      <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
        <span className="font-medium text-gray-700 dark:text-gray-300">
          Pipeline{version ? ` v${version}` : ""}
        </span>
        {" · "}Michael Droste{" · "}MIT license{" · "}
        <a
          href="https://github.com/mdroste/pipeline"
          onClick={(e) => {
            e.preventDefault();
            openUrl("https://github.com/mdroste/pipeline");
          }}
          className="text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300 hover:underline cursor-pointer"
        >
          github.com/mdroste/pipeline
        </a>
        {" · "}
        <a
          href="https://github.com/mdroste/pipeline/blob/main/THIRD_PARTY_LICENSES.md"
          onClick={(e) => {
            e.preventDefault();
            openUrl("https://github.com/mdroste/pipeline/blob/main/THIRD_PARTY_LICENSES.md");
          }}
          className="text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300 hover:underline cursor-pointer"
        >
          third-party licenses
        </a>
      </p>
    </div>
  );
}
