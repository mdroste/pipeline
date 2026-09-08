import { lazy, Suspense, useState } from "react";
import Jobs from "./research-studio/Jobs";
import { button, type StudioProps } from "./research-studio/shared";
const Manuscript = lazy(() => import("./research-studio/Manuscript"));
const Responses = lazy(() => import("./research-studio/Responses"));
const Experiments = lazy(() => import("./research-studio/Experiments"));
const Bindings = lazy(() => import("./research-studio/Bindings"));
const Literature = lazy(() => import("./research-studio/Literature"));
const Theory = lazy(() => import("./research-studio/Theory"));
export default function WorkspaceResearchStudio(props: StudioProps & {active?: boolean; destination?: string; tool?: "manuscript" | "responses" | "experiments" | "bindings" | "literature" | "theory"}) {
  const [selected, setTab] = useState(() => props.destination === "research" ? "experiments" : props.destination === "literature" ? "literature" : "manuscript");
  const tab = props.tool ?? selected;
  return (
    <div className="space-y-5">
      {!props.tool && <nav aria-label="Research tools" className="flex flex-wrap gap-2">
        {[
          ["manuscript", "Manuscript"],
          ["responses", "Responses"],
          ["experiments", "Experiments"],
          ["bindings", "Result links"],
          ["literature", "Literature"],
          ["theory", "Theory"],
        ].filter(([id]) => !props.destination || (props.destination === "research" ? ["experiments","bindings","theory"] : props.destination === "literature" ? ["literature"] : ["manuscript","responses"]).includes(id)).map(([id, label]) => (
          <button
            key={id}
            className={button}
            aria-current={tab === id ? "page" : undefined}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </nav>}
      <Jobs
        visible={props.active}
        workspaceId={props.workspaceId}
        onCompleted={() => void props.onRefresh()}
      />
      <Suspense
        fallback={<p className="p-4 text-sm">Loading research tools…</p>}
      >
        {tab === "manuscript" && <Manuscript {...props} />}{" "}
        {tab === "responses" && <Responses {...props} />}{" "}
        {tab === "experiments" && <Experiments {...props} />}{" "}
        {tab === "bindings" && <Bindings {...props} />}{" "}
        {tab === "literature" && <Literature {...props} />}{" "}
        {tab === "theory" && <Theory {...props} />}
      </Suspense>
    </div>
  );
}
