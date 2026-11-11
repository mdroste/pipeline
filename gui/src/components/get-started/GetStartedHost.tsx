import { lazy, Suspense, useEffect, useState } from "react";
import {
  onGetStartedRequest,
  saveGetStarted,
  readGetStarted,
  shouldOfferGetStarted,
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
  const [step, setStepState] = useState(() => readGetStarted()?.step ?? 0);
  const [goal, setGoalState] = useState<"review" | "project">(
    () => readGetStarted()?.goal ?? "review",
  );
  const [pendingSettings, setPendingSettings] = useState(
    () => !!readGetStarted()?.pendingSettings,
  );
  const setStep = (value: number) => {
    setStepState(value);
    saveGetStarted({ step: value });
  };
  const setGoal = (value: "review" | "project") => {
    setGoalState(value);
    saveGetStarted({ goal: value });
  };
  const openSettings = async (section: string, targetId?: string) => {
    setView(null);
    const allowed = await router.navigate({
      page: "settings",
      section,
      targetId,
    });
    if (allowed) {
      setPendingSettings(true);
      saveGetStarted({ pendingSettings: true, step, goal });
    } else setView("setup");
  };
  const [returnToSetup, setReturnToSetup] = useState(false);

  useEffect(
    () =>
      onGetStartedRequest((next) => {
        setReturnToSetup(false);
        setPendingSettings(false);
        saveGetStarted({ pendingSettings: false });
        setView(next);
      }),
    [],
  );

  const close = () => {
    saveGetStarted({
      dismissedAt: new Date().toISOString(),
      pendingSettings: false,
    });
    setPendingSettings(false);
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
  if (view !== "setup")
    return pendingSettings ? (
      <aside
        aria-label="Continue setup"
        className="fixed bottom-5 right-5 z-40 flex items-center gap-3 rounded-xl border border-line bg-raised p-3 text-ui-label shadow-ui-2"
      >
        <span>
          Continue setting up your {goal === "review" ? "review" : "project"}.
        </span>
        <button
          className="rounded-md bg-inverse px-3 py-2 text-on-inverse"
          onClick={() => {
            setPendingSettings(false);
            saveGetStarted({ pendingSettings: false });
            onCheckDependencies();
            setView("setup");
          }}
        >
          Return to setup
        </button>
        <button aria-label="Dismiss setup reminder" onClick={close}>
          ×
        </button>
      </aside>
    ) : null;
  return (
    <Suspense fallback={null}>
      <GetStartedSheet
        goal={goal}
        onGoalChange={setGoal}
        step={step}
        onStepChange={setStep}
        dependencies={dependencies}
        dependenciesLoading={dependenciesLoading}
        onCheckDependencies={onCheckDependencies}
        onClose={close}
        onOpenConnections={(targetId) =>
          void openSettings("providers", targetId)
        }
        onOpenPdfSettings={() =>
          void openSettings("extraction", "workflow-extraction")
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
