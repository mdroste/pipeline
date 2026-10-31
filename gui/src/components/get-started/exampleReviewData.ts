// Assembles the Example Review from the invented paper and comments into the
// same shapes a saved run supplies, so the real report screen can show it. It
// is presentation data only: no model ran, and it never reaches the run store.
// The figures below are illustrative, and the quality totals are counted with
// the rules in output/quality.rs so the sample reads the way a real run would.

import type {
  Finding,
  PipelineReport,
  RunSummary,
  StepOutput,
} from "../../lib/types";
import {
  EXAMPLE_EXTRACTED_TEXT,
  EXAMPLE_PAPER,
  EXAMPLE_PAPER_PAGES,
} from "./exampleReviewPaper";
import {
  EXAMPLE_FINDINGS,
  EXAMPLE_SECTIONS,
  findingBody,
} from "./exampleReviewFindings";

const PROVIDER = "Example provider";
const MODEL = "Example model";
const RUN_ID = "example";
const REVIEWED = "2026-03-16";

/** Keeps a dollar amount from opening a math span in the report renderer. */
const escapeDollars = (text: string) => text.replace(/\$/g, "\\$");

const REVIEWERS = [
  ["auto_contribution", "Contribution & Literature", 41, 118_400, 1_920],
  ["auto_consistency", "Claims & Consistency", 58, 121_300, 2_640],
  ["auto_exposition", "Exposition & Architecture", 26, 96_700, 1_180],
  [
    "subject_economics_labor",
    "Economics — Labor Economics",
    63,
    124_900,
    2_410,
  ],
  [
    "causal_identification",
    "Method — Causal Identification",
    74,
    131_200,
    3_370,
  ],
  ["statistical_validity", "Method — Statistical Validity", 52, 119_800, 2_050],
] as const;

function step(
  id: string,
  label: string,
  phase: string,
  durationSecs: number,
  inputTokens: number,
  outputTokens: number,
  rawText: string,
): StepOutput {
  return {
    step_id: id,
    step_label: label,
    phase,
    agent: PROVIDER,
    provider: PROVIDER,
    model: MODEL,
    raw_text: rawText,
    duration_secs: durationSecs,
    input_tokens: inputTokens,
    output_tokens: outputTokens,
    cached_input_tokens: Math.round(inputTokens * 0.7),
    cache_write_input_tokens: 0,
  };
}

function reviewerOutput(id: string): string {
  return JSON.stringify({
    findings: EXAMPLE_FINDINGS.filter((finding) =>
      finding.reviewers.includes(id),
    ).map((finding) => ({
      title: escapeDollars(finding.title),
      in_the_paper: escapeDollars(finding.inThePaper),
      problem: escapeDollars(finding.problem),
      consequence: escapeDollars(finding.consequence),
      what_would_help: escapeDollars(finding.whatWouldHelp),
      evidence: finding.evidence,
    })),
  });
}

const COMMENTS = EXAMPLE_SECTIONS.map((section) => {
  const comments = EXAMPLE_FINDINGS.filter(
    (finding) => finding.category === section,
  ).map(
    (finding) =>
      `**#${EXAMPLE_FINDINGS.indexOf(finding) + 1}. ${escapeDollars(finding.title)}**\n\n${escapeDollars(findingBody(finding))}`,
  );
  return `## ${section}\n\n${comments.join("\n\n")}`;
}).join("\n\n");

const STEP_OUTPUTS = [
  ...REVIEWERS.map(([id, label, seconds, input, output]) =>
    step(id, label, "parallel", seconds, input, output, reviewerOutput(id)),
  ),
  step(
    "auto_synthesis",
    "Consolidate Feedback",
    "sequential",
    66,
    29_800,
    4_930,
    COMMENTS,
  ),
  step(
    "auto_validate",
    "Validate Feedback",
    "sequential",
    47,
    142_600,
    3_600,
    COMMENTS,
  ),
];

const evidenceItems = EXAMPLE_FINDINGS.flatMap((finding) => finding.evidence);
const countEvidence = (...statuses: string[]) =>
  evidenceItems.filter((item) => statuses.includes(item.verification_status))
    .length;
const verifiedEvidence = countEvidence(
  "verified",
  "verified_with_normalization",
);
const partialEvidence = countEvidence("partially_verified");
const unverifiedEvidence = countEvidence("not_found");
const notCheckableEvidence = countEvidence("not_checkable");

const reviewerIds = STEP_OUTPUTS.map((output) => output.step_id);

const QUALITY: PipelineReport["quality"] = {
  schema_version: 1,
  status: "degraded",
  extraction_method: "paddleocr-vl-full",
  input_bytes: EXAMPLE_EXTRACTED_TEXT.length,
  orientation_included_bytes: EXAMPLE_EXTRACTED_TEXT.length,
  orientation_omitted_bytes: 0,
  recovered_pages: EXAMPLE_PAPER_PAGES.length,
  recovered_blocks: 38,
  recovered_assets: 2,
  completed_steps: reviewerIds,
  failed_steps: [],
  skipped_steps: [],
  schema_retry_events: 0,
  verified_evidence: verifiedEvidence,
  partially_verified_evidence: partialEvidence,
  unverified_evidence: unverifiedEvidence,
  not_checkable_evidence: notCheckableEvidence,
  manual_review_items: 0,
  limitations: [
    `Evidence requiring caution: ${partialEvidence} partially verified and ${unverifiedEvidence} unverified item(s).`,
    `${notCheckableEvidence} evidence item(s) could not be checked automatically.`,
  ],
  reproducibility_hash: "",
};

const durationSecs = STEP_OUTPUTS.reduce(
  (total, output) => total + (output.duration_secs ?? 0),
  0,
);

const FINDINGS: Finding[] = EXAMPLE_FINDINGS.map((finding, index) => ({
  rank: index + 1,
  id: finding.id,
  sources: finding.reviewers,
  reviewer_ids: finding.reviewers,
  title: finding.title,
  category: finding.category,
  priority: finding.priority,
  body: findingBody(finding),
  evidence: finding.evidence,
}));

export const EXAMPLE_REPORT: PipelineReport = {
  orientation: {
    metadata: {
      title: EXAMPLE_PAPER.title,
      authors: EXAMPLE_PAPER.authors,
      date: EXAMPLE_PAPER.date,
      paper_type: "empirical",
      page_count: EXAMPLE_PAPER_PAGES.length,
      has_appendix: false,
      has_online_appendix: false,
    },
    sections: [
      { number: "", title: "Abstract", page_start: 1, page_end: 1 },
      { number: "1", title: "Introduction", page_start: 2, page_end: 2 },
      { number: "2", title: "Background and data", page_start: 3, page_end: 3 },
      { number: "3", title: "Empirical strategy", page_start: 4, page_end: 4 },
      { number: "4", title: "Results", page_start: 5, page_end: 5 },
      { number: "5", title: "Costs and benefits", page_start: 6, page_end: 6 },
      { number: "6", title: "Conclusion", page_start: 6, page_end: 6 },
    ],
    formal_results: [],
    tables_figures: [
      {
        kind: "table",
        number: "2",
        page: 5,
        caption_summary: "Effect of a new station on employment.",
        what_it_shows:
          "Main estimates for all women, mothers, other women, and men.",
      },
      {
        kind: "figure",
        number: "3",
        page: 5,
        caption_summary: "Event-study estimates.",
        what_it_shows: "Coefficients by year relative to station opening.",
      },
    ],
    notation: [],
    stated_contribution:
      "Estimates the effect of a bus rapid transit line on women's employment using its phased opening.",
    key_references: ["Lee and Ortiz (2017)", "Marsh, Tovar, and Dean (2022)"],
    extraction_quality_notes: [],
    review_plan: {
      primary_domain: "Economics",
      subject: "Labor economics",
      paper_forms: ["causal_empirical"],
      methods: ["difference_in_differences"],
      subject_specialist_ids: ["subject_economics_labor"],
      method_specialist_ids: ["causal_identification", "statistical_validity"],
      genre: "research_article",
      selection_notes: [
        {
          id: "subject_economics_labor",
          reason: "The main outcome is women's employment.",
        },
        {
          id: "causal_identification",
          reason:
            "The paper makes causal claims from a staggered difference-in-differences design.",
        },
        {
          id: "statistical_validity",
          reason: "The paper relies on clustered inference and an event study.",
        },
      ],
      routing_uncertainty: [],
    },
  },
  step_outputs: STEP_OUTPUTS,
  failed_steps: [],
  products: {
    schema_version: 1,
    primary_step_id: "auto_validate",
    findings: {
      schema_version: 1,
      source_step_id: "auto_validate",
      source_step_label: "Validate Feedback",
      findings: FINDINGS,
    },
  },
  quality: QUALITY,
  report_date: REVIEWED,
  paper_hash: RUN_ID,
};

export const EXAMPLE_SUMMARY: RunSummary = {
  run_id: "",
  created: `${REVIEWED}T14:32:00`,
  input_name: "port-harlan-brt.pdf",
  input_path: "",
  input_mode: "document",
  profile_id: "auto_paper_review",
  profile_name: "Automatic Paper Review",
  provider: PROVIDER,
  status: "degraded",
  duration_secs: durationSecs,
  input_tokens: 0,
  output_tokens: 0,
  cached_input_tokens: 0,
  cache_write_input_tokens: 0,
  step_count: STEP_OUTPUTS.length,
  artifact_count: 0,
  failed_steps: [],
  resumable: false,
  title: EXAMPLE_PAPER.title,
  tags: [],
};

const formatTokens = (value: number) => `${(value / 1000).toFixed(1)}k`;

function runSummaryRow(output: StepOutput): string {
  const input = formatTokens(output.input_tokens ?? 0);
  const out = formatTokens(output.output_tokens ?? 0);
  return `| ${output.step_label} | ${PROVIDER} · ${MODEL} | ${output.duration_secs}s | ${input} in · ${out} out |`;
}

const minutes = `${Math.floor(durationSecs / 60)}m ${durationSecs % 60}s`;

export const EXAMPLE_REPORT_MARKDOWN = `# Referee Report

<!-- PIPELINE RUN DETAILS START -->
**Review subject**: ${EXAMPLE_PAPER.title}
**Authors**: ${EXAMPLE_PAPER.authors.join(", ")}
**Type**: empirical · **Reviewed**: ${REVIEWED}
**LLM**: ${PROVIDER} · **Model**: ${MODEL} · **Generated in**: ${minutes}
*Report generated by Pipeline*

<!-- PIPELINE RUN DETAILS END -->

> **Status:** Completed. ${partialEvidence + unverifiedEvidence} quotes need checking. Coverage and limitations are reported below.

---

${COMMENTS}

---

## Quality and limitations

**Final status:** Completed
**Extraction coverage:** ${EXAMPLE_PAPER_PAGES.length} pages recovered.
**Reviewer coverage:** ${STEP_OUTPUTS.length} completed, 0 failed or blocked, 0 skipped.
**Evidence:** ${verifiedEvidence} verified, ${partialEvidence} partially verified, ${unverifiedEvidence} unverified, ${notCheckableEvidence} not checkable.

### Recorded limitations

${(QUALITY?.limitations ?? []).map((item) => `- ${item}`).join("\n")}

<!-- PIPELINE RUN DETAILS START -->
---

## Run summary

| Step | Provider · Model | Time | Tokens |
|------|------------------|------|--------|
${STEP_OUTPUTS.map(runSummaryRow).join("\n")}

<!-- PIPELINE RUN DETAILS END -->
`;
