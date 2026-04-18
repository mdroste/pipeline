import { useEffect, useState } from "react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { getVersion } from "@tauri-apps/api/app";

interface Props {
  initialTab?: "help" | "about";
  onClose: () => void;
}

export default function AboutPage({ initialTab = "help", onClose }: Props) {
  const tab = initialTab;

  return (
    <div className="p-8 max-w-3xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-lg font-bold text-gray-900 dark:text-gray-100">
          {tab === "help" ? "Help" : "About"}
        </h2>
        <button
          onClick={onClose}
          className="text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
        >
          Back
        </button>
      </div>

      {tab === "help" ? <HelpContent /> : <AboutContent />}
    </div>
  );
}

function HelpContent() {
  return (
    <div className="space-y-8">
      {/* Overview */}
      <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
        Pipeline generates structured feedback on academic papers by running
        multiple independent analysis passes in parallel, then consolidating
        the results into a single prioritized issue list.
      </p>

      {/* Pipeline stages */}
      <Section title="Pipeline stages">
        <div className="grid gap-3">
          <StageCard
            number="1"
            title="Extract text"
            description="Reads the paper from LaTeX source, PDF via marker, or PDF via pdftotext. LaTeX is preferred because equations come through clean."
          />
          <StageCard
            number="2"
            title="Build orientation map"
            description="One LLM call produces a structured inventory of the paper: sections, formal results, tables, notation, and stated contribution. This anchors all subsequent passes."
          />
          <StageCard
            number="3"
            title="Referee passes"
            description="Multiple passes run in parallel, each with its own prompt and clean context. Each produces a list of issues with severity, location, and specific critique."
          />
          <StageCard
            number="4"
            title="Consolidation"
            description="A final pass reads all referee outputs and produces a single deduplicated issue list ordered by severity, with cross-references to which passes flagged each issue."
          />
        </div>
      </Section>

      {/* Profiles */}
      <Section title="Profiles">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed mb-3">
          Profiles are named configurations of referee passes and post-processing
          steps. Switch between them from the Pipeline page.
        </p>
        <div className="grid gap-2">
          <ProfileCard
            name="Deep Review"
            description="All 5 referees with feedback validation. The default."
          />
          <ProfileCard
            name="Quick Review"
            description="Contribution + Internal Consistency only. Fast two-pass analysis."
          />
          <ProfileCard
            name="Empirical"
            description="Tailored for empirical papers. Drops Technical Correctness, adds web search to Contribution."
          />
        </div>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-2">
          You can create, duplicate, rename, and delete custom profiles.
          Export and import profiles to share configurations with others.
        </p>
      </Section>

      {/* Customization */}
      <Section title="Customizing the pipeline">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed mb-3">
          Click <span className="font-medium text-gray-700">Customize prompts & add referees</span> in
          the sidebar to open the pipeline editor, where you can:
        </p>
        <ul className="space-y-1.5">
          <CheckItem text="Edit any referee's prompt" />
          <CheckItem text="Add new referee passes with custom instructions" />
          <CheckItem text="Reorder passes and toggle web search" />
          <CheckItem text="Edit the consolidation prompt or add post-processing steps" />
          <CheckItem text="Export and import individual items, profiles, or full backups" />
        </ul>
      </Section>

      {/* Text extraction */}
      <Section title="Text extraction">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed mb-3">
          Three tiers, in order of preference:
        </p>
        <div className="space-y-2">
          <TierCard
            tier="Best"
            tierColor="text-green-700 bg-green-50"
            title="LaTeX source"
            description="Provide a .tex file for clean equations and tables. Resolves \input{} includes automatically."
          />
          <TierCard
            tier="Good"
            tierColor="text-blue-700 bg-blue-50"
            title="marker-pdf"
            description="Converts PDF to markdown with equations preserved. Requires marker-pdf to be installed."
          />
          <TierCard
            tier="Fallback"
            tierColor="text-amber-700 bg-amber-50"
            title="pdftotext"
            description="Equations will be garbled. Technical findings get confidence warnings."
          />
        </div>
      </Section>

      {/* Revision tracking */}
      <Section title="Revision tracking">
        <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
          Reports are saved to <code className="text-xs bg-gray-100 px-1.5 py-0.5 rounded font-mono">~/.pipeline/history/</code> keyed
          by a hash of the paper content. When you run with the diff option on a
          revised version, the pipeline compares against the prior report and
          produces a structured diff of addressed, remaining, and new issues.
        </p>
      </Section>

      {/* Requirements */}
      <Section title="Requirements">
        <div className="space-y-2">
          <ReqCard
            name="Claude Code"
            tag="Required"
            tagColor="text-red-700 bg-red-50"
            description="All LLM calls go through claude -p."
          />
          <ReqCard
            name="poppler (pdftoppm + pdftotext)"
            tag="Bundled"
            tagColor="text-green-700 bg-green-50"
            description="Bundled with Pipeline. Used by the LLM Read tool to render PDFs and as a text-extraction fallback."
          />
          <ReqCard
            name="marker-pdf"
            tag="Optional"
            tagColor="text-gray-600 bg-gray-100"
            description="For better PDF equation extraction."
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

// --- About page ---

function AboutContent() {
  const [version, setVersion] = useState<string>("");

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion(""));
  }, []);

  return (
    <div className="flex items-center justify-center py-12">
      <div className="text-center">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-gray-100 mb-8">Pipeline</h2>

        <div className="space-y-4 text-sm text-gray-700 dark:text-gray-300">
          <div>
            <p className="text-gray-500 dark:text-gray-400 text-xs uppercase tracking-wider mb-1">
              Version
            </p>
            <p className="font-mono">{version || "—"}</p>
          </div>
          <div>
            <p className="text-gray-500 dark:text-gray-400 text-xs uppercase tracking-wider mb-1">
              Author
            </p>
            <p>Michael Droste</p>
          </div>
          <div>
            <p className="text-gray-500 dark:text-gray-400 text-xs uppercase tracking-wider mb-1">
              Source
            </p>
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
          </div>
          <div>
            <p className="text-gray-500 dark:text-gray-400 text-xs uppercase tracking-wider mb-1">
              License
            </p>
            <p>MIT</p>
          </div>
          <div>
            <p className="text-gray-500 dark:text-gray-400 text-xs uppercase tracking-wider mb-1">
              Bundled software
            </p>
            <a
              href="https://github.com/mdroste/pipeline/blob/main/THIRD_PARTY_LICENSES.md"
              onClick={(e) => {
                e.preventDefault();
                openUrl("https://github.com/mdroste/pipeline/blob/main/THIRD_PARTY_LICENSES.md");
              }}
              className="text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300 hover:underline cursor-pointer"
            >
              Third-party licenses
            </a>
          </div>
        </div>
      </div>
    </div>
  );
}
