import { useEffect, useState } from "react";
import type { EffectiveHarness } from "../../lib/workbench/harnessTypes";
import {
  researcherClient,
  type ResearcherProfile,
} from "../../lib/researcherClient";
import { router } from "../../lib/router";

const OWN_WRITING = "own_writing";
/** Remembered per device once the researcher hides the suggestion. */
const DISMISSED_KEY = "pipeline.ui.yourWritingHintDismissed";

function dismissed(): boolean {
  try {
    return localStorage.getItem(DISMISSED_KEY) === "1";
  } catch {
    return false;
  }
}

function plural(count: number, noun: string) {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

/** What a profile with Your writing receives from About you, in a few words,
 *  or `null` when there is nothing. Matches what the harness sends: style
 *  notes, and papers marked as examples whose text could be read. */
export function writingInUse(profile: ResearcherProfile): string | null {
  const papers = profile.samples.filter(
    (sample) => sample.useAsExample && sample.words !== null,
  ).length;
  const style = profile.style.trim().length > 0;
  if (style && papers) return `your style notes and ${plural(papers, "paper")}`;
  if (style) return "your style notes";
  if (papers) return plural(papers, "paper");
  return null;
}

/**
 * One line in Assistant settings about Your writing: what the profile uses,
 * why it uses nothing, or, for a profile that would use it, where to add some.
 * Shown only for the Writing profile and profiles that turn Your writing on.
 */
export default function YourWritingStatus({
  effective,
}: {
  effective: EffectiveHarness;
}) {
  const wanted =
    (effective.preset.builtIn && effective.preset.id === "writing") ||
    effective.preset.modules.includes(OWN_WRITING);
  const turnedOff =
    effective.moduleAvailability?.find((module) => module.id === OWN_WRITING)
      ?.available === false;
  const [profile, setProfile] = useState<ResearcherProfile | null>(null);
  const [hidden, setHidden] = useState(dismissed);
  useEffect(() => {
    if (!wanted) return;
    let live = true;
    researcherClient
      .profile()
      .then((value) => {
        if (live) setProfile(value);
      })
      // Without About you there is nothing to say.
      .catch(() => {});
    return () => {
      live = false;
    };
    // The harness changes when About you does, so read it again then.
  }, [wanted, effective.fingerprint]);
  if (!wanted) return null;
  const openAboutYou = () =>
    void router.navigate({ page: "settings", section: "about" });
  const link = (
    <button type="button" className="underline" onClick={openAboutYou}>
      About you
    </button>
  );
  const line = "text-xs text-gray-600 dark:text-neutral-300";
  if (turnedOff)
    return (
      <p data-testid="your-writing-status" className={line}>
        About you is turned off for this project or conversation, so this
        profile doesn’t use your writing.
      </p>
    );
  if (!profile) return null;
  const summary = writingInUse(profile);
  if (summary)
    return (
      <p data-testid="your-writing-status" className={line}>
        Using {summary} from {link}.
      </p>
    );
  if (hidden) return null;
  return (
    <p
      data-testid="your-writing-status"
      className={`${line} flex items-start gap-2`}
    >
      <span className="flex-1">
        Add style notes or past papers in {link} so drafts sound like you.
      </span>
      <button
        type="button"
        aria-label="Hide this suggestion"
        className="text-gray-500"
        onClick={() => {
          try {
            localStorage.setItem(DISMISSED_KEY, "1");
          } catch {
            // Hidden for now, if not remembered.
          }
          setHidden(true);
        }}
      >
        ×
      </button>
    </p>
  );
}
