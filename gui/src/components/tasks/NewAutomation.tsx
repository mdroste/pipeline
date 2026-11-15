/** One creation entry point; each automation keeps its own preparation and runtime. */
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import type { TaskRun } from "../../lib/taskTypes";
import type { Mission } from "../../lib/missionClient";
import type { DiscoveryRun } from "../../lib/discoveryClient";
import IconButton from "../../ui/IconButton";
import { Icon } from "../../ui/icons";
import { confirmDialog } from "../DialogService";
import Builder, { type TemplateKind } from "./Builder";
import "../research-missions/Missions.css";
import "../self-discovery/discovery.css";
import "./Builder.css";

const ResearchBuilder = lazy(() => import("../research-missions/Builder"));
const DiscoveryBuilder = lazy(() => import("../self-discovery/Builder"));
type Kind = TemplateKind | "research" | "discovery";
const options: { id: Kind; title: string; description: string }[] = [
  {
    id: "review",
    title: "Review and revise",
    description:
      "Draft or load a paper, review it, and revise within your limits.",
  },
  {
    id: "prompt",
    title: "Follow up",
    description:
      "Continue a conversation now, later, or on a recurring schedule.",
  },
  {
    id: "research",
    title: "Investigate a question",
    description:
      "Plan, investigate, and challenge findings over several research rounds.",
  },
  {
    id: "discovery",
    title: "Discover research projects",
    description:
      "Explore a topic, compare proposals, and develop selected papers.",
  },
  {
    id: "input",
    title: "Wait for input",
    description: "Ask for missing information, then continue the conversation.",
  },
  {
    id: "blank",
    title: "Start empty",
    description:
      "Build your own sequence of assistant work, reviews, and checks.",
  },
];

export default function NewAutomation({
  initialSessionId,
  initialPath,
  skipChooser = false,
  onPrepared,
  onMissionPrepared,
  onDiscoveryStarted,
  onDirtyChange,
  onClose,
}: {
  initialSessionId?: string | null;
  skipChooser?: boolean;
  initialPath?: string | null;
  onPrepared: (run: TaskRun) => void;
  onMissionPrepared: (mission: Mission) => void;
  onDiscoveryStarted: (run: DiscoveryRun) => void;
  onDirtyChange: (dirty: boolean) => void;
  onClose: () => void;
}) {
  const [kind, setKind] = useState<Kind | null>(skipChooser ? "review" : null);
  const dirty = useRef(false);
  const heading = useRef<HTMLHeadingElement>(null);
  const changeDirty = useCallback(
    (value: boolean) => {
      dirty.current = value;
      onDirtyChange(value);
    },
    [onDirtyChange],
  );
  useEffect(() => {
    if (kind === null) {
      heading.current?.focus({ preventScroll: true });
      heading.current?.scrollIntoView?.({ block: "nearest" });
    }
  }, [kind]);
  useEffect(() => () => onDirtyChange(false), [onDirtyChange]);
  async function chooseType() {
    if (
      dirty.current &&
      !(await confirmDialog(
        "This automation has not been previewed or saved. Discard it?",
        {
          title: "Discard automation draft?",
          confirmLabel: "Discard",
          cancelLabel: "Keep editing",
          destructive: true,
        },
      ))
    )
      return;
    changeDirty(false);
    setKind(null);
  }
  return (
    <div className="automation-setup">
      {kind === null ? (
        <section className="automation-chooser" aria-label="New automation">
          <div className="task-section-heading">
            <div>
              <h2 ref={heading} tabIndex={-1}>
                New automation
              </h2>
              <p className="task-muted">What would you like Pipeline to do?</p>
            </div>
            <IconButton onClick={onClose} label="Close new automation">
              <Icon name="close" />
            </IconButton>
          </div>
          <div className="automation-type-grid">
            {options.map((option) => (
              <button
                key={option.id}
                className="automation-type-card"
                aria-label={option.title}
                onClick={() => setKind(option.id)}
              >
                <strong>
                  {option.title}
                  <span aria-hidden="true">→</span>
                </strong>
                <span>{option.description}</span>
              </button>
            ))}
          </div>
        </section>
      ) : (
        <Suspense
          fallback={<p className="task-empty">Loading automation setup…</p>}
        >
          {kind === "research" ? (
            <ResearchBuilder
              initialSessionId={initialSessionId}
              onPrepared={onMissionPrepared}
              onClose={onClose}
              onChooseType={() => void chooseType()}
            />
          ) : kind === "discovery" ? (
            <div onChangeCapture={() => changeDirty(true)}>
              <DiscoveryBuilder
                initialSessionId={initialSessionId}
                initialMode="supervised"
                onStarted={onDiscoveryStarted}
                onClose={onClose}
                onChooseType={() => void chooseType()}
              />
            </div>
          ) : (
            <Builder
              initialKind={kind}
              initialSessionId={initialSessionId}
              initialPath={initialPath}
              onPrepared={onPrepared}
              onDirtyChange={changeDirty}
              onClose={onClose}
              onChooseType={() => void chooseType()}
            />
          )}
        </Suspense>
      )}
    </div>
  );
}
