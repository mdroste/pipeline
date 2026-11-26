import { Icon } from "../ui/icons";
import { Fragment, lazy, Suspense, useEffect, useRef, useState } from "react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { getVersion } from "@tauri-apps/api/app";
import { workflowProfileClient } from "../lib/workflowProfileClient";
import type { AppPage } from "./NavRail";
import type { ProfileSummary } from "../lib/workflowTypes";
import { REPORT_WORKSPACE_TABS } from "../lib/productMetadata";
import { workspaceSections } from "../lib/workspaceNavigation";
import PageHeader from "../ui/PageHeader";

const FlappyBirdGame = lazy(() => import("./FlappyBirdGame"));

interface Props {
  onClose: () => void;
  showBack?: boolean;
  /** Lets Help sections jump to the page they describe; links are hidden without it. */
  onNavigate?: (page: AppPage) => void;
  /** Jumps to the PaddleOCR-VL install card in Settings → Reviews → PDF extraction. */
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
      if (event.repeat || event.altKey || event.ctrlKey || event.metaKey)
        return;
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
    void workflowProfileClient
      .profiles()
      .then((profiles) => {
        if (live)
          setBuiltinProfiles(profiles.filter((profile) => profile.builtin));
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);

  return (
    <>
      <div className="ui-page ui-page--index">
        <PageHeader
          title="Help"
          description="How to get set up, what each part of the app does, and where your data goes."
          action={
            showBack ? (
              <button type="button" className="ui-button" onClick={onClose}>
                Back
              </button>
            ) : undefined
          }
        />
        {/* Long passages stay at a readable width on a wide page. */}
        <div className="max-w-3xl">
          <HelpContent
            onNavigate={onNavigate}
            onOpenPdfSettings={onOpenPdfSettings}
            initialSection={initialSection}
            builtinProfiles={builtinProfiles}
          />
          <AboutFooter />
        </div>
      </div>
      {gameOpen && (
        <Suspense fallback={null}>
          <FlappyBirdGame onClose={() => setGameOpen(false)} />
        </Suspense>
      )}
    </>
  );
}

/** Listed until `list_profiles` answers; its results reuse these descriptions. */
const BUILTIN_REVIEWS = [
  {
    id: "auto-paper-review",
    name: "Paper Review",
    description:
      "Assesses the argument, presentation, and contribution. It adds subject and method specialists, combines their comments, and checks them against the paper.",
  },
  {
    id: "auto-paper-review-fast",
    name: "Paper Review (Fast)",
    description:
      "A shorter Paper Review with fewer specialists and no separate assessment of contribution and literature.",
  },
  {
    id: "slide-review",
    name: "Slide Review",
    description:
      "Assesses a deck's argument, figures, readability, and slide order. Export your slides as a PDF first.",
  },
  {
    id: "grant-review",
    name: "Grant Proposal Review",
    description:
      "Assesses aims, feasibility, consistency, and how well the proposal reads for a panel.",
  },
  {
    id: "declaude",
    name: "Declaude",
    description:
      "Finds formulaic prose and suggests replacements that fit the document's subject, audience, and style.",
  },
];

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

  const builtinReviews =
    builtinProfiles.length > 0
      ? builtinProfiles.map((profile) => ({
          name: profile.name,
          description:
            BUILTIN_REVIEWS.find((review) => review.id === profile.id)
              ?.description ?? "",
        }))
      : BUILTIN_REVIEWS;

  return (
    <div className="space-y-8 text-ui-body leading-relaxed text-ink-muted">
      {/* Overview */}
      <p>
        Pipeline is a desktop app for academic research. Chat with an AI
        assistant, keep each paper and its files in a project, get AI reviews of
        your drafts, and automate repeated work.
      </p>

      {/* Setup */}
      <Section title="Set up">
        <BadgeRow
          badge="Required"
          badgeColor="text-danger bg-danger-soft"
          title="Connect an AI provider"
        >
          <p>
            Add at least one connection in <Label>Settings → Models</Label>. The
            dot beside Help turns green when Pipeline has what it needs, and the
            sidebar shows <Label>Setup needed</Label> when it does not.
          </p>
          <TermList
            items={[
              {
                name: "ChatGPT plan",
                description: (
                  <>
                    Install the Codex command-line tool. On the{" "}
                    <Label>OpenAI</Label> card, choose{" "}
                    <Label>Sign in to ChatGPT</Label>, then{" "}
                    <Label>Subscription</Label>.
                  </>
                ),
              },
              {
                name: "Claude plan",
                description: (
                  <>
                    Install Claude Code and run{" "}
                    <InlineCode>claude auth login</InlineCode> in a terminal. On
                    the <Label>Anthropic</Label> card, choose{" "}
                    <Label>Subscription</Label>.
                  </>
                ),
              },
              {
                name: "API key",
                description:
                  "Paste a key from OpenAI, Anthropic, or Google. The provider bills you for each call.",
              },
              {
                name: "Local server",
                description: (
                  <>
                    Enter the address of a server such as Ollama or LM Studio on
                    the <Label>OpenAI-compatible</Label> card.
                  </>
                ),
              },
            ]}
          />
          <NavLink
            onClick={onNavigate && (() => onNavigate("settings"))}
            label="Open Settings"
          />
        </BadgeRow>
        <BadgeRow
          badge="Included"
          badgeColor="bg-sunken text-success bg-success-soft"
          title="PDF tools"
        >
          Pipeline can show PDFs and extract their plain text with nothing else
          to install.
        </BadgeRow>
        <BadgeRow
          badge="Highly recommended"
          badgeColor="text-warning bg-warning-soft"
          title="PaddleOCR-VL Full Parser"
        >
          Reads PDFs on your computer and keeps the reading order, headings,
          equations, and tables. It uses no model calls.
          <NavLink onClick={onOpenPdfSettings} label="Install in Settings" />
        </BadgeRow>
      </Section>

      {/* First review */}
      <Section title="Run your first review">
        <div className="grid gap-3">
          <StageCard
            number="1"
            title="Choose a review and a document"
            description={
              <>
                Open <Label>New review</Label> and pick a review design. Add a
                PDF, Word file, .tex file, or folder. For a folder, say whether
                it holds one LaTeX paper, source files to browse, or a batch of
                documents.
              </>
            }
          />
          <StageCard
            number="2"
            title="Check the preview"
            description={
              <>
                Choose <Label>Start review</Label> to open the preview. It shows
                each step, its provider, the files it can read, and the expected
                usage. Choose <Label>Start review</Label> again when it looks
                right.
              </>
            }
          />
          <StageCard
            number="3"
            title="Read the report"
            description="Pipeline extracts the text, runs the reviewers, combines their comments, and checks them against your document. The report opens when the review finishes."
          />
        </div>
        <p>
          To see a finished report first, choose{" "}
          <Label>See an example review</Label> on Home. It uses no model calls.
        </p>
      </Section>

      <Section title="Projects and conversations">
        <p>
          A conversation is a chat with the AI assistant. Start one with{" "}
          <Label>New conversation</Label> on Home, or inside a project to keep
          it with that paper. The <Label>Model and thinking</Label> chip below
          the message box sets the provider and model.{" "}
          <Label>Assistant settings</Label> sets which files and tools the
          assistant can use.
        </p>
        <p>
          A project keeps a paper, its folder, your notes, and its conversations
          together. Create one with <Label>New project</Label> under{" "}
          <Label>Projects</Label>. Choose a view below the current project’s
          name:
        </p>
        <TermList
          items={workspaceSections.map((section) => ({
            name: section.label,
            description: section.description,
          }))}
        />
        <p>
          To review a saved version of a paper, open{" "}
          <Label>Reviews → Send for review</Label>, choose the{" "}
          <Label>Paper to review</Label>, then choose{" "}
          <Label>Review this revision…</Label> to open the preview. The review
          uses your Reviews settings and account. Project notes aren't sent.
        </p>
        <p>
          To back up all your projects and conversations, open{" "}
          <Label>Settings → Data & Storage → Manage research data</Label> and
          choose <Label>Export research backup</Label>. Backups leave out
          reviews, automations, project folders, and sign-ins. Keep your own
          copies of project folders, and sign in again after restoring.
        </p>
        <NavLink
          onClick={onNavigate && (() => onNavigate("workspace"))}
          label="Open Projects"
        />
      </Section>

      <Section title="Automations">
        <p>
          An automation runs a series of steps for you, such as conversation
          turns, reviews, checks, and pauses for your input. The{" "}
          <Label>Review and revise</Label> template drafts a paper, reviews it,
          and revises it a set number of times. To start one, open{" "}
          <Label>Automations</Label> and choose <Label>+ New automation</Label>.
        </p>
        <p>
          Automations run only while Pipeline is running. To keep them going
          after you close the window, turn on{" "}
          <Label>Keep running when the window closes</Label> on the Automations
          page. Pipeline can't wake a sleeping computer.
        </p>
        <NavLink
          onClick={onNavigate && (() => onNavigate("tasks"))}
          label="Open Automations"
        />
      </Section>

      {/* Reference sections, collapsed by default */}
      <div className="border-t border-line">
        <Collapsible title="Reviews">
          <p>
            A review design is a saved set of steps, prompts, and settings.
            Pipeline comes with these:
          </p>
          <TermList items={builtinReviews} />
          <p>
            The <Label>Gallery</Label> has more, such as Reference Check and
            Thesis Chapter Review. Install one and edit it to fit your work.
          </p>
          <div>
            <p className="mb-2">
              In the <Label>Review designer</Label> you can:
            </p>
            <ul className="space-y-1.5">
              <CheckItem text="Add, remove, reorder, or rewrite steps" />
              <CheckItem text="Choose what each step can read" />
              <CheckItem text="Pick a provider and model for each step, or run one step with several providers and merge the results" />
              <CheckItem text="Turn on web search, run a step only under set conditions, or repeat it across matching files" />
              <CheckItem text="Ask for values before a review starts" />
              <CheckItem text="Import and export a step, a review design, or all your designs and settings" />
            </ul>
          </div>
          <p>Steps that don't depend on each other run at the same time.</p>
          <div className="flex gap-4">
            <NavLink
              onClick={onNavigate && (() => onNavigate("pipeline"))}
              label="Open Review designer"
            />
            <NavLink
              onClick={onNavigate && (() => onNavigate("gallery"))}
              label="Open Gallery"
            />
          </div>
        </Collapsible>

        <Collapsible title="Reading a report">
          <p>A finished review has these tabs:</p>
          <TermList
            items={REPORT_WORKSPACE_TABS.map((tab) => ({
              name: tab.label,
              description: tab.description,
            }))}
          />
          <p>
            <Label>Export</Label> saves a report as PDF, HTML, or Word.{" "}
            <Label>Word with comments on the original</Label> and{" "}
            <Label>PDF with highlights</Label> mark each finding on a copy of
            your document.
          </p>
          <p>
            <Label>Shareable report</Label> removes private information and
            leaves out unverified findings, your source documents, and raw model
            responses. <Label>Complete review archive</Label> includes all of
            them, plus the logs. <Label>Save Markdown…</Label>,{" "}
            <Label>Save as PDF…</Label>, and <Label>Print…</Label> are in the
            same menu.
          </p>
        </Collapsible>

        <Collapsible title="Review history & collections">
          <p>
            <Label>Review history</Label> lists every saved report.{" "}
            <Label>Resume</Label> continues an interrupted review.{" "}
            <Label>Regenerate</Label> runs the current review design again on
            the saved document. <Label>Compare</Label> shows two reports side by
            side. Deleted reports go to <Label>Trash</Label>, where you can
            restore them.
          </p>
          <p>
            To compare each new report with the previous one automatically, turn
            on <Label>Compare with the previous report</Label> under{" "}
            <Label>Settings → Reviews</Label>. It lists which concerns were
            addressed, which remain, and which are new. This adds a model call.
          </p>
          <p>
            A review collection groups related reports and tracks their findings
            across drafts. Open <Label>Collections</Label> from Review history.
          </p>
          <div className="flex gap-4">
            <NavLink
              onClick={onNavigate && (() => onNavigate("history"))}
              label="Open Review history"
            />
            <NavLink
              onClick={onNavigate && (() => onNavigate("projects"))}
              label="Open review collections"
            />
          </div>
        </Collapsible>

        <Collapsible title="How Pipeline reads documents">
          <p>
            Pipeline turns each document into text before the review starts. For
            PDFs, set the method under{" "}
            <Label>Settings → Reviews → PDF extraction</Label>, or change it for
            a single review. The default, <Label>Automatic</Label>, uses the
            Full Parser if it's installed and your AI model if it isn't. If
            extraction fails, the review stops and shows the error.
          </p>
          <div className="space-y-2">
            <BadgeRow
              badge="Included"
              badgeColor="bg-sunken text-success bg-success-soft"
              title="LaTeX and Word"
            >
              Read directly, so equations and tables come through intact. If a
              compiled PDF sits beside your LaTeX source, Pipeline uses it for
              page images.
            </BadgeRow>
            <BadgeRow
              badge="Optional install"
              badgeColor="text-gray-700 bg-gray-100 dark:text-gray-300 dark:bg-gray-700/50"
              title="PaddleOCR-VL Full Parser"
            >
              Runs on your computer and keeps reading order, headings,
              equations, and tables, even when a table spans pages. It needs
              about 4 GB of disk space.
            </BadgeRow>
            <BadgeRow
              badge="Uses your provider"
              badgeColor="text-blue-700 bg-blue-50 dark:text-blue-300 dark:bg-blue-900/30"
              title="LLM (your AI model)"
            >
              Your selected model reads the PDF a few pages at a time. The PDF
              goes to your provider, and each call counts toward your usage.
            </BadgeRow>
            <BadgeRow
              badge="Included"
              badgeColor="bg-sunken text-success bg-success-soft"
              title="pdftotext"
            >
              The fastest option. It runs on your computer, flattens the layout,
              and usually loses equations.
            </BadgeRow>
          </div>
        </Collapsible>

        <Collapsible title="Reviewing several documents">
          <p>
            To run the same review on several documents, select multiple files
            on <Label>New review</Label>, or choose a folder and select{" "}
            <Label>Batch of documents</Label>. Each document gets its own
            report. <Label>Activity</Label> in the sidebar lists running and
            queued reviews, and{" "}
            <Label>Settings → Reviews → Reviews at once</Label> sets how many
            run at once.
          </p>
        </Collapsible>

        <Collapsible
          title="Data & privacy"
          defaultOpen={initialSection === "privacy"}
          detailsRef={privacyRef}
        >
          <ul className="space-y-2 list-disc pl-5">
            <li>
              Your research is saved on this computer. Pipeline has no account
              or cloud storage, and it collects no analytics.
            </li>
            <li>
              Reports, source documents, and logs are in{" "}
              <InlineCode>~/.pipeline/runs/</InlineCode> until you delete them
              or set a retention limit in Settings.
            </li>
            <li>
              Conversations, research records, and attachments are in{" "}
              <InlineCode>~/.pipeline/workbench/</InlineCode>. If you chose
              another research data folder, both folders are there instead.
            </li>
            <li>
              A conversation sends your messages, recent history, and any
              material you select or an allowed tool reads to the provider you
              chose. Your project’s papers stay on your computer until then.
            </li>
            <li>
              A review step sends its prompt and the material it can read to
              that step's provider. The preview shows what each step can read.
            </li>
            <li>
              Your provider's plan and data-use terms apply to everything it
              receives.
            </li>
            <li>
              With web search on, queries go to your provider's search tool or
              to Bing. They can include terms from your paper, even when the
              model runs on your computer.
            </li>
          </ul>
          <ExternalLink url="https://github.com/mdroste/pipeline/blob/main/PRIVACY.md">
            Full privacy details
          </ExternalLink>
        </Collapsible>
      </div>

      {/* Troubleshooting */}
      <Section title="Troubleshooting">
        <ul className="space-y-1.5 list-disc pl-5">
          <li>
            If the sidebar shows <Label>Setup needed</Label>, select it to see
            what's missing.
          </li>
          <li>
            If a provider fails, choose <Label>Check connection</Label> on its
            card in <Label>Settings → Models</Label>.
          </li>
          <li>
            The <Label>Console</Label> below a review shows each step's log,
            including errors from failed steps.
          </li>
          <li>
            To report a bug, open an issue on GitHub using the link below.
          </li>
        </ul>
      </Section>
    </div>
  );
}

// --- Help page components ---

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <h2 className="text-sm font-semibold text-gray-800 dark:text-gray-200 mb-3">
        {title}
      </h2>
      <div className="space-y-3">{children}</div>
    </div>
  );
}

/** The name of a control, written exactly as the app shows it. */
function Label({ children }: { children: React.ReactNode }) {
  return (
    <strong className="font-medium text-gray-800 dark:text-gray-200">
      {children}
    </strong>
  );
}

function InlineCode({ children }: { children: React.ReactNode }) {
  return (
    <code className="text-xs bg-gray-100 dark:bg-gray-800 px-1.5 py-0.5 rounded font-mono">
      {children}
    </code>
  );
}

function TermList({
  items,
}: {
  items: { name: string; description: React.ReactNode }[];
}) {
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5">
      {items.map(({ name, description }) => (
        <Fragment key={name}>
          <dt className="font-medium text-gray-800 dark:text-gray-200">
            {name}
          </dt>
          <dd>{description}</dd>
        </Fragment>
      ))}
    </dl>
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
        <Icon
          name="chevron-down"
          className="w-4 h-4 shrink-0 text-gray-400 transition-transform group-open:rotate-180"
        />
      </summary>
      <div className="pb-4 space-y-3">{children}</div>
    </details>
  );
}

function NavLink({ onClick, label }: { onClick?: () => void; label: string }) {
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

function StageCard({
  number,
  title,
  description,
}: {
  number: string;
  title: string;
  description: React.ReactNode;
}) {
  return (
    <div className="flex gap-3 items-start p-3 rounded-lg bg-gray-50 dark:bg-gray-800 border border-gray-100 dark:border-gray-700">
      <span className="w-6 h-6 rounded-full bg-gray-900 text-white text-xs font-bold flex items-center justify-center shrink-0 mt-0.5">
        {number}
      </span>
      <div>
        <p className="text-sm font-medium text-gray-800 dark:text-gray-200">
          {title}
        </p>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5 leading-relaxed">
          {description}
        </p>
      </div>
    </div>
  );
}

function CheckItem({ text }: { text: string }) {
  return (
    <li className="flex items-start gap-2 text-sm text-gray-600 dark:text-gray-400">
      <Icon
        name="check"
        className="w-4 h-4 text-ink-muted text-success mt-0.5 shrink-0"
      />
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
      <span
        className={`text-[0.625rem] font-semibold uppercase px-1.5 py-0.5 rounded shrink-0 mt-0.5 whitespace-nowrap ${badgeColor}`}
      >
        {badge}
      </span>
      <div>
        <p className="text-sm font-medium text-gray-800 dark:text-gray-200">
          {title}
        </p>
        <div className="text-xs text-gray-500 dark:text-gray-400 mt-0.5 leading-relaxed space-y-1.5">
          {children}
        </div>
      </div>
    </div>
  );
}

function ExternalLink({
  url,
  children,
}: {
  url: string;
  children: React.ReactNode;
}) {
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
        <p role="alert" className="mt-1 text-xs leading-relaxed text-danger">
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
    getVersion()
      .then(setVersion)
      .catch(() => setVersion(""));
  }, []);

  const openExternal = (
    event: React.MouseEvent<HTMLAnchorElement>,
    url: string,
  ) => {
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
    <div className="mt-10 pt-6 border-t border-line">
      <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
        <span className="font-medium text-gray-700 dark:text-gray-300">
          Pipeline{version ? ` v${version}` : ""}
        </span>
        {" · "}Michael Droste{" · "}MIT license{" · "}
        <a
          href="https://github.com/mdroste/pipeline"
          onClick={(event) =>
            openExternal(event, "https://github.com/mdroste/pipeline")
          }
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
        <p role="alert" className="mt-2 text-xs leading-relaxed text-danger">
          {linkError}
        </p>
      )}
    </div>
  );
}
