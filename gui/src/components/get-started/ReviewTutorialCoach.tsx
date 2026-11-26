import type { ComponentProps } from "react";
import TutorialCoach from "./TutorialCoach";
import { REVIEW_TUTORIAL_STEPS, TUTORIAL_STEP } from "./reviewTutorialSteps";
export default function ReviewTutorialCoach(
  props: Omit<
    ComponentProps<typeof TutorialCoach>,
    "steps" | "label" | "icon" | "completionTitle" | "skipLabel"
  >,
) {
  return (
    <TutorialCoach
      {...props}
      steps={REVIEW_TUTORIAL_STEPS}
      label="Review tutorial"
      icon="review"
      completionTitle="Ready to try your own paper?"
      skipLabel={
        props.step < TUTORIAL_STEP.results ? "Skip to results" : undefined
      }
    />
  );
}
