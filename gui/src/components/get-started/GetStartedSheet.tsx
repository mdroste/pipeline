import { useEffect, useId, useRef, type ReactNode } from "react";
import useModalDialog from "../../hooks/useModalDialog";
import type { DepsReport } from "../../lib/types";
import { WORKFLOW_GALLERY } from "../../lib/workflowGallery";
import { Button, IconButton } from "../../ui";
import { Icon } from "../../ui/icons";
import ConnectionsStep from "./ConnectionsStep";
import ToolsStep from "./ToolsStep";
import { STARTER_TEMPLATES, type StarterTemplate } from "./guide";

export const GET_STARTED_STEPS = [
  "Connections",
  "Tools",
  "Next steps",
] as const;

export interface GetStartedSheetProps {
  step: number;
  onStepChange: (step: number) => void;
  dependencies: DepsReport | null;
  dependenciesLoading: boolean;
  onCheckDependencies: () => void;
  onClose: () => void;
  onOpenConnections: (target?: string) => void;
  onOpenPdfSettings: () => void;
  onOpenExample: () => void;
  onNewReview: () => void;
  onOpenTemplate: (id: StarterTemplate) => void;
  onNewProject: () => void;
}

function Action({
  title,
  detail,
  children,
}: {
  title: string;
  detail: string;
  children: ReactNode;
}) {
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-ui-md border border-line px-3 py-2.5">
      <div className="min-w-[12rem] flex-1">
        <p className="text-ui-label font-medium">{title}</p>
        <p className="text-ui-meta text-ink-muted">{detail}</p>
      </div>
      {children}
    </li>
  );
}

function NextStepsStep({
  headingId,
  onOpenExample,
  onNewReview,
  onOpenTemplate,
  onNewProject,
}: Pick<
  GetStartedSheetProps,
  "onOpenExample" | "onNewReview" | "onOpenTemplate" | "onNewProject"
> & { headingId: string }) {
  const templates = STARTER_TEMPLATES.flatMap((id) => {
    const item = WORKFLOW_GALLERY.find((candidate) => candidate.id === id);
    return item ? [{ ...item, id }] : [];
  });
  return (
    <section aria-labelledby={headingId} className="space-y-4">
      <h3
        id={headingId}
        tabIndex={-1}
        className="text-ui-body font-semibold outline-none"
      >
        Next steps
      </h3>
      <ul aria-label="Suggested next steps" className="space-y-2">
        <Action
          title="See an example review"
          detail="A sample Review of a fictional paper. No connection or model call is needed."
        >
          <Button variant="primary" onClick={onOpenExample}>
            Open the example
          </Button>
        </Action>
        <Action
          title="Start your first Review"
          detail="Choose a PDF, Word file, .tex file, or folder. LaTeX source keeps equations and structure."
        >
          <Button onClick={onNewReview}>New review</Button>
        </Action>
        {templates.map((item) => (
          <Action
            key={item.id}
            title={`Template: ${item.name}`}
            detail={item.description}
          >
            <Button
              aria-label={`View ${item.name} in the gallery`}
              onClick={() => onOpenTemplate(item.id)}
            >
              View in gallery
            </Button>
          </Action>
        ))}
        <Action
          title="Create a project"
          detail="Projects keep conversations, files, notes, and local checks together."
        >
          <Button onClick={onNewProject}>New project</Button>
        </Action>
      </ul>
    </section>
  );
}

/** First-run guide. It reads status and links to Settings; it never saves settings. */
export default function GetStartedSheet(props: GetStartedSheetProps) {
  const { step, onStepChange, onClose } = props;
  const titleId = useId();
  const descriptionId = useId();
  const headingId = useId();
  const dialogRef = useModalDialog<HTMLDivElement>(onClose, true, true);
  const previousStep = useRef(step);
  useEffect(() => {
    if (previousStep.current === step) return;
    previousStep.current = step;
    document.getElementById(headingId)?.focus();
  }, [step, headingId]);
  const last = GET_STARTED_STEPS.length - 1;

  return (
    <div
      data-testid="get-started"
      className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-2 sm:p-6"
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        className="flex max-h-[calc(100vh-1rem)] w-full max-w-2xl flex-col overflow-hidden rounded-ui-lg border border-line bg-raised text-ink shadow-ui-3 outline-none sm:max-h-[calc(100vh-3rem)]"
      >
        <header className="shrink-0 border-b border-line px-4 pb-3 pt-4 sm:px-5">
          <div className="flex items-start gap-3">
            <div className="min-w-0 flex-1">
              <h2 id={titleId} className="text-ui-title font-semibold">
                Get started with Pipeline
              </h2>
              <p
                id={descriptionId}
                className="mt-0.5 text-ui-label text-ink-muted"
              >
                Check what is ready on this computer and choose a first step.
                You can reopen this guide from Home or Settings.
              </p>
            </div>
            <IconButton label="Close setup guide" onClick={onClose}>
              <Icon name="close" className="h-4 w-4" />
            </IconButton>
          </div>
          <ol aria-label="Setup steps" className="mt-3 flex flex-wrap gap-1.5">
            {GET_STARTED_STEPS.map((label, index) => (
              <li key={label}>
                <button
                  type="button"
                  aria-current={index === step ? "step" : undefined}
                  onClick={() => onStepChange(index)}
                  className={`rounded-full px-2.5 py-1 text-ui-meta font-medium focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${
                    index === step
                      ? "bg-inverse text-on-inverse"
                      : "bg-sunken text-ink-muted hover:text-ink"
                  }`}
                >
                  {index + 1}. {label}
                </button>
              </li>
            ))}
          </ol>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4 sm:px-5">
          {step === 0 ? (
            <ConnectionsStep
              dependencies={props.dependencies}
              dependenciesLoading={props.dependenciesLoading}
              onCheckDependencies={props.onCheckDependencies}
              onOpenConnections={props.onOpenConnections}
              headingId={headingId}
            />
          ) : step === 1 ? (
            <ToolsStep
              dependencies={props.dependencies}
              onOpenPdfSettings={props.onOpenPdfSettings}
              headingId={headingId}
            />
          ) : (
            <NextStepsStep
              headingId={headingId}
              onOpenExample={props.onOpenExample}
              onNewReview={props.onNewReview}
              onOpenTemplate={props.onOpenTemplate}
              onNewProject={props.onNewProject}
            />
          )}
        </div>

        <footer className="flex shrink-0 flex-wrap items-center gap-2 border-t border-line px-4 py-3 sm:px-5">
          <Button variant="link" onClick={onClose}>
            Skip for now
          </Button>
          <span className="ml-auto" />
          {step > 0 && (
            <Button onClick={() => onStepChange(step - 1)}>Back</Button>
          )}
          {step < last ? (
            <Button
              variant="primary"
              data-autofocus
              onClick={() => onStepChange(step + 1)}
            >
              Next
            </Button>
          ) : (
            <Button variant="primary" onClick={onClose}>
              Done
            </Button>
          )}
        </footer>
      </div>
    </div>
  );
}
