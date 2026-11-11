import { WORKFLOW_GALLERY } from "./workflowGallery";
import type { ProfileSummary } from "./types";
const BUILTINS: Record<
  string,
  { category: string; outcome: string; inputs: string }
> = {
  "auto-paper-review": {
    category: "Papers",
    outcome:
      "A detailed review of claims, methods, and evidence, with specialists chosen for your paper.",
    inputs: "Paper or LaTeX source",
  },
  "auto-paper-review-fast": {
    category: "Papers",
    outcome:
      "A focused first pass with fewer specialists. Use it to identify the main issues before a deeper review.",
    inputs: "Paper or LaTeX source",
  },
  "slide-review": {
    category: "Presentations",
    outcome: "Feedback on the argument, evidence, and clarity of a slide deck.",
    inputs: "Slides exported as PDF",
  },
  "grant-review": {
    category: "Proposals",
    outcome:
      "A review of the proposal's aims, methods, feasibility, and case for support.",
    inputs: "Grant proposal",
  },
  declaude: {
    category: "Writing",
    outcome: "Suggested edits to make writing clearer and more natural.",
    inputs: "Draft document",
  },
};
export function workflowDescription(profile: ProfileSummary) {
  const builtin = profile.builtin && BUILTINS[profile.id];
  const gallery = WORKFLOW_GALLERY.find(
    (item) => item.id === profile.id || item.name === profile.name,
  );
  return (
    builtin ||
    (gallery
      ? {
          category: gallery.category,
          outcome: gallery.outcome,
          inputs: gallery.inputs,
        }
      : {
          category: "Custom",
          outcome: "A saved workflow. Open its steps to see what it produces.",
          inputs: "Defined by this workflow",
        })
  );
}
