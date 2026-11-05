// The invented paper behind the Example Review. Its authors, city, survey, and
// references are fictional. Its mistakes are deliberate: exampleReviewFindings.ts
// cites them, and the tests check every quotation against these pages. Dollar
// signs are escaped because the report renderer reads an unescaped pair as math.

export const EXAMPLE_PAPER = {
  title: "Bus Rapid Transit and Women's Employment: Evidence from Port Harlan",
  authors: ["Ingrid Halvorsen", "Mateo Quintero"],
  date: "March 2026",
};

export const EXAMPLE_PAPER_PAGES = [
  `**${EXAMPLE_PAPER.title}**

${EXAMPLE_PAPER.authors.join(" and ")}. ${EXAMPLE_PAPER.date}. Preliminary draft.

**Abstract.** Port Harlan opened the Blue Line, a 14-mile bus rapid transit corridor, in three phases between 2018 and 2021. We compare census tracts within half a mile of a new station with tracts farther away, before and after each station opened, using eight waves of the Port Harlan Household Survey. Employment among women aged 25–54 rose by 4.1 percentage points in station tracts, with no change for men. The gain is concentrated among mothers of children under six, for whom shorter commutes make work easier to combine with child care. Our estimates imply that each dollar of annual operating subsidy returns \\$2.30 in additional earnings for women.`,

  `### 1. Introduction

Women in low-income neighborhoods often work close to home. Long commutes are hard to fit around school hours and child care, so poor transit can keep mothers out of work even when there are jobs across town. Lee and Ortiz (2017) write down a commuting-cost model with this feature. Cities spend heavily on buses, and we know little about whether new lines change who works.

This is the first paper to estimate the effect of bus rapid transit on women's employment. We use the phased opening of the Blue Line in Port Harlan. Stations opened on three dates, so we can compare neighborhoods that received a station early with those that received one later or not at all.

Employment among women aged 25–54 rose by 4.1 percentage points in tracts with a new station. We find no effect for men. The effect for mothers of children under six is about twice the average. Section 5 shows that the earnings gain exceeds the line's operating subsidy.`,

  `### 2. Background and data

The Port Harlan Transit Authority chose the route in 2015. A corridor study ranked candidate streets on existing ridership, projected job growth, and available right of way. Two corridors that scored lower were studied and not built. The Blue Line opened in three phases: 11 stations in September 2018, 9 in June 2019, and 8 in August 2021.

We define a station tract as a census tract whose centroid lies within half a mile of a station. Of the 212 tracts in the metro area, 61 are station tracts. Comparison tracts are all other tracts.

Employment data come from the Port Harlan Household Survey, an annual panel of households run by the county since 2014. The survey was not fielded in 2020, so we have eight waves from 2014 to 2022. We keep women aged 25–54 at the time of the survey. We drop households that moved during the sample period, which removes 11 percent of observations. The final sample has 48,212 woman-year observations on 9,640 women.`,

  `### 3. Empirical strategy

We estimate

$$Y_{ijt} = \\alpha_i + \\gamma_t + \\beta\\,(\\text{Station}_j \\times \\text{Post}_{jt}) + X_{it}'\\delta + \\varepsilon_{ijt}$$

where $Y_{ijt}$ equals one if woman $i$ in tract $j$ is employed in year $t$, $\\alpha_i$ and $\\gamma_t$ are woman and year fixed effects, and $\\text{Post}_{jt}$ equals one from the first survey year after the tract's nearest station opened. Controls $X_{it}$ are age, age squared, number of children, and current household income. Standard errors are clustered by household.

Because stations opened on different dates, tracts that opened earlier serve as comparisons for tracts that opened later. The identifying assumption is that station and comparison tracts would have followed parallel trends without the line.`,

  `### 4. Results

Table 2 reports the main estimates.

**Table 2. Effect of a new station on employment**

| | (1) All women | (2) Mothers, child under 6 | (3) Other women | (4) Men |
|---|---|---|---|---|
| Station × Post | 0.041\\*\\*\\* | 0.087\\*\\*\\* | 0.018 | 0.003 |
| | (0.012) | (0.024) | (0.014) | (0.011) |
| Mean, comparison tracts | 0.702 | 0.581 | 0.744 | 0.861 |
| Observations | 46,812 | 6,118 | 40,694 | 44,730 |

Standard errors in parentheses. \\*\\*\\* p < 0.01.

The coefficient in column 1 implies a 7 percent rise in employment over the comparison mean of 0.702. Column 2 shows that the effect for mothers of young children is about twice as large. The estimate for men is close to zero.

Figure 3 plots event-study estimates relative to the year before opening. The coefficients for four, three, and two years before opening are −0.021 (s.e. 0.016), −0.014 (0.014), and −0.006 (0.012). They are small and statistically indistinguishable from zero, which supports the parallel-trends assumption. Effects grow after opening, reaching 0.058 (0.019) three years out.

Table 3 looks at commuting. One-way commutes for employed women in station tracts fell by 1.2 minutes (s.e. 1.9). In 2022, 6 percent of employed women in station tracts named the Blue Line as their main way to work.`,

  `### 5. Costs and benefits

The Blue Line costs \\$18.4 million a year in operating subsidy, on top of \\$310 million in construction. About 62,000 women aged 25–54 live in station tracts, and mean annual earnings for employed women in the survey are \\$31,000. Multiplying the 4.1 point effect by these two numbers gives \\$42.3 million in additional annual earnings, or \\$2.30 for each dollar of operating subsidy.

### 6. Conclusion

A new bus rapid transit line raised women's employment in Port Harlan, especially among mothers of young children. Our results are consistent with the commuting-cost model of Lee and Ortiz (2017). Cities weighing bus investments should count these labor market gains.`,

  `### References

Lee, J., and R. Ortiz. 2017. "Commuting Costs and the Labor Supply of Mothers." *Journal of Transit Economics* 12 (3): 201–233.

Marsh, T., L. Tovar, and K. Dean. 2022. "Bus Rapid Transit and the Labor Supply of Women." *Review of City Economics* 8 (1): 55–84.

Port Harlan Transit Authority. 2015. *Corridor Selection Study: Final Report.*`,
];

/** One heading per page, which is what lets evidence links jump to a page. */
export const EXAMPLE_EXTRACTED_TEXT = EXAMPLE_PAPER_PAGES.map(
  (text, index) => `## Page ${index + 1}\n\n${text}`,
).join("\n\n");
