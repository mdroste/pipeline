import { useEffect, useId, useRef } from "react";
import useModalDialog from "../../hooks/useModalDialog";
import type { DepsReport } from "../../lib/types";
import { Button, IconButton } from "../../ui";
import { Icon } from "../../ui/icons";
import ConnectionsStep from "./ConnectionsStep";
import ToolsStep from "./ToolsStep";
import { type StarterTemplate } from "./guide";

export const GET_STARTED_STEPS = [
  "Your task",
  "Connections",
  "Optional tools",
] as const;

export interface GetStartedSheetProps {
  goal?: "review" | "project";
  onGoalChange?: (goal: "review" | "project") => void;
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
                Choose what you want to do. You can add connections and tools as
                you need them.
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
            <section aria-labelledby={headingId} className="space-y-4">
              <h3
                id={headingId}
                tabIndex={-1}
                className="font-semibold outline-none"
              >
                Choose your first task
              </h3>
              <div className="grid gap-3 sm:grid-cols-2">
                {(["review", "project"] as const).map((goal) => (
                  <button
                    type="button"
                    key={goal}
                    aria-pressed={(props.goal ?? "review") === goal}
                    onClick={() => {
                      props.onGoalChange?.(goal);
                      onStepChange(1);
                    }}
                    className="rounded-xl border border-line p-4 text-left hover:bg-sunken aria-pressed:bg-sunken"
                  >
                    <span className="block font-medium">
                      {goal === "review"
                        ? "Review a document"
                        : "Start a project"}
                    </span>
                    <span className="mt-2 block text-ui-label text-ink-muted">
                      {goal === "review"
                        ? "Get feedback on a paper, proposal, or presentation."
                        : "Keep files, chats, and notes together."}
                    </span>
                  </button>
                ))}
              </div>
              <button
                type="button"
                className="text-ui-label underline"
                onClick={props.onOpenExample}
              >
                Explore an example review
              </button>
              <p className="text-ui-meta text-ink-muted">
                The example uses a saved report. No account or model call is
                needed.
              </p>
            </section>
          ) : step === 1 ? (
            <ConnectionsStep
              dependencies={props.dependencies}
              dependenciesLoading={props.dependenciesLoading}
              onCheckDependencies={props.onCheckDependencies}
              onOpenConnections={props.onOpenConnections}
              headingId={headingId}
            />
          ) : (
            <ToolsStep
              goal={props.goal ?? "review"}
              dependencies={props.dependencies}
              onOpenPdfSettings={props.onOpenPdfSettings}
              headingId={headingId}
            />
          )}
        </div>

        <footer className="flex shrink-0 flex-wrap items-center gap-2 border-t border-line px-4 py-3 sm:px-5">
          <Button variant="link" onClick={onClose}>
            Skip for now
          </Button>
          <span className="ml-auto" />
          {step > 0 && (
            <Button
              variant="primary"
              onClick={
                (props.goal ?? "review") === "review"
                  ? props.onNewReview
                  : props.onNewProject
              }
            >
              {(props.goal ?? "review") === "review"
                ? "Continue to review"
                : "Create project"}
            </Button>
          )}
          {step > 0 && (
            <Button onClick={() => onStepChange(step - 1)}>Back</Button>
          )}
          {step < last ? (
            <Button
              variant={step === 0 ? "primary" : "default"}
              data-autofocus
              onClick={() => onStepChange(step + 1)}
            >
              {step === 0 ? "Check connections" : "See optional tools"}
            </Button>
          ) : (
            <Button onClick={onClose}>Close guide</Button>
          )}
        </footer>
      </div>
    </div>
  );
}
