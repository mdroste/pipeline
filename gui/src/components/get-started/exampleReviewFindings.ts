// The comments in the Example Review. Each is written once, in the four parts a
// reviewer reports, so the reviewer reports, the findings list, and the final
// report cannot disagree. No model produced them. Comment 10 misquotes the paper
// on purpose: its premise is wrong, and the quote check is what shows it.

import type { EvidenceVerificationStatus } from "../../lib/issues";

export interface ExampleEvidence {
  page: number;
  verification_status: EvidenceVerificationStatus;
  quote?: string;
  description?: string;
}

export interface ExampleFinding {
  id: string;
  title: string;
  category: string;
  priority: "high" | "medium" | "low";
  reviewers: string[];
  inThePaper: string;
  problem: string;
  consequence: string;
  whatWouldHelp: string;
  evidence: ExampleEvidence[];
}

const verified = (page: number, quote: string): ExampleEvidence => ({
  page,
  quote,
  verification_status: "verified",
});

const CORRECTNESS = "Correctness and Internal Consistency";
const METHODS = "Methodological and Evidentiary Concerns";
const CONTRIBUTION = "Contribution and Scope";
const EXPOSITION = "Exposition and Organization";

/** In report order, which is the order of `EXAMPLE_SECTIONS`. */
export const EXAMPLE_FINDINGS: ExampleFinding[] = [
  {
    id: "return-arithmetic",
    title: "The $2.30 return does not follow from the paper's own numbers",
    category: CORRECTNESS,
    priority: "high",
    reviewers: ["auto_consistency"],
    inThePaper:
      "Section 5 multiplies the 4.1 point effect by 62,000 women and mean earnings of $31,000 and reports $42.3 million a year.",
    problem:
      "That product is $78.8 million, so the ratio to the $18.4 million subsidy is 4.3 where the paper says 2.30. Neither figure is a return. Both count gross earnings and leave out the $310 million construction cost, child care, taxes, and the value of time spent at home.",
    consequence: "The policy claim in the abstract depends on this number.",
    whatWouldHelp:
      "Correct the arithmetic, set out a full cost-benefit calculation, and keep the figure out of the abstract until that is done.",
    evidence: [
      verified(6, "gives $42.3 million in additional annual earnings"),
      verified(1, "each dollar of annual operating subsidy returns $2.30"),
    ],
  },
  {
    id: "sample-size-mismatch",
    title: "Section 2 and Table 2 report different sample sizes",
    category: CORRECTNESS,
    priority: "medium",
    reviewers: ["auto_consistency"],
    inThePaper:
      "Section 2 reports 48,212 woman-year observations. Column 1 of Table 2 reports 46,812.",
    problem:
      "Columns 2 and 3 add up to the smaller number, so 1,400 observations leave the sample at a step the paper does not describe. Section 4 also calls the estimate a 7 percent rise over the comparison mean, but 0.041 over 0.702 is 5.8 percent.",
    consequence:
      "A reader cannot tell which sample produced the headline estimate.",
    whatWouldHelp:
      "List the sample restrictions in order with counts, and correct the percentage.",
    evidence: [
      verified(3, "The final sample has 48,212 woman-year observations"),
      {
        page: 5,
        description: "Table 2, Observations row",
        verification_status: "not_checkable",
      },
      verified(5, "a 7 percent rise in employment"),
    ],
  },
  {
    id: "selected-corridors",
    title: "Stations followed projected job growth, and the leads trend upward",
    category: METHODS,
    priority: "high",
    reviewers: ["causal_identification", "subject_economics_labor"],
    inThePaper:
      "The 2015 corridor study ranked streets partly on projected job growth, and Section 4 says the event-study leads support parallel trends.",
    problem:
      "Station tracts were picked in part for the trend the design has to rule out. The leads also climb steadily, from −0.021 to −0.014 to −0.006, about 0.7 points a year. Each is insignificant, but with standard errors of 0.012 to 0.016 the test could not detect a drift of that size.",
    consequence:
      "If the drift continued for three years after opening, it would account for about half of the 4.1 point estimate.",
    whatWouldHelp:
      "Report a joint test of the leads and a sensitivity analysis in the style of Rambachan and Roth (2023). Use the two corridors that were studied and not built as a placebo.",
    evidence: [
      verified(
        3,
        "ranked candidate streets on existing ridership, projected job growth, and available right of way",
      ),
      verified(5, "which supports the parallel-trends assumption"),
    ],
  },
  {
    id: "clustering-level",
    title: "Standard errors are clustered below the level of treatment",
    category: METHODS,
    priority: "high",
    reviewers: ["statistical_validity"],
    inThePaper:
      "Treatment is assigned by tract, and Section 3 clusters standard errors by household.",
    problem:
      "Women in the same tract share the station opening and the local labor market, so their outcomes are correlated across households.",
    consequence:
      "The reported standard error of 0.012 is likely too small, and the stars in Table 2 may not survive.",
    whatWouldHelp:
      "Cluster by tract, and report wild cluster bootstrap p-values because stations in a phase opened together.",
    evidence: [
      {
        page: 4,
        quote: "standard errors are clustered by household",
        description: "Found after ignoring capitalization",
        verification_status: "verified_with_normalization",
      },
    ],
  },
  {
    id: "staggered-timing",
    title: "Earlier phases are poor comparisons for later ones",
    category: METHODS,
    priority: "high",
    reviewers: ["causal_identification", "statistical_validity"],
    inThePaper:
      "Section 3 says tracts that opened earlier serve as comparisons for later ones, and Section 4 reports effects that grow after opening.",
    problem:
      "When effects grow with time since opening, the trend in already-treated tracts is subtracted from the estimate for later phases. The 2021 phase has a second problem: the survey skipped 2020, so its post-period is the pandemic recovery.",
    consequence:
      "The coefficient is a weighted average of these comparisons, with weights the paper does not report, and it need not equal the average effect of a station.",
    whatWouldHelp:
      "Show a Goodman-Bacon decomposition, re-estimate with Callaway and Sant'Anna (2021), and report each phase separately.",
    evidence: [
      verified(
        4,
        "tracts that opened earlier serve as comparisons for tracts that opened later",
      ),
      verified(5, "reaching 0.058 (0.019) three years out"),
      verified(3, "The survey was not fielded in 2020"),
    ],
  },
  {
    id: "dropped-movers",
    title:
      "Dropping households that moved conditions on a response to the line",
    category: METHODS,
    priority: "medium",
    reviewers: ["causal_identification"],
    inThePaper:
      "Section 2 drops households that moved during the sample period, 11 percent of observations.",
    problem:
      "The line may itself cause moves. Women may move near a station to take a job, and higher rents may push others out.",
    consequence:
      "If the women who leave station tracts are less attached to work, the estimate is biased upward.",
    whatWouldHelp:
      "Keep movers, assign them to their baseline tract, and report moving rates by tract type.",
    evidence: [
      verified(3, "We drop households that moved during the sample period"),
    ],
  },
  {
    id: "commuting-magnitude",
    title: "Commuting is too small a channel to produce a 4.1 point gain",
    category: METHODS,
    priority: "medium",
    reviewers: ["subject_economics_labor", "auto_consistency"],
    inThePaper:
      "Section 4 reports that 6 percent of employed women in station tracts mainly commute on the Blue Line, and that commutes fell by 1.2 minutes (s.e. 1.9).",
    problem:
      "With about 74 percent of women employed, riders are about 4.5 percent of women in these tracts. A 4.1 point gain through commuting would need more than nine in ten riders to be women who would not otherwise work.",
    consequence:
      "The account of shorter commutes in the abstract does not fit these numbers. The gain may come from something else, such as new jobs near stations, or the estimate may be too large.",
    whatWouldHelp:
      "Report where the added jobs are and how far these women travel to them.",
    evidence: [
      verified(
        5,
        "6 percent of employed women in station tracts named the Blue Line",
      ),
      verified(5, "fell by 1.2 minutes (s.e. 1.9)"),
      verified(
        1,
        "shorter commutes make work easier to combine with child care",
      ),
    ],
  },
  {
    id: "income-control",
    title: "Current household income is a bad control",
    category: METHODS,
    priority: "medium",
    reviewers: ["causal_identification"],
    inThePaper: "The controls in Section 3 include current household income.",
    problem:
      "Household income rises when a woman starts work, so it is an outcome of the treatment.",
    consequence:
      "Controlling for it removes part of the effect the paper wants to measure.",
    whatWouldHelp:
      "Use income from before the first station opened, or drop the control, and report how the estimate changes.",
    evidence: [verified(4, "number of children, and current household income")],
  },
  {
    id: "first-paper-claim",
    title: "The paper's own references contradict the “first paper” claim",
    category: CONTRIBUTION,
    priority: "low",
    reviewers: ["auto_contribution"],
    inThePaper:
      "The introduction calls this the first paper on bus rapid transit and women's employment.",
    problem:
      "The references include Marsh, Tovar, and Dean (2022), “Bus Rapid Transit and the Labor Supply of Women.”",
    consequence:
      "The claim will not survive a referee reading the reference list.",
    whatWouldHelp:
      "Drop the claim and say what this paper adds to Marsh, Tovar, and Dean.",
    evidence: [
      {
        page: 1,
        quote:
          "This is the first paper to estimate the effect of bus rapid transit on women's employment.",
        description: "Found on page 2, not page 1",
        verification_status: "partially_verified",
      },
      verified(7, "Bus Rapid Transit and the Labor Supply of Women."),
    ],
  },
  {
    id: "model-predictions",
    title: "Say which prediction of Lee and Ortiz (2017) the paper tests",
    category: EXPOSITION,
    priority: "low",
    reviewers: ["auto_exposition"],
    inThePaper:
      "The conclusion says the results confirm the commuting-cost model of Lee and Ortiz (2017).",
    problem: "The paper never says which prediction of the model it tests.",
    consequence:
      "Readers cannot tell what the model adds to the reduced-form estimate.",
    whatWouldHelp:
      "State one testable prediction, such as larger effects for women with longer commutes before the line opened, and test it.",
    evidence: [
      {
        page: 6,
        quote: "Our results confirm the commuting-cost model of Lee and Ortiz",
        verification_status: "not_found",
      },
    ],
  },
];

export const EXAMPLE_SECTIONS = [
  CORRECTNESS,
  METHODS,
  CONTRIBUTION,
  EXPOSITION,
];

export function findingBody(finding: ExampleFinding): string {
  return [
    finding.inThePaper,
    finding.problem,
    finding.consequence,
    finding.whatWouldHelp,
  ].join(" ");
}
