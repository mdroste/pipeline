import { useEffect, useId, useRef, type ReactNode } from "react";
import useModalDialog from "../../hooks/useModalDialog";
import {
  GET_STARTED_PERSONAS,
  type GetStartedPersona,
} from "../../lib/getStarted";
import type { DepsReport } from "../../lib/types";
import { WORKFLOW_GALLERY } from "../../lib/workflowGallery";
import { Button, IconButton } from "../../ui";
import { Icon } from "../../ui/icons";
import ConnectionsStep from "./ConnectionsStep";
import ToolsStep from "./ToolsStep";
import { PERSONAS, personaGuide, type StarterTemplate } from "./guide";

export const GET_STARTED_STEPS = [
  "Your work",
  "Connections",
  "Tools",
  "Next steps",
] as const;

export interface GetStartedSheetProps {
  step: number;
  onStepChange: (step: number) => void;
  persona: GetStartedPersona | null;
  onPersonaChange: (persona: GetStartedPersona) => void;
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

function PersonaStep({
  persona,
  onPersonaChange,
  headingId,
}: {
  persona: GetStartedPersona | null;
  onPersonaChange: (persona: GetStartedPersona) => void;
  headingId: string;
}) {
  return (
    <fieldset aria-labelledby={headingId} className="space-y-3">
      <div>
        <h3
          id={headingId}
          tabIndex={-1}
          className="text-ui-body font-semibold outline-none"
        >
          What do you work with?
        </h3>
        <p className="mt-1 text-ui-label text-ink-muted">
          Your answer only chooses which tips and tools this guide shows first.
          It does not change any setting.
        </p>
      </div>
      <div className="grid gap-2 sm:grid-cols-2">
        {GET_STARTED_PERSONAS.map((id, index) => {
          const checked = persona === id;
          return (
            <label
              key={id}
              className={`flex cursor-pointer items-start gap-3 rounded-ui-md border px-3 py-2.5 focus-within:outline focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-accent ${
                checked
                  ? "border-accent bg-accent-soft"
                  : "border-line hover:bg-sunken"
              }`}
            >
              <input
                type="radio"
                name="get-started-persona"
                value={id}
                checked={checked}
                onChange={() => onPersonaChange(id)}
                data-autofocus={
                  checked || (!persona && index === 0) ? true : undefined
                }
                className="mt-1"
              />
              <span>
                <span className="block text-ui-label font-medium">
                  {PERSONAS[id].label}
                </span>
                <span className="block text-ui-meta text-ink-muted">
                  {PERSONAS[id].description}
                </span>
              </span>
            </label>
          );
        })}
      </div>
    </fieldset>
  );
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
  persona,
  headingId,
  onOpenExample,
  onNewReview,
  onOpenTemplate,
  onNewProject,
}: Pick<
  GetStartedSheetProps,
  | "persona"
  | "onOpenExample"
  | "onNewReview"
  | "onOpenTemplate"
  | "onNewProject"
> & { headingId: string }) {
  const guide = personaGuide(persona);
  const templates = guide.templates.flatMap((id) => {
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
          title="See an example Review"
          detail="A sample Review of a fictional paper. No connection or model call is needed."
        >
          <Button variant="primary" onClick={onOpenExample}>
            Open the example
          </Button>
        </Action>
        <Action title="Start your first Review" detail={guide.reviewHint}>
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
      <div>
        <h4 className="text-ui-label font-semibold">
          {persona ? `Tips for ${guide.label}` : "Tips"}
        </h4>
        <ul className="mt-1.5 list-disc space-y-1 pl-5 text-ui-label text-ink-muted">
          {guide.tips.map((tip) => (
            <li key={tip}>{tip}</li>
          ))}
        </ul>
      </div>
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
            <PersonaStep
              persona={props.persona}
              onPersonaChange={props.onPersonaChange}
              headingId={headingId}
            />
          ) : step === 1 ? (
            <ConnectionsStep
              dependencies={props.dependencies}
              dependenciesLoading={props.dependenciesLoading}
              onCheckDependencies={props.onCheckDependencies}
              onOpenConnections={props.onOpenConnections}
              headingId={headingId}
            />
          ) : step === 2 ? (
            <ToolsStep
              persona={props.persona}
              dependencies={props.dependencies}
              onOpenPdfSettings={props.onOpenPdfSettings}
              headingId={headingId}
            />
          ) : (
            <NextStepsStep
              persona={props.persona}
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
            <Button variant="primary" onClick={() => onStepChange(step + 1)}>
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
