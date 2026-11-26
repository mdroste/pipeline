import { WORKFLOW_GALLERY } from "./workflowGallery";
import type { ProfileSummary } from "./workflowTypes";
const BUILTINS: Record<
  string,
  { category: string; outcome: string; inputs: string }
> = {
  "auto-paper-review": {
    category: "Papers",
    outcome: "A detailed review of an academic research paper.",
    inputs: "Paper or LaTeX source",
  },
  "auto-paper-review-fast": {
    category: "Papers",
    outcome: "A quick review of an academic research paper.",
    inputs: "Paper or LaTeX source",
  },
  "slide-review": {
    category: "Presentations",
    outcome: "A review of a presentation slide deck.",
    inputs: "PDF, PowerPoint (.pptx), or Beamer (.tex) slides",
  },
  "grant-review": {
    category: "Proposals",
    outcome: "A review of a grant proposal.",
    inputs: "Grant proposal",
  },
  declaude: {
    category: "Writing",
    outcome: "Edits for clearer, more natural writing.",
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
