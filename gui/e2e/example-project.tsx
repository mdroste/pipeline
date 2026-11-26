// Browser fixture using the production tour, with no native/backend bridge.
import { useState } from "react";
import { createRoot } from "react-dom/client";
import ExampleProject from "../src/components/get-started/ExampleProject";
import NavRail from "../src/components/NavRail";
import {
  ExampleReviewSurface,
  ExampleReviewSurfaceProvider,
  useExampleReviewSurface,
} from "../src/components/get-started/ExampleReviewSurface";
import "../src/App.css";

const query = new URLSearchParams(location.search);
document.documentElement.classList.toggle("dark", query.has("dark"));

function Fixture() {
  const [open, setOpen] = useState(true);
  const surface = useExampleReviewSurface();
  return (
    <div className="flex h-screen bg-surface text-ink">
      <NavRail
        activePage={surface?.page ?? "home"}
        transientNavigation={!!surface?.page}
        transientProjectNavigation={surface?.projectNavigation}
        hasCurrentRun={false}
        dependenciesReady
        dependenciesLoading={false}
        width={216}
        collapsed={query.has("collapsed")}
        onResize={() => {}}
        onNewRun={() => {}}
        onNavigate={() => {}}
        onDependencies={() => {}}
      />
      <ExampleReviewSurface className="min-w-0 flex-1">
        <button className="ui-button m-12" onClick={() => setOpen(true)}>
          Explore an example project
        </button>
      </ExampleReviewSurface>
      {open && <ExampleProject onClose={() => setOpen(false)} />}
    </div>
  );
}
createRoot(document.getElementById("root")!).render(
  <ExampleReviewSurfaceProvider>
    <Fixture />
  </ExampleReviewSurfaceProvider>,
);
