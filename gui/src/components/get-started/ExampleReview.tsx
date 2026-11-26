import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import useModalDialog from "../../hooks/useModalDialog";
import HomePage from "../HomePage";
import RunSetupPanel from "../RunSetupPanel";
import RunPreview from "../RunPreview";
import ReportWorkspace from "../ReportWorkspace";
import type { WorkspaceTab } from "../report-workspace/types";
import type { PrimaryInputSelection } from "../../lib/workflowTypes";
import { ReviewSetupReads } from "../reviewSetupReads";
import { ReviewSetupEnvironment } from "../reviewSetupEnvironment";
import {
  EXAMPLE_REPORT,
  EXAMPLE_REPORT_MARKDOWN,
  EXAMPLE_SUMMARY,
} from "./exampleReviewData";
import { EXAMPLE_EXTRACTED_TEXT } from "./exampleReviewPaper";
import {
  SAMPLE_CONFIG,
  SAMPLE_ENVIRONMENT,
  SAMPLE_NOTE,
  SAMPLE_PATH,
  SAMPLE_PLAN,
  SAMPLE_READS,
} from "./reviewTutorialFixture";
import { useExampleReviewSurface } from "./ExampleReviewSurface";
import ReviewTutorialCoach from "./ReviewTutorialCoach";
import ReviewTutorialRun from "./ReviewTutorialRun";
import {
  REVIEW_TUTORIAL_STEPS,
  TUTORIAL_STEP as S,
} from "./reviewTutorialSteps";
import "./ReviewTour.css";

export const EXAMPLE_REVIEW_TITLE = "Example review";
const noop = () => {};
const paper: PrimaryInputSelection = {
  paths: [SAMPLE_PATH],
  interpretation: "document",
  selectionKind: "file",
};

/** The production review screens, temporarily mounted in the app's content slot. */
export default function ExampleReview({
  onClose,
  backLabel,
}: {
  onClose: () => void;
  backLabel?: string;
}) {
  const [step, setStep] = useState(0);
  const externalTarget =
    step === S.running ? REVIEW_TUTORIAL_STEPS[S.running].target : undefined;
  // The spotlight owns background isolation, including the active sidebar target.
  const root = useModalDialog<HTMLDivElement>(
    onClose,
    true,
    false,
    externalTarget,
  );
  const surface = useExampleReviewSurface();
  const setSurfacePage = surface?.setPage;
  const [note, setNote] = useState(SAMPLE_NOTE);
  const [selection, setSelection] = useState<PrimaryInputSelection | null>(
    null,
  );
  const [tab, setTab] = useState<WorkspaceTab>("report");
  const [generation, setGeneration] = useState(0);
  const opener = useRef(document.activeElement);
  useEffect(() => {
    const containShortcuts = (event: KeyboardEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      const editable =
        event.target instanceof HTMLElement &&
        event.target.matches("input, textarea, [contenteditable=true]");
      if (editable && /^[acvxyz]$/i.test(event.key)) return;
      event.preventDefault();
      event.stopPropagation();
    };
    document.addEventListener("keydown", containShortcuts, true);
    return () =>
      document.removeEventListener("keydown", containShortcuts, true);
  }, []);
  useLayoutEffect(() => {
    setSurfacePage?.(step === S.start || step === S.away ? "home" : "main");
  }, [setSurfacePage, step]);
  useEffect(
    () => () => {
      setSurfacePage?.(null);
      const previous = opener.current;
      // Wait for the preserved page to become visible again.
      window.setTimeout(() => {
        if (previous instanceof HTMLElement && previous.isConnected)
          previous.focus();
      }, 0);
    },
    [setSurfacePage],
  );

  const goTo = (next: number) => {
    // Kept report panels retain expanded findings when going back through the tour.
    if (
      next === S.finding &&
      root.current
        ?.querySelector("[data-review-issue] button[aria-expanded]")
        ?.getAttribute("aria-expanded") === "true"
    )
      next = S.evidence;
    setStep(next);
    if (next === S.start) setSelection(null);
    else if (next < S.running) setSelection(paper);
    if (next === S.results) setTab("report");
    if (next === S.finding || next === S.evidence) setTab("issues");
  };
  const restart = () => {
    setNote(SAMPLE_NOTE);
    setGeneration((value) => value + 1);
    goTo(S.start);
  };
  const complete = step === REVIEW_TUTORIAL_STEPS.length;
  const content = (
    <div
      ref={root}
      role="dialog"
      aria-modal={externalTarget ? undefined : true}
      aria-label={EXAMPLE_REVIEW_TITLE}
      tabIndex={-1}
      data-testid="example-review"
      className={`review-tutorial ${surface ? "" : "review-tutorial-standalone"}`}
      onClick={(event) => {
        if (
          step === S.finding &&
          event.target instanceof Element &&
          event.target.closest(
            '[data-review-issue] button[aria-expanded="false"]',
          )
        )
          goTo(S.evidence);
      }}
    >
      <div className="review-tutorial-screen">
        {(step === S.start || step === S.away) && (
          <HomePage
            projects={[]}
            projectsLoading={false}
            hasCurrentReview={step === S.away}
            reviewRunning={step === S.away}
            onAssistant={noop}
            onNewProject={noop}
            onOpenProject={noop}
            onNewReview={() => goTo(S.document)}
            onContinueReview={() => goTo(S.resumed)}
            onNavigate={noop}
          />
        )}
        {step >= S.document && step <= S.preview && (
          <ReviewSetupEnvironment.Provider value={SAMPLE_ENVIRONMENT}>
            <ReviewSetupReads.Provider value={SAMPLE_READS}>
              <RunSetupPanel
                key={generation}
                authorNote={note}
                onAuthorNoteChange={setNote}
                configVersion={0}
                configError={null}
                configLoading={false}
                dependenciesError={null}
                inputMode="document"
                inputSelection={selection}
                listenersReady
                localLlmActive={false}
                paperPath={selection?.paths[0] ?? null}
                preparingRun={false}
                parallelOverrides={null}
                selectionKey={generation}
                onConfigureWorkflow={noop}
                onGenerate={() => goTo(S.preview)}
                onPaperPathChange={noop}
                onInputSelectionChange={setSelection}
                onPrivacyDetails={noop}
                onParallelOverridesChange={noop}
                onProfileChange={noop}
                onRetryConfig={noop}
                onRetryDependencies={noop}
              />
            </ReviewSetupReads.Provider>
          </ReviewSetupEnvironment.Provider>
        )}
        {step >= S.running && step <= S.finished && step !== S.away && (
          <ReviewTutorialRun
            phase={
              step < S.validation
                ? "review"
                : step === S.validation
                  ? "validate"
                  : "done"
            }
            onCancel={() => goTo(S.previewAction)}
            onOpen={() => goTo(S.results)}
          />
        )}
        {step >= S.results && (
          <ReportWorkspace
            key={generation}
            report={EXAMPLE_REPORT}
            markdown={EXAMPLE_REPORT_MARKDOWN}
            extractedText={EXAMPLE_EXTRACTED_TEXT}
            summary={EXAMPLE_SUMMARY}
            selectedTab={tab}
            exportEnabled={false}
            onTabChange={(next) => {
              setTab(next);
              if (step === S.results && next === "issues") goTo(S.finding);
              if (step === S.evidence && next === "sources") goTo(S.sources);
            }}
          />
        )}
      </div>
      {step === S.preview && (
        <RunPreview
          embedded
          authorNote={note}
          onEditAuthorNote={() => goTo(S.instructions)}
          plan={SAMPLE_PLAN}
          config={SAMPLE_CONFIG}
          inputPath={SAMPLE_PATH}
          onCancel={() => goTo(S.previewAction)}
          onRun={() => goTo(S.running)}
        />
      )}
      <ReviewTutorialCoach
        root={root}
        step={step}
        onNext={() => goTo(Math.min(step + 1, REVIEW_TUTORIAL_STEPS.length))}
        onBack={() => goTo(step === S.evidence ? S.results : step - 1)}
        onRestart={restart}
        onSkip={() => goTo(S.results)}
        onExplore={() => goTo(REVIEW_TUTORIAL_STEPS.length)}
        onClose={onClose}
        closeLabel={backLabel}
      />
      {complete && (
        <span className="sr-only">
          The tutorial is complete. All report controls are available; exports
          remain disabled in this example.
        </span>
      )}
    </div>
  );
  return surface?.target ? createPortal(content, surface.target) : content;
}
