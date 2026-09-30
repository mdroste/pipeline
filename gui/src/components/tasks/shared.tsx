import { stateLabel, type Chain, type Step } from "../../lib/taskClient";
import { describeStep } from "./language";

export function Status({ state }: { state: string }) {
  return (
    <span className={`task-status task-status--${state}`}>
      <i />
      {stateLabel(state)}
    </span>
  );
}

/** Read-only outline: each row is the step name plus a plain-language
 * sentence — never an internal kind id. */
export function Outline({
  steps,
  chain,
  level = 0,
}: {
  steps: Step[];
  chain?: Chain;
  level?: number;
}) {
  const scope: Chain = chain ?? {
    schemaVersion: 1,
    name: "",
    description: "",
    steps,
    limits: { maxActions: 64, deadlineHours: 168, actionTimeoutSecs: 7200 },
  };
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
            <small>{describeStep(scope, step)}</small>
          </div>
          {step.kind === "repeat" ||
          step.kind === "while" ||
          step.kind === "forEach" ? (
            <Outline steps={step.steps} chain={scope} level={level + 1} />
          ) : step.kind === "if" ? (
            <>
              <Outline steps={step.thenSteps} chain={scope} level={level + 1} />
              {!!step.elseSteps?.length && (
                <Outline
                  steps={step.elseSteps}
                  chain={scope}
                  level={level + 1}
                />
              )}
            </>
          ) : step.kind === "parallel" ? (
            step.branches.map((branch, n) => (
              <Outline key={n} steps={branch} chain={scope} level={level + 1} />
            ))
          ) : step.kind === "chain" ? (
            <Outline steps={step.chain.steps} chain={scope} level={level + 1} />
          ) : null}
        </li>
      ))}
    </ol>
  );
}
