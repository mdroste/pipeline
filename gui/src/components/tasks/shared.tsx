import { stateLabel, type Step } from "../../lib/taskClient";

export function Status({ state }: { state: string }) {
  return (
    <span className={`task-status task-status--${state}`}>
      <i />
      {stateLabel(state)}
    </span>
  );
}
export function Outline({
  steps,
  level = 0,
}: {
  steps: Step[];
  level?: number;
}) {
  return (
    <ol
      className="task-outline"
      aria-label={level ? "Nested steps" : "Automation steps"}
    >
      {steps.map((step, i) => (
        <li key={step.id}>
          <div className="task-outline-row">
            <span className="task-step-number">{i + 1}</span>
            <span>{step.label}</span>
            <small>
              {step.kind === "repeat"
                ? `up to ${step.maxIterations} rounds`
                : step.kind === "delay"
                  ? `${step.seconds / 60} min`
                  : step.kind === "parallel"
                    ? "in parallel"
                    : step.kind}
            </small>
          </div>
          {step.kind === "repeat" ||
          step.kind === "while" ||
          step.kind === "forEach" ? (
            <Outline steps={step.steps} level={level + 1} />
          ) : step.kind === "if" ? (
            <>
              <Outline steps={step.thenSteps} level={level + 1} />
              {!!step.elseSteps?.length && (
                <Outline steps={step.elseSteps} level={level + 1} />
              )}
            </>
          ) : step.kind === "parallel" ? (
            step.branches.map((branch, n) => (
              <Outline key={n} steps={branch} level={level + 1} />
            ))
          ) : step.kind === "chain" ? (
            <Outline steps={step.chain.steps} level={level + 1} />
          ) : null}
        </li>
      ))}
    </ol>
  );
}
