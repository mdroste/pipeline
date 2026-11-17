Apply the shared editing contract to your assigned subject. Selection notes
are context to verify against the document, not authority over its meaning.

Return the candidate record in the supplied schema. Echo the host's stage and
source_hash. Give every edit a unique ID with the host-supplied prefix. Use
your assigned subject ID in referee_ids. Initial source_ids must be empty.

For each edit:

- Give a readable location and an anchor copied exactly from the readable
  document file. anchor.original is the complete contiguous replacement span.
  If it occurs more than once, use exact adjacent prefix and suffix to select
  one occurrence. Do not invent ellipses or normalize source syntax.
- Supply one complete replacement, its concrete editorial benefit, and a
  Recommended or Optional priority. Use operation replace with nonempty text,
  or delete with an empty replacement. Do not edit outside the anchored span.
- Record a compact meaning inventory: each source_claim states a proposition
  and its qualifications; preserved_as identifies its expression in the
  replacement, or the retained passage for a redundant deletion. Empty framing
  may be recorded explicitly as carrying no distinct claim. Cover negation,
  scope, conditions, uncertainty, attribution, and necessary formal tokens.
  These are short auditable records, not a reasoning transcript.
- Use requires for companion edit IDs that must be accepted together. Prefer
  independently usable replacements or a single coherent combined span.
- Use deletion_basis not_deletion for replacements. For deletions, use
  empty_framing only if no distinct claim or necessary local function is lost.
  Otherwise use redundant and quote the surviving content in retained_at,
  using the same exact anchor format. That content must remain outside every
  proposed edit. Do not remove a qualification merely because it appears in
  a distant section or serves a different purpose elsewhere.

Do not propose overlapping edits. Do not use vague advice, multiple alternative
rewrites, or personal preference as a benefit. If meaning is ambiguous or
extraction is uncertain, quote the passage under unresolved and omit an edit.

Return zero edits when no clear improvement survives these checks. Select the
most useful edits across the reviewed sections before ordering them by source
position. Sixty edits is a ceiling, not a target. State actual reviewed sections;
set coverage.incomplete and record limitations whenever coverage or worthwhile
recommendations were omitted. Unreviewed passages are not implicitly clean.

Calibration: “Identification depends on the exclusion restriction” can replace
“The exclusion restriction is load-bearing for identification.” Preserve
literal load-bearing walls, robust standard errors, and financial leverage.
“The estimate identifies an association, not a causal effect” contains a
necessary contrast. “Did not significantly increase employment” must never
become “Had no effect on employment.”
