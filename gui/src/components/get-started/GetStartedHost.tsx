import { lazy, Suspense, useEffect, useState } from "react";
import {
  onGetStartedRequest,
  readGetStarted,
  saveGetStarted,
  shouldOfferGetStarted,
  type GetStartedPersona,
  type GetStartedView,
} from "../../lib/getStarted";
import { router } from "../../lib/router";
import type { DepsReport } from "../../lib/types";

// The guide and the sample Review load only when opened.
const GetStartedSheet = lazy(() => import("./GetStartedSheet"));
const ExampleReview = lazy(() => import("./ExampleReview"));

/** Focus a destination element once its lazily loaded page has rendered. */
export function focusWhenPresent(id: string, attempts = 40) {
  const target = document.getElementById(id);
  if (target) {
    target.scrollIntoView?.({ block: "center" });
    target.focus({ preventScroll: true });
    return;
  }
  if (attempts > 0)
    window.setTimeout(() => focusWhenPresent(id, attempts - 1), 50);
}

/**
 * Owns when the Get started sheet and the Example Review are open. The sheet
 * opens by itself once on a first launch; Home and Settings reopen it through
 * `openGetStarted`.
 */
export default function GetStartedHost({
  dependencies,
  dependenciesLoading,
  onCheckDependencies,
  onNewReview,
  onNewProject,
}: {
  dependencies: DepsReport | null;
  dependenciesLoading: boolean;
  onCheckDependencies: () => void;
  onNewReview: () => void;
  onNewProject: () => void;
}) {
  // Decided during the first render, before any startup effect stores state.
  // WebDriver builds (`--mode e2e`) drive fresh profiles and never auto-open.
  const [view, setView] = useState<GetStartedView | null>(() =>
    import.meta.env.MODE !== "e2e" && shouldOfferGetStarted() ? "setup" : null,
  );
  const [step, setStep] = useState(0);
  const [returnToSetup, setReturnToSetup] = useState(false);
  const [persona, setPersona] = useState<GetStartedPersona | null>(
    () => readGetStarted()?.persona ?? null,
  );

  useEffect(
    () =>
      onGetStartedRequest((next) => {
        setReturnToSetup(false);
        if (next === "setup") setStep(0);
        setView(next);
      }),
    [],
  );

  const close = () => {
    saveGetStarted({ dismissedAt: new Date().toISOString() });
    setView(null);
  };
  // Close first, so a navigation guard's confirmation is not hidden behind
  // this dialog.
  const leave = (action: () => void) => {
    close();
    action();
  };

  if (view === "example")
    return (
      <Suspense fallback={null}>
        <ExampleReview
          backLabel={returnToSetup ? "Back to setup" : undefined}
          onClose={() => {
            setView(returnToSetup ? "setup" : null);
            setReturnToSetup(false);
          }}
        />
      </Suspense>
    );
  if (view !== "setup") return null;
  return (
    <Suspense fallback={null}>
      <GetStartedSheet
        step={step}
        onStepChange={setStep}
        persona={persona}
        onPersonaChange={(next) => {
          setPersona(next);
          saveGetStarted({ persona: next });
        }}
        dependencies={dependencies}
        dependenciesLoading={dependenciesLoading}
        onCheckDependencies={onCheckDependencies}
        onClose={close}
        onOpenConnections={(targetId) =>
          leave(
            () =>
              void router.navigate({
                page: "settings",
                section: "providers",
                ...(targetId ? { targetId } : {}),
              }),
          )
        }
        onOpenPdfSettings={() =>
          leave(
            () =>
              void router.navigate({
                page: "settings",
                section: "extraction",
                targetId: "workflow-extraction",
              }),
          )
        }
        onOpenExample={() => {
          setReturnToSetup(true);
          setView("example");
        }}
        onNewReview={() => leave(onNewReview)}
        onOpenTemplate={(id) =>
          leave(
            () =>
              void router.navigate({ page: "gallery" }).then((allowed) => {
                if (allowed) focusWhenPresent(`gallery-${id}`);
              }),
          )
        }
        onNewProject={() => leave(onNewProject)}
      />
    </Suspense>
  );
}
