import ReviewRunView from "../ReviewRunView";
import type { QueuedRun } from "../../lib/reviewTypes";
import type { PassStatus, RuntimeStage } from "../../lib/reviewRuntimeTypes";
import { SAMPLE_PATH, SAMPLE_PLAN } from "./reviewTutorialFixture";

export default function ReviewTutorialRun({
  phase,
  onCancel,
  onOpen,
}: {
  phase: "review" | "validate" | "done";
  onCancel: () => void;
  onOpen: () => void;
}) {
  const run: QueuedRun = {
    key: "example-only",
    kind: "review",
    label: "port-harlan-brt.pdf",
    inputPath: SAMPLE_PATH,
    profileName: "Paper Review",
    status: phase === "done" ? "done" : "running",
    stage: phase === "review" ? "dispatching" : "synthesizing",
    stageLabel: null,
    waiting: null,
    queuedAt: 0,
    startedAt: null,
    finishedAt: null,
    runId: null,
    error: null,
    partial: false,
    resultAvailable: false,
  };
  const passes = Object.fromEntries(
    SAMPLE_PLAN.stages[2].stepIds.map((id, index) => [
      id,
      (phase !== "review" || index < 2 ? "done" : "running") as PassStatus,
    ]),
  );
  const validating = {
    auto_synthesis: "done",
    auto_validate: "running",
  } as const;
  const history: RuntimeStage[] = SAMPLE_PLAN.stages
    .slice(0, phase === "review" ? 3 : 4)
    .map((stage, index) => ({
      ...stage,
      status: index === (phase === "review" ? 2 : 3) ? "active" : "done",
      passes: index === 2 ? passes : index === 3 ? validating : {},
    }));
  return (
    <ReviewRunView
      run={run}
      runs={[run]}
      limit={1}
      state={
        phase === "review"
          ? { kind: "dispatching", passes }
          : { kind: "synthesizing", passes: validating }
      }
      plan={SAMPLE_PLAN.stages}
      stageHistory={history}
      runStartedAt={null}
      passTimes={{}}
      reviewRouting={null}
      allowPassCancellation={false}
      onCancel={onCancel}
      onNewReview={onCancel}
      onOpenHistory={onOpen}
    />
  );
}
