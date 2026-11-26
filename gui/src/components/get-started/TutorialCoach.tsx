import { useEffect, useRef, type RefObject } from "react";
import { Button } from "../../ui";
import { Icon } from "../../ui/icons";
import type { IconName } from "../../ui/icons";
import { useTutorialSpotlight } from "./useTutorialSpotlight";

export interface TutorialStep {
  id: string;
  title: string;
  body: string;
  target: string;
  interaction?: boolean;
  externalTarget?: boolean;
  actionTarget?: string;
  next?: string;
  prompt?: string;
}

export default function TutorialCoach({
  steps,
  label,
  icon,
  completionTitle,
  skipLabel,
  root,
  step,
  onNext,
  onBack,
  onRestart,
  onSkip,
  onExplore,
  onClose,
  closeLabel,
}: {
  steps: readonly TutorialStep[];
  label: string;
  icon: IconName;
  completionTitle: string;
  skipLabel?: string;
  root: RefObject<HTMLDivElement | null>;
  step: number;
  onNext: () => void;
  onBack: () => void;
  onRestart: () => void;
  onSkip: () => void;
  onExplore: () => void;
  onClose: () => void;
  closeLabel?: string;
}) {
  const coach = useRef<HTMLElement>(null);
  const title = useRef<HTMLHeadingElement>(null);
  const complete = step === steps.length;
  const guide = steps[Math.min(step, steps.length - 1)];
  const { rect, position } = useTutorialSpotlight(
    root,
    coach,
    complete ? null : guide.target,
    !!guide.interaction,
    step,
    {
      actionSelector: "actionTarget" in guide ? guide.actionTarget : undefined,
      externalTarget: !complete && !!guide.externalTarget,
      onExternalActivate: onNext,
    },
  );
  useEffect(() => {
    title.current?.focus({ preventScroll: true });
  }, [step]);
  return (
    <>
      {!complete && (
        <>
          <svg
            className="review-tutorial-mask"
            aria-hidden="true"
            width="100%"
            height="100%"
          >
            <path
              fillRule="evenodd"
              d={`M0 0H${window.innerWidth}V${window.innerHeight}H0Z ${rect ? `M${rect.left} ${rect.top}v${rect.height}h${rect.width}v-${rect.height}Z` : ""}`}
            />
          </svg>
          {rect && (
            <div
              className="review-tutorial-spotlight"
              style={rect}
              aria-hidden="true"
            >
              <span className="review-tutorial-beacon" />
            </div>
          )}
        </>
      )}
      <aside
        ref={coach}
        className={`review-tutorial-coach ${complete ? "is-complete" : ""}`}
        style={position ?? undefined}
        aria-label={label}
      >
        <header>
          <span>
            <Icon name={complete ? "check" : icon} />
            {complete ? "Tutorial complete" : label}
          </span>
          <button
            type="button"
            aria-label={closeLabel ?? "Close example"}
            title={closeLabel ?? "Close tutorial"}
            onClick={onClose}
          >
            <Icon name="close" />
          </button>
        </header>
        {!complete && (
          <div
            className="review-tutorial-meter"
            aria-label={`Step ${step + 1} of ${steps.length}`}
          >
            <span
              style={{
                width: `${((step + 1) / steps.length) * 100}%`,
              }}
            />
          </div>
        )}
        <h2 ref={title} tabIndex={-1}>
          {complete ? completionTitle : guide.title}
        </h2>
        <p>
          {complete
            ? "Keep exploring the sample, or finish to return to your workspace."
            : guide.body}
        </p>
        <div className="review-tutorial-actions">
          {!complete && (
            <Button variant="link" onClick={onBack} disabled={step === 0}>
              <Icon name="arrow-left" /> Back
            </Button>
          )}
          {complete ? (
            <Button variant="primary" onClick={onClose}>
              {closeLabel ?? "Finish tutorial"}
            </Button>
          ) : guide.next ? (
            <Button variant="primary" onClick={onNext}>
              {guide.next}
              <Icon name="chevron-right" />
            </Button>
          ) : (
            <span className="review-tutorial-prompt">
              <Icon name="arrow-up" />
              {guide.prompt}
            </span>
          )}
        </div>
        <footer>
          <button type="button" onClick={onRestart} disabled={step === 0}>
            Restart
          </button>
          {!complete &&
            (skipLabel ? (
              <button type="button" onClick={onSkip}>
                {skipLabel}
              </button>
            ) : (
              <button type="button" onClick={onExplore}>
                Explore freely
              </button>
            ))}
          <span>{!complete && `${step + 1} / ${steps.length}`}</span>
        </footer>
      </aside>
    </>
  );
}
