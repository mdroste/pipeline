import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import useModalDialog from "../../hooks/useModalDialog";
import HomePage from "../HomePage";
import WorkspaceProjectDialog from "../WorkspaceProjectDialog";
import ProjectOverview from "../project-surface/ProjectOverview";
import ProjectActionItems from "../project-surface/ProjectActionItems";
import ProjectPage from "../project-surface/ProjectPage";
import type { SurfaceApi } from "../project-surface/shared";
import { ProjectEnvironment } from "../../lib/projectEnvironment";
import ReportWorkspace from "../ReportWorkspace";
import { useExampleReviewSurface } from "./ExampleReviewSurface";
import TutorialCoach from "./TutorialCoach";
import ProjectTutorialNavigation from "./ProjectTutorialNavigation";
import {
  PROJECT_TUTORIAL_STEPS,
  PROJECT_STEP as S,
} from "./projectTutorialSteps";
import {
  createProjectTutorialData,
  createProjectTutorialEnvironment,
} from "./projectTutorialFixture";
import {
  EXAMPLE_REPORT,
  EXAMPLE_REPORT_MARKDOWN,
  EXAMPLE_SUMMARY,
} from "./exampleReviewData";
import { EXAMPLE_EXTRACTED_TEXT } from "./exampleReviewPaper";
import "./ReviewTour.css";
import "./ProjectTour.css";

const noop = () => {};

/** Real project controls and a per-mount, in-memory sample; never creates a workspace. */
export default function ExampleProject({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState(0);
  const [generation, setGeneration] = useState(0);
  const [sample, setSample] = useState(createProjectTutorialData);
  const [, redraw] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [taskId, setTaskId] = useState("");
  const root = useModalDialog<HTMLDivElement>(onClose);
  const surface = useExampleReviewSurface();
  const setPage = surface?.setPage;
  const setProjectNavigation = surface?.setProjectNavigation;
  const opener = useRef(document.activeElement);
  const goTo = useCallback((next: number) => {
    setError(null);
    setStep(Math.max(0, Math.min(next, PROJECT_TUTORIAL_STEPS.length)));
  }, []);
  const environment = useMemo(
    () =>
      createProjectTutorialEnvironment(
        sample,
        () => redraw((value) => value + 1),
        async () => {
          goTo(S.report);
          return true;
        },
      ),
    [sample, goTo],
  );
  useLayoutEffect(() => {
    setPage?.(
      step <= S.create ? "home" : step >= S.report ? "main" : "workspace",
    );
  }, [step, setPage]);
  useLayoutEffect(() => {
    setProjectNavigation?.(
      step > S.create && step < S.report ? (
        <ProjectTutorialNavigation
          workspace={sample.workspace}
          session={sample.session}
          destination={step === S.taskStatus ? "tasks" : "overview"}
        />
      ) : null,
    );
  }, [step, sample.workspace, sample.session, setProjectNavigation]);
  useEffect(
    () => () => {
      setPage?.(null);
      setProjectNavigation?.(null);
      window.setTimeout(() => {
        const previous = opener.current;
        if (previous instanceof HTMLElement && previous.isConnected)
          previous.focus();
      }, 0);
    },
    [setPage, setProjectNavigation],
  );
  useEffect(() => {
    const containShortcuts = (event: KeyboardEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      if (
        event.target instanceof HTMLElement &&
        event.target.matches("input, textarea, [contenteditable=true]") &&
        /^[acvxyz]$/i.test(event.key)
      )
        return;
      event.preventDefault();
      event.stopPropagation();
    };
    document.addEventListener("keydown", containShortcuts, true);
    return () =>
      document.removeEventListener("keydown", containShortcuts, true);
  }, []);
  const api: SurfaceApi = {
    workspaceId: sample.workspace.id,
    workspace: sample.workspace,
    data: sample.home,
    run: async (operation) => {
      try {
        await operation();
      } catch (cause) {
        setError(String(cause));
      }
    },
    act: async (action) => {
      await environment.projectClient.mutate(sample.workspace.id, action);
    },
    saveSettings: async (patch) => {
      await environment.projectClient.mutate(sample.workspace.id, {
        action: "saveHome",
        expectedRevision: sample.home.settings.revision,
        settings: { ...sample.home.settings.body, ...patch },
      });
    },
    reportError: (cause) => setError(String(cause)),
    openAnnotation: noop,
    setTab: (tab) => {
      if (tab === "tasks") goTo(S.taskStatus);
    },
    taskId,
    setTaskId,
  };
  const restart = () => {
    setSample(createProjectTutorialData());
    setGeneration((value) => value + 1);
    setTaskId("");
    goTo(S.start);
  };
  const content = (
    <div
      ref={root}
      role="dialog"
      aria-modal="true"
      aria-label="Example project"
      tabIndex={-1}
      data-testid="example-project"
      className={`review-tutorial project-tutorial ${surface ? "" : "review-tutorial-standalone"}`}
    >
      <ProjectEnvironment.Provider value={environment}>
        <div className="review-tutorial-screen">
          {step <= S.create && (
            <HomePage
              projects={[]}
              projectsLoading={false}
              hasCurrentReview={false}
              reviewRunning={false}
              onAssistant={noop}
              onNewProject={() => goTo(S.create)}
              onOpenProject={noop}
              onNewReview={noop}
              onContinueReview={noop}
              onNavigate={noop}
            />
          )}
          {step === S.create && (
            <WorkspaceProjectDialog
              key={generation}
              onClose={() => goTo(S.start)}
              onCreated={async () => goTo(S.overview)}
            />
          )}
          {step > S.create && step < S.report && step !== S.taskStatus && (
            <div className="project-overview-host">
              <ProjectOverview
                key={`${generation}-${step}`}
                {...api}
                sessions={[sample.session]}
                showProjectName
                initialPanel={
                  step === S.briefContent
                    ? "brief"
                    : step === S.notes
                      ? "notes"
                      : step === S.reviews
                        ? "draft"
                        : undefined
                }
                onPanelChange={(panel) => {
                  if (step === S.brief && panel === "brief")
                    goTo(S.briefContent);
                  if (step === S.briefContent)
                    goTo(
                      panel === "notes"
                        ? S.notes
                        : panel === null
                          ? S.brief
                          : step,
                    );
                  if (step === S.notes && panel === null) goTo(S.tasks);
                }}
                error={error}
                onDismissError={() => setError(null)}
                onResume={noop}
                onStartConversation={noop}
                onAsk={noop}
                onOpenDocument={noop}
                onImportPaper={noop}
                onAttachFolder={noop}
              />
            </div>
          )}
          {step === S.taskStatus && (
            <ProjectPage view="tasks">
              <div data-project-tour="action-items">
                <ProjectActionItems key={generation} {...api} />
                {error && <p role="alert">{error}</p>}
              </div>
            </ProjectPage>
          )}
          {step >= S.report && (
            <div className="h-full" data-project-tour="report">
              <ReportWorkspace
                key={generation}
                report={EXAMPLE_REPORT}
                markdown={EXAMPLE_REPORT_MARKDOWN}
                extractedText={EXAMPLE_EXTRACTED_TEXT}
                summary={EXAMPLE_SUMMARY}
                exportEnabled={false}
              />
            </div>
          )}
        </div>
      </ProjectEnvironment.Provider>
      <TutorialCoach
        root={root}
        steps={PROJECT_TUTORIAL_STEPS}
        step={step}
        label="Projects tutorial"
        icon="project"
        completionTitle="Ready to start your own project?"
        skipLabel={
          step < S.overview
            ? "Skip to project"
            : step < S.report
              ? "Skip to report"
              : undefined
        }
        onNext={() => goTo(step + 1)}
        onBack={() => goTo(step - 1)}
        onRestart={restart}
        onSkip={() => goTo(step < S.overview ? S.overview : S.report)}
        onExplore={() => goTo(PROJECT_TUTORIAL_STEPS.length)}
        onClose={onClose}
      />
    </div>
  );
  return surface?.target ? createPortal(content, surface.target) : content;
}
