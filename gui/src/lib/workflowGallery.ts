import type { PipelineConfig, StepConfig } from "./types";

export interface WorkflowGalleryItem {
  id: string;
  name: string;
  category: string;
  description: string;
  inputs: string;
  outcome: string;
  config: PipelineConfig;
}

const genericParallelContext = `You are one independent reviewer in a structured workflow.

Use only the artifacts made available to this step. Read source files or document assets when the task requires evidence. Do not claim to have inspected material that is not present.

INPUT: {input_path}
SURVEY: {orientation}

TASK
{step_prompt}`;

const issuesSchema = {
  type: "object",
  required: ["issues"],
  properties: {
    issues: {
      type: "array",
      items: {
        type: "object",
        required: ["id", "title", "severity", "section", "body"],
        properties: {
          id: { type: "string" },
          title: { type: "string" },
          severity: { type: "string" },
          section: { type: "string" },
          body: { type: "string" },
          evidence: {
            type: "array",
            items: {
              type: "object",
              properties: {
                page: { type: "integer" },
                line_start: { type: "integer" },
                line_end: { type: "integer" },
                node_id: { type: "string" },
                asset_id: { type: "string" },
                artifact_path: { type: "string" },
                description: { type: "string" },
                quote: { type: "string" },
              },
            },
          },
        },
      },
    },
  },
};

function issueOutputInstruction(max = 20): string {
  return `Populate the configured issues schema. Each issue must contain id, title, severity (high, medium, or low), section, body, and an evidence array when the source supports a precise citation. Evidence entries may include page, line_start, line_end, node_id, asset_id, artifact_path, description, and a short quote. Use artifact_path only for a path available in the run's saved Sources; otherwise put a source-file path and line range in the description. Include no more than ${max} issues.`;
}

function step(config: Partial<StepConfig> & Pick<StepConfig, "id" | "label" | "prompt" | "phase">): StepConfig {
  return {
    enabled: true,
    tools: [],
    agents: [],
    after: [],
    context: { include: [] },
    ...config,
  };
}

const revisionResponse: PipelineConfig = {
  use_orientation: true,
  orientation_prompt: "",
  extraction: {
    method: "",
    input_mode: "document",
    extra_inputs: [
      { key: "prior_report", label: "Prior referee or review report", mode: "document", required: true },
      { key: "response_letter", label: "Author response letter", mode: "document", required: true },
    ],
  },
  variables: [
    { key: "standard", label: "Verification standard", kind: "choice", default: "strict", choices: ["strict", "balanced"] },
  ],
  context_cache: { enabled: true },
  parallel_context_template: genericParallelContext,
  merge: { enabled: false, prompt: "", agents: [] },
  outputs: { primary_step: "revision_verdict", findings_step: "revision_verdict" },
  steps: [
    step({
      id: "verify_claims",
      label: "Verify Claimed Revisions",
      phase: "parallel",
      prompt: "Compare every material claim in the author response with the revised manuscript. Classify it as verified, partial, unsupported, or not checkable. Use a {var:standard} standard. Cite exact pages or document nodes whenever possible.",
      context: { include: [
        { kind: "primary", parts: ["text", "structure", "visuals"] },
        { kind: "survey" },
        { kind: "named_input", key: "response_letter", parts: ["text"] },
        { kind: "named_input", key: "prior_report", parts: ["text"] },
      ] },
    }),
    step({
      id: "remaining_concerns",
      label: "Check Remaining Concerns",
      phase: "parallel",
      prompt: "Read the prior report and revised manuscript. Identify concerns that remain substantively unresolved, including cases where the response letter overstates the revision. Do not repeat concerns that are clearly resolved. Cite source pages or nodes.",
      context: { include: [
        { kind: "primary", parts: ["text", "structure", "visuals"] },
        { kind: "survey" },
        { kind: "named_input", key: "prior_report", parts: ["text"] },
        { kind: "named_input", key: "response_letter", parts: ["text"] },
      ] },
    }),
    step({
      id: "revision_verdict",
      label: "Revision Issue Ledger",
      phase: "sequential",
      prompt: `Consolidate the two checks into an issue ledger for the revised manuscript. Deduplicate findings and distinguish unsupported response claims from genuinely unresolved substantive concerns. ${issueOutputInstruction()}`,
      context: { include: [
        { kind: "primary", parts: ["text", "structure", "visuals"] },
        { kind: "named_input", key: "prior_report", parts: ["text"] },
        { kind: "named_input", key: "response_letter", parts: ["text"] },
        { kind: "step", step: "verify_claims", parts: ["report"] },
        { kind: "step", step: "remaining_concerns", parts: ["report"] },
      ] },
      output_schema: issuesSchema,
    }),
  ],
};

const literaturePositioning: PipelineConfig = {
  use_orientation: true,
  orientation_prompt: "",
  extraction: { method: "", input_mode: "document", extra_inputs: [] },
  variables: [
    { key: "cutoff", label: "Literature cutoff or emphasis", kind: "text", default: "recent and directly related work" },
  ],
  context_cache: { enabled: true },
  parallel_context_template: genericParallelContext,
  merge: { enabled: false, prompt: "", agents: [] },
  outputs: { primary_step: "positioning_ledger", findings_step: "positioning_ledger" },
  steps: [
    step({
      id: "claimed_contribution",
      label: "Map Claimed Contribution",
      phase: "parallel",
      prompt: "Identify each distinct novelty claim, the comparison set used by the authors, and the paper feature on which the claim rests. Separate explicit claims from implied positioning. Cite the manuscript.",
      context: { include: [
        { kind: "primary", parts: ["text", "structure"] },
        { kind: "survey" },
      ] },
    }),
    step({
      id: "literature_search",
      label: "Search Related Literature",
      phase: "parallel",
      tools: ["WebSearch"],
      prompt: "Search for {var:cutoff} that bears directly on the manuscript's novelty claims. Prefer primary papers and stable bibliographic sources. Record titles, authors, dates, links, and the precise overlap; do not infer overlap from titles alone.",
      context: { include: [
        { kind: "primary", parts: ["text", "structure"] },
        { kind: "survey" },
      ] },
    }),
    step({
      id: "positioning_ledger",
      label: "Positioning Issue Ledger",
      phase: "sequential",
      prompt: `Assess which contribution claims are well supported, need narrowing, or omit close work. Do not treat a search result as dispositive without a concrete mechanism or result overlap. ${issueOutputInstruction(15)}`,
      context: { include: [
        { kind: "primary", parts: ["text", "structure"] },
        { kind: "step", step: "claimed_contribution", parts: ["report"] },
        { kind: "step", step: "literature_search", parts: ["report"] },
      ] },
      output_schema: issuesSchema,
    }),
  ],
};

const thesisReview: PipelineConfig = {
  use_orientation: true,
  orientation_prompt: "",
  extraction: { method: "", input_mode: "folder", extra_inputs: [] },
  variables: [],
  context_cache: { enabled: false },
  parallel_context_template: genericParallelContext,
  merge: { enabled: false, prompt: "", agents: [] },
  outputs: { primary_step: "cross_chapter", findings_step: "cross_chapter" },
  steps: [
    step({
      id: "chapter_review",
      label: "Review Each Chapter",
      phase: "parallel",
      prompt: "Review {item} as one chapter of a larger thesis. Identify its contribution, logical gaps, unclear dependencies on other chapters, notation problems, and missing transitions. Preserve the file path in every finding.",
      context: { include: [
        { kind: "primary", parts: ["text", "source"] },
        { kind: "survey" },
      ] },
      for_each: { glob: "**/*.tex", max: 20 },
    }),
    step({
      id: "cross_chapter",
      label: "Cross-Chapter Issue Ledger",
      phase: "sequential",
      prompt: `Consolidate the chapter reviews, emphasizing duplicated material, inconsistent notation or assumptions, missing connective arguments, and conflicts across chapters. ${issueOutputInstruction(25)}`,
      context: { include: [
        { kind: "primary", parts: ["text", "source"] },
        { kind: "step", step: "chapter_review", parts: ["report"] },
      ] },
      output_schema: issuesSchema,
    }),
  ],
};

const rubricReview: PipelineConfig = {
  use_orientation: true,
  orientation_prompt: "",
  extraction: {
    method: "",
    input_mode: "document",
    extra_inputs: [{ key: "rubric", label: "Rubric", mode: "document", required: true }],
  },
  variables: [
    { key: "assignment", label: "Assignment or evaluation context", kind: "text", default: "written submission" },
  ],
  context_cache: { enabled: true },
  parallel_context_template: genericParallelContext,
  merge: { enabled: false, prompt: "", agents: [] },
  outputs: { primary_step: "rubric_ledger", findings_step: "rubric_ledger" },
  steps: [
    step({
      id: "rubric_assessment",
      label: "Apply Rubric",
      phase: "parallel",
      prompt: "Apply the supplied rubric criterion by criterion to this {var:assignment}. Quote the rubric standard, cite manuscript evidence, and distinguish missing evidence from weak evidence. Do not invent weights or requirements.",
      context: { include: [
        { kind: "primary", parts: ["text", "structure", "visuals"] },
        { kind: "survey" },
        { kind: "named_input", key: "rubric", parts: ["text"] },
      ] },
    }),
    step({
      id: "rubric_ledger",
      label: "Rubric Issue Ledger",
      phase: "sequential",
      prompt: `Convert the assessment into a concise, non-duplicative list of actionable deficiencies. Retain the applicable rubric criterion and source evidence for each item. ${issueOutputInstruction()}`,
      context: { include: [
        { kind: "primary", parts: ["text", "structure", "visuals"] },
        { kind: "named_input", key: "rubric", parts: ["text"] },
        { kind: "step", step: "rubric_assessment", parts: ["report"] },
      ] },
      output_schema: issuesSchema,
    }),
  ],
};

export const WORKFLOW_GALLERY: WorkflowGalleryItem[] = [
  {
    id: "revision-response-check",
    name: "Revision Response Check",
    category: "Papers",
    description: "Verifies an author response against the revised manuscript and carries unresolved concerns into an evidence-linked issue ledger.",
    inputs: "Revised manuscript, prior report, response letter",
    outcome: "Verified, partial, unsupported, and unresolved revision claims",
    config: revisionResponse,
  },
  {
    id: "literature-positioning-scan",
    name: "Literature Positioning Scan",
    category: "Research",
    description: "Maps contribution claims and checks them against directly related work using a separate literature-search pass.",
    inputs: "Paper or extended draft",
    outcome: "Evidence-linked positioning and novelty concerns",
    config: literaturePositioning,
  },
  {
    id: "thesis-chapter-review",
    name: "Thesis Chapter Review",
    category: "Long documents",
    description: "Fans out over chapter source files, then checks consistency, notation, and connective arguments across the thesis.",
    inputs: "Thesis source folder",
    outcome: "Chapter-level and cross-chapter issue ledger",
    config: thesisReview,
  },
  {
    id: "rubric-review",
    name: "Rubric-Based Review",
    category: "Teaching",
    description: "Applies an explicit rubric without inventing criteria and retains evidence for each actionable deficiency.",
    inputs: "Submission and rubric",
    outcome: "Criterion-linked, evidence-backed issues",
    config: rubricReview,
  },
];

export function cloneGalleryConfig(item: WorkflowGalleryItem): PipelineConfig {
  return JSON.parse(JSON.stringify(item.config)) as PipelineConfig;
}
