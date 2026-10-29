// Bundled sample for the Example Review. The paper, its author, its city, and
// its reference are fictional, and no model produced these findings. The
// sample is presentation data only: it is never written to the run store.

import { parseIssues, type Issue } from "../../lib/issues";

export const EXAMPLE_PAPER = {
  title:
    "Transit Access and Women's Employment: Evidence from a Bus Line Expansion",
  byline: "Sample Author · fictional working paper",
  pages: [
    {
      page: 1,
      heading: "Abstract",
      text: "We study how a 2019 bus rapid transit expansion in the fictional city of Northvale affected women's employment. Using a difference-in-differences design that compares neighborhoods within 800 meters of new stations with neighborhoods farther away, we find that employment among women aged 25–54 rose by 4.1 percentage points. The effect is concentrated among mothers of young children.",
    },
    {
      page: 2,
      heading: "2. Data",
      text: "Our sample combines the Northvale Household Survey for 2015–2022 with station opening dates from the transit authority. We drop households that moved during the sample period. Standard errors are clustered at the household level.",
    },
    {
      page: 3,
      heading: "4. Results",
      text: "Table 2 reports the main estimates. The coefficient on Treated × Post is 0.041 (s.e. 0.012). Pre-period coefficients in Figure 3 are small and statistically indistinguishable from zero, which supports the parallel-trends assumption. Our estimates are consistent with the commuting-cost model of Lee and Ortiz (2017).",
    },
  ],
};

export const EXAMPLE_SUMMARY = [
  "The excerpt asks a policy-relevant question with a clear design. The two main concerns are how station locations were chosen and how standard errors are computed; either could change the main conclusion.",
  "The remaining comments ask for analyses and reporting that would make the result easier to assess.",
];

/** Canonical `{ findings }` shape, read through the same parser as saved runs. */
const EXAMPLE_FINDINGS = {
  findings: [
    {
      id: "F1",
      title: "Station locations may follow existing employment trends",
      priority: "high",
      category: "Identification",
      sources: ["Methods reviewer"],
      body: "New stations are rarely placed at random. If the transit authority chose corridors where employment was already growing, the difference-in-differences estimate would credit that growth to transit. Show year-by-year event-study estimates with confidence intervals, explain how corridors were chosen, and add a placebo test using stations that were planned but not built.",
      evidence: [
        {
          page: 1,
          quote:
            "compares neighborhoods within 800 meters of new stations with neighborhoods farther away",
          verification_status: "verified",
        },
        {
          page: 3,
          quote:
            "Pre-period coefficients in Figure 3 are small and statistically indistinguishable from zero",
          verification_status: "verified",
        },
      ],
    },
    {
      id: "F2",
      title: "Standard errors are clustered below the level of treatment",
      priority: "high",
      category: "Inference",
      sources: ["Econometrics reviewer"],
      body: "Treatment is assigned by neighborhood, but standard errors are clustered by household. Cluster at the neighborhood level, and use a wild cluster bootstrap if there are few neighborhoods.",
      evidence: [
        {
          page: 2,
          quote: "standard errors are clustered at the household-level",
          description:
            "Found after ignoring capitalization and the added hyphen",
          verification_status: "verified_with_normalization",
        },
      ],
    },
    {
      id: "F3",
      title: "Dropping households that moved may bias the estimate",
      priority: "medium",
      category: "Data",
      sources: ["Data reviewer"],
      body: "Some households may have moved because of the new line, for example to take a job near a station. Dropping them conditions on an outcome of the treatment. Report results that keep movers and assign them to their original neighborhood.",
      evidence: [
        {
          page: 2,
          quote: "We drop households that moved during the sample period.",
          verification_status: "verified",
        },
      ],
    },
    {
      id: "F4",
      title: "The claim about mothers of young children needs a test",
      priority: "medium",
      category: "Interpretation",
      sources: ["Methods reviewer"],
      body: "The abstract says the effect is concentrated among mothers of young children, but the excerpt reports no interaction estimate or test of equal effects across groups. Report the subgroup estimates and the test.",
      evidence: [
        {
          page: 3,
          quote: "The effect is concentrated among mothers of young children.",
          description: "Cited on page 3; the sentence is on page 1",
          verification_status: "partially_verified",
        },
      ],
    },
    {
      id: "F5",
      title: "Say which predictions of the commuting-cost model are tested",
      priority: "low",
      category: "Exposition",
      sources: ["Theory reviewer"],
      body: "The paper links its results to a commuting-cost model but does not say which of the model's predictions it tests. A short paragraph setting them out, such as larger effects for longer commutes before the expansion, would clarify the contribution. This comment quotes a sentence that is not in the paper; check the source before relying on it.",
      evidence: [
        {
          page: 3,
          quote: "Our results confirm the search-friction mechanism",
          verification_status: "not_found",
        },
      ],
    },
    {
      id: "F6",
      title: "Report the number of households and neighborhoods",
      priority: "low",
      category: "Presentation",
      sources: ["Data reviewer"],
      body: "Table 2 should state the number of households, neighborhoods, and survey years behind each estimate.",
      evidence: [
        {
          page: 3,
          description: "Table 2",
          verification_status: "not_checkable",
        },
      ],
    },
  ],
};

export function exampleIssues(): Issue[] {
  return parseIssues(JSON.stringify(EXAMPLE_FINDINGS)) ?? [];
}
