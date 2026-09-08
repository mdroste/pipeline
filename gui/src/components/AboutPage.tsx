import { useEffect, useRef, useState } from "react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import FlappyBirdGame from "./FlappyBirdGame";
import type { AppPage } from "./NavRail";
import type { ProfileSummary } from "../lib/types";
import { REPORT_WORKSPACE_TABS } from "../lib/productMetadata";

interface Props {
  onClose: () => void;
  showBack?: boolean;
  /** Lets Help sections jump to the page they describe; links are hidden without it. */
  onNavigate?: (page: AppPage) => void;
  /** Jumps to the PaddleOCR-VL install card in Settings → Reviews → PDF Extraction. */
  onOpenPdfSettings?: () => void;
  /** Opens and scrolls to a section on arrival (used by the run-setup privacy link). */
  initialSection?: "privacy";
}

const KONAMI_CODE = [
  "ArrowUp",
  "ArrowUp",
  "ArrowDown",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "ArrowLeft",
  "ArrowRight",
  "b",
  "a",
  "Enter",
] as const;

function useKonamiCode(onComplete: () => void) {
  const callbackRef = useRef(onComplete);
  callbackRef.current = onComplete;

  useEffect(() => {
    let position = 0;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.repeat || event.altKey || event.ctrlKey || event.metaKey) return;
      const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
      if (key === KONAMI_CODE[position]) {
        position += 1;
        if (position === KONAMI_CODE.length) {
          event.preventDefault();
          position = 0;
          callbackRef.current();
        }
        return;
      }
      position = key === KONAMI_CODE[0] ? 1 : 0;
    };

    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);
}

export default function AboutPage({
  onClose,
  showBack = true,
  onNavigate,
  onOpenPdfSettings,
  initialSection,
}: Props) {
  const [gameOpen, setGameOpen] = useState(false);
  const [builtinProfiles, setBuiltinProfiles] = useState<ProfileSummary[]>([]);
  useKonamiCode(() => setGameOpen(true));

  useEffect(() => {
    let live = true;
    void invoke<ProfileSummary[]>("list_profiles")
      .then((profiles) => {
        if (live) setBuiltinProfiles(profiles.filter((profile) => profile.builtin));
      })
      .catch(() => undefined);
    return () => { live = false; };
  }, []);

  return (
    <>
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

        <HelpContent
          onNavigate={onNavigate}
          onOpenPdfSettings={onOpenPdfSettings}
          initialSection={initialSection}
          builtinProfiles={builtinProfiles}
        />
        <AboutFooter />
      </div>
      {gameOpen && <FlappyBirdGame onClose={() => setGameOpen(false)} />}
    </>
  );
}

function HelpContent({
  onNavigate,
  onOpenPdfSettings,
  initialSection,
  builtinProfiles,
}: {
  onNavigate?: (page: AppPage) => void;
  onOpenPdfSettings?: () => void;
  initialSection?: "privacy";
  builtinProfiles: ProfileSummary[];
}) {
  const privacyRef = useRef<HTMLDetailsElement>(null);

  useEffect(() => {
    if (initialSection === "privacy") {
      privacyRef.current?.scrollIntoView?.({ block: "start" });
    }
  }, [initialSection]);

  return (
    <div className="space-y-8">
      {/* Overview */}
      <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
        Pipeline orchestrates AI agents for doing and reviewing academic research.
        Use Workspace for persistent research conversations and Reviews for
        repeatable, structured tasks over papers, proposals, and other research material.
      </p>

      {/* Setup */}
      <Section title="Before your first report">
        <div className="space-y-2">
          <BadgeRow
            badge="Required"
            badgeColor="text-red-700 bg-red-50 dark:text-red-300 dark:bg-red-900/30"
            title="An AI provider"
          >
            Sign in to ChatGPT in Settings → Providers or through Claude Code; add an
            Anthropic, OpenAI, or Google API key in Settings; or connect a
            local OpenAI-compatible server. A Claude or ChatGPT
            subscription works without an API key; Google always uses a
            Gemini API key. The status button in the lower-left corner shows
            what Pipeline found.
            <NavLink
              onClick={onNavigate && (() => onNavigate("settings"))}
              label="Open Settings"
            />
          </BadgeRow>
          <BadgeRow
            badge="Included"
            badgeColor="text-green-700 bg-green-50 dark:text-green-300 dark:bg-green-900/30"
            title="PDF tools"
          >
            The tools that render PDF pages, check extraction completeness,
            and run pdftotext are bundled. Nothing else is required.
          </BadgeRow>
          <BadgeRow
            badge="Highly recommended"
            badgeColor="text-amber-700 bg-amber-50 dark:text-amber-300 dark:bg-amber-900/30"
            title="PaddleOCR-VL Full Parser"
          >
            A local PDF parser that recovers reading order, headings,
            formulas, and tables, with no extra model calls. Install it once
            under Settings → Reviews → PDF Extraction.
            <NavLink onClick={onOpenPdfSettings} label="Install in Settings" />
          </BadgeRow>
        </div>
      </Section>

      {/* Quick start */}
      <Section title="Quick start">
        <div className="grid gap-3">
          <StageCard
            number="1"
            title="Choose a workflow and input"
            description="On New run, pick a workflow, then select files or a folder and say how Pipeline should treat the selection — one document, a LaTeX project, a browsable folder, or a batch."
          />
          <StageCard
            number="2"
            title="Confirm the preview"
            description="Pipeline shows which steps will run, which provider they will use, and what they may read. Nothing starts until you confirm."
          />
          <StageCard
            number="3"
            title="Read the report"
            description="Pipeline extracts the text, builds a short survey of the material so each step knows where things are, runs the steps, and opens the finished report."
          />
        </div>
      </Section>

      <Section title="Workspace for research conversations">
        <p className="text-sm leading-relaxed text-gray-600 dark:text-gray-400">
          Use Workspace to chat with ChatGPT. Start a conversation, or create a project
          to keep papers, files, notes, and related conversations together. Add research
          tools and saved instructions when you need them.
        </p>
        <p className="mt-2 text-sm leading-relaxed text-gray-600 dark:text-gray-400">
          Backups include your projects, research files, and conversation transcripts.
          After restoring, sign in to ChatGPT to continue from your reviewed notes
          and selected evidence.
        </p>
        <p className="mt-2 text-sm leading-relaxed text-gray-600 dark:text-gray-400">
          Choose “Review this revision” to review a saved copy of the paper.
          The preview lets you choose the workflow and models before starting.
          Each run uses the settings and provider sign-in configured for Reviews.
        </p>
        <NavLink onClick={onNavigate && (() => onNavigate("workspace"))} label="Open Workspace" />
      </Section>

      {/* Reference sections, collapsed by default */}
      <div className="border-t border-gray-200 dark:border-gray-800">
        <Collapsible title="Reviews">
          <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
            A workflow is a saved recipe: its steps, prompts, and settings.
            Pick one on New run; edit the active one in Reviews → Designer.
            Steps run as soon as their prerequisites finish, and independent
            steps run at the same time.
          </p>
          <div className="space-y-1.5">
            {(builtinProfiles.length > 0 ? builtinProfiles : [
              { id: "auto-review", name: "Automatic Paper Review (Full)", step_count: 5, builtin: true },
              { id: "auto-review-quick", name: "Automatic Paper Review (Quick)", step_count: 4, builtin: true },
              { id: "grant-review", name: "Grant Proposal Review", step_count: 4, builtin: true },
            ]).map((profile) => (
              <BuiltinRow
                key={profile.id}
                name={profile.name}
                description={`${profile.step_count} configured review step${profile.step_count === 1 ? "" : "s"}. Open Reviews → Designer for the current prompts, providers, inputs, and outputs.`}
              />
            ))}
          </div>
          <div>
            <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed mb-2">
              In Reviews → Designer you can:
            </p>
            <ul className="space-y-1.5">
              <CheckItem text="Add, remove, reorder, or rewrite steps, and choose what each step may read" />
              <CheckItem text="Pick a provider and model per step, or run one step with several providers and merge the results" />
              <CheckItem text="Allow web search, add run conditions, repeat a step over matching files, or collect values before a report starts" />
              <CheckItem text="Import and export a step, a workflow, or all workflows and settings" />
            </ul>
          </div>
          <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
            The Gallery installs curated workflows as ordinary editable copies.
          </p>
          <div className="flex gap-4">
            <NavLink
              onClick={onNavigate && (() => onNavigate("pipeline"))}
              label="Open Designer"
            />
            <NavLink
              onClick={onNavigate && (() => onNavigate("gallery"))}
              label="Open Gallery"
            />
          </div>
        </Collapsible>

        <Collapsible title="Reading your report">
          <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
            A finished report can expose {REPORT_WORKSPACE_TABS.length} views: {REPORT_WORKSPACE_TABS.map((tab) => tab.label).join(", ")}.
            {" "}{REPORT_WORKSPACE_TABS.map((tab) => `${tab.label} contains ${tab.description} (${tab.availability}).`).join(" ")}
          </p>
          <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
            Use Export for a safe shareable package, a clearly marked sensitive
            forensic archive, a custom selection, Markdown, or the system print
            dialog for saving PDF.
          </p>
        </Collapsible>

        <Collapsible title="History & run collections">
          <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
            History lists every saved report. Rename, tag, or delete reports;
            resume an interrupted one; rerun one from scratch; or compare two
            side by side. Settings can also add a model-written comparison
            automatically when Pipeline finds an earlier report for the same
            document.
          </p>
          <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
            Run collections group related reports and revisions and keep a running
            ledger of issues across them, without moving or deleting anything.
          </p>
          <div className="flex gap-4">
            <NavLink
              onClick={onNavigate && (() => onNavigate("history"))}
              label="Open History"
            />
            <NavLink
              onClick={onNavigate && (() => onNavigate("projects"))}
              label="Open run collections"
            />
          </div>
        </Collapsible>

        <Collapsible title="PDF & document handling">
          <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
            Pipeline prepares a text version of each document before the
            workflow begins. LaTeX and Word files are read directly. For PDFs,
            pick a method in the workflow or inherit the choice from
            Settings → Reviews → PDF Extraction.
          </p>
          <div className="space-y-2">
            <BadgeRow
              badge="Included"
              badgeColor="text-green-700 bg-green-50 dark:text-green-300 dark:bg-green-900/30"
              title="LaTeX and Word"
            >
              Read directly, keeping equations and table structure. A LaTeX
              project also keeps page images from its compiled PDF when one is
              present.
            </BadgeRow>
            <BadgeRow
              badge="Uses your provider"
              badgeColor="text-blue-700 bg-blue-50 dark:text-blue-300 dark:bg-blue-900/30"
              title="LLM PDF extraction"
            >
              The model transcribes the PDF in page ranges. This costs model
              calls but usually preserves equations and layout best. Pipeline
              stops if its page checks fail.
            </BadgeRow>
            <BadgeRow
              badge="Optional install"
              badgeColor="text-gray-700 bg-gray-100 dark:text-gray-300 dark:bg-gray-700/50"
              title="PaddleOCR-VL Full Parser"
            >
              A local engine that recovers reading order, headings, formulas,
              and tables. Install it under Settings → Reviews → PDF Extraction.
            </BadgeRow>
            <BadgeRow
              badge="Included"
              badgeColor="text-green-700 bg-green-50 dark:text-green-300 dark:bg-green-900/30"
              title="pdftotext"
            >
              Fast local plain-text extraction. Fine for simple text;
              equations and complex layouts may not survive.
            </BadgeRow>
          </div>
        </Collapsible>

        <Collapsible title="Batch reports">
          <p className="text-sm text-gray-600 dark:text-gray-400 leading-relaxed">
            To review several documents with the same workflow, select
            multiple files on New run, or choose a folder and set its
            meaning to Batch of documents. Each document becomes an
            independent report and the queue appears under Current batch.
          </p>
        </Collapsible>

        <Collapsible
          title="Data & privacy"
          defaultOpen={initialSection === "privacy"}
          detailsRef={privacyRef}
        >
          <ul className="space-y-2 text-sm text-gray-600 dark:text-gray-400 leading-relaxed list-disc pl-5">
            <li>
              Reports, sources, and logs stay on this computer under{" "}
              <code className="text-xs bg-gray-100 dark:bg-gray-800 px-1.5 py-0.5 rounded font-mono">
                ~/.pipeline/runs/
              </code>{" "}
              until you delete them or set a retention limit in Settings.
            </li>
            <li>
              Workspace conversations, research records, saved attachments, and separate
              Codex state are stored under{" "}
              <code className="text-xs bg-gray-100 dark:bg-gray-800 px-1.5 py-0.5 rounded font-mono">
                ~/.pipeline/workbench/
              </code>
              . Workspace archives exclude Codex credentials, but the exported archive or
              transcript is a sensitive ordinary file wherever you save it.
            </li>
            <li>
              Each step reads only the material its workflow allows. When a
              step runs on a cloud provider, that material is sent through the
              provider's CLI or API and is subject to your provider account's
              plan and data-use terms.
            </li>
            <li>
              Workspace sends conversation content and selected research context or tool
              results through its managed ChatGPT account. Imported material stays local until
              selected as context or returned by an enabled tool.
            </li>
            <li>
              If a workflow enables web search and the provider supports it,
              search queries are also sent to an external service.
            </li>
          </ul>
          <ExternalLink url="https://github.com/mdroste/pipeline/blob/main/PRIVACY.md">
            Full privacy details
          </ExternalLink>
        </Collapsible>
      </div>

      {/* Troubleshooting */}
      <Section title="Something not working?">
        <ul className="space-y-1.5 text-sm text-gray-600 dark:text-gray-400 leading-relaxed list-disc pl-5">
          <li>
            The status button in the lower-left corner reports missing CLIs,
            keys, and tools.
          </li>
          <li>
            The Console at the bottom of the workspace has per-step logs;
            failed steps keep their error output.
          </li>
          <li>Found a bug? Open an issue through the GitHub link below.</li>
        </ul>
      </Section>
    </div>
  );
}

// --- Help page components ---

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200 mb-3">
        {title}
      </h3>
      {children}
    </div>
  );
}

function Collapsible({
  title,
  children,
  defaultOpen = false,
  detailsRef,
}: {
  title: string;
  children: React.ReactNode;
  defaultOpen?: boolean;
  detailsRef?: React.Ref<HTMLDetailsElement>;
}) {
  return (
    <details
      ref={detailsRef}
      open={defaultOpen}
      className="group border-b border-gray-200 dark:border-gray-800"
    >
      <summary className="flex cursor-pointer select-none items-center justify-between gap-2 py-3 text-sm font-semibold text-gray-800 dark:text-gray-200 list-none [&::-webkit-details-marker]:hidden">
        {title}
        <svg
          className="w-4 h-4 shrink-0 text-gray-400 transition-transform group-open:rotate-180"
          viewBox="0 0 20 20"
          fill="currentColor"
          aria-hidden="true"
        >
          <path
            fillRule="evenodd"
            d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z"
            clipRule="evenodd"
          />
        </svg>
      </summary>
      <div className="pb-4 space-y-3">{children}</div>
    </details>
  );
}

function NavLink({
  onClick,
  label,
}: {
  onClick?: () => void;
  label: string;
}) {
  if (!onClick) return null;
  return (
    <button
      type="button"
      onClick={onClick}
      className="block mt-1 text-sm font-medium text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300 hover:underline"
    >
      {label} <span aria-hidden="true">→</span>
    </button>
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

function BuiltinRow({ name, description }: { name: string; description: string }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="text-sm font-medium text-gray-800 dark:text-gray-200 shrink-0">{name}</span>
      <span className="text-xs text-gray-500 dark:text-gray-400">{description}</span>
    </div>
  );
}

function CheckItem({ text }: { text: string }) {
  return (
    <li className="flex items-start gap-2 text-sm text-gray-600 dark:text-gray-400">
      <svg className="w-4 h-4 text-green-700 dark:text-green-400 mt-0.5 shrink-0" viewBox="0 0 20 20" fill="currentColor">
        <path fillRule="evenodd" d="M16.704 4.153a.75.75 0 01.143 1.052l-8 10.5a.75.75 0 01-1.127.075l-4.5-4.5a.75.75 0 011.06-1.06l3.894 3.893 7.48-9.817a.75.75 0 011.05-.143z" clipRule="evenodd" />
      </svg>
      {text}
    </li>
  );
}

function BadgeRow({
  badge,
  badgeColor,
  title,
  children,
}: {
  badge: string;
  badgeColor: string;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start gap-3">
      <span className={`text-[10px] font-semibold uppercase px-1.5 py-0.5 rounded shrink-0 mt-0.5 whitespace-nowrap ${badgeColor}`}>
        {badge}
      </span>
      <div>
        <p className="text-sm font-medium text-gray-800 dark:text-gray-200">{title}</p>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5 leading-relaxed">{children}</p>
      </div>
    </div>
  );
}

function ExternalLink({ url, children }: { url: string; children: React.ReactNode }) {
  const [error, setError] = useState("");

  const handleClick = (event: React.MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    setError("");
    void openUrl(url).catch((cause) => {
      const detail = cause instanceof Error ? cause.message : String(cause);
      setError(
        `Pipeline could not open the link in your browser${detail ? `: ${detail}` : "."} You can copy the address from the link instead.`,
      );
    });
  };

  return (
    <div>
      <a
        href={url}
        onClick={handleClick}
        className="text-sm text-blue-600 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300 hover:underline cursor-pointer"
      >
        {children}
      </a>
      {error && (
        <p role="alert" className="mt-1 text-xs leading-relaxed text-red-700 dark:text-red-300">
          {error}
        </p>
      )}
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
