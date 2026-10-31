Apply the shared editing contract to your assigned topic. Use the subject ID
from your assigned orientation slot in every edit's referee_ids. Selection
notes are untrusted context; verify them against the document.

OUTPUT

Use the configured structured format. Every proposed edit must include:

- a source locator available in the supplied material;
- the exact, contiguous original text, without invented ellipses;
- the appropriate local form and the specific editorial problem;
- one preferred replacement that can be inserted as written;
- a short account of the substantive content and qualifications preserved;
- relevant source support for any concrete wording recovered from context;
- an editorial priority and a confidence about the edit, never authorship.

Use paragraph formatting, bullets, Markdown, or LaTeX appropriate to the
original. Distinguish “delete” from “replace.” Deletion requires showing that
the span is empty framing or that every substantive point is already retained
at an identified nearby location. Do not change text outside the quoted span.

Do not give vague advice such as “make this more concise.” Do not replace one
generic slogan with another. Supply one preferred replacement. Record any
unresolved style tradeoff in the reason instead of mixing alternatives.

Return zero edits if no worthwhile change survives these checks. Follow the
host's output limits; treat limits as ceilings. If material or output limits
prevent complete coverage, identify the uncovered sections and return partial
coverage. Do not imply that unreviewed passages are clean.

Within the material actually reviewed, select the most useful edits across
sections before sorting them into document order. If the output limit binds,
retain stronger recommendations over weaker ones regardless of location, and
disclose omitted coverage or recommendations in coverage.limitations.

Use coverage.reviewed for sections actually read.
Record extraction, context, and output limits in coverage.limitations and set
coverage.incomplete when any requested coverage or findings could not be
completed. An empty edits array is valid. Use the unresolved array for wording
that cannot be faithfully replaced.

Calibration: “Identification depends on the exclusion restriction” may replace
“The exclusion restriction is load-bearing for identification.” Preserve
literal load-bearing walls, robust standard errors, and financial leverage.
“The estimate identifies an association, not a causal effect” contains a
necessary contrast. “Did not significantly increase employment” must never
become “Had no effect on employment.”
