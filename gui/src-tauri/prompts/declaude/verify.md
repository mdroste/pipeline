Verify each consolidated prose edit against the source under the shared
editing contract. Return the structured verification record. The host renders
accepted edits; do not write Markdown, new replacements, or additional edits.
Echo the host's stage and source_hash exactly.

Give exactly one accept or reject decision for every consolidated candidate
ID, including rejected candidates. The decision applies to the candidate's
exact original, replacement, operation, and companion requirements. Do not
repair an edit during verification: reject it with the specific problem.

For each candidate:

1. Independently establish the source's claims from the anchored passage and
   surrounding context before relying on earlier preservation summaries.
   Exact text matching establishes identity only. Check OCR uncertainty and
   source metadata; reject an edit that requires resolving unreadable text.
2. Require a concrete editorial benefit appropriate to the intended voice and
   local rhetorical purpose. Reject suspect-word substitutions, cosmetic
   churn, and edits whose only benefit is brevity. Preserve purposeful
   contrasts, repetition, emphasis, and technical terms.
3. Record a concise source_claim / preserved_as inventory. Check both ways:
   no distinct source assertion or qualification is lost, and no assertion,
   mechanism, implication, or stronger claim is introduced. Check negation,
   quantifier scope, necessity versus sufficiency, modal strength, conditions,
   counterfactual baselines, time horizons, attribution, citation scope,
   numerical values, and formal content. A summary such as “meaning preserved”
   is insufficient. For a rejection identify the missing or changed content.
4. Read the replacement in place. Check boundaries, definitions, referents,
   grammar, local format, and the attachment of qualifications and citations.
   Do not claim visual layout verification without inspecting the visuals.
5. Inspect the host's combined proposed-document preview at affected passages.
   Evaluate the entire set you intend to accept, including the consequences of
   rejecting other candidates. Every accepted requires ID must also be accepted.
   Reject a dependent edit if its companion is rejected. Check that each
   retained_at witness survives and still serves the necessary local purpose.
   Record the concrete interaction check, including why standalone edits work
   independently or which companions must be accepted together.

The final accepted set must not lose information through two individually
plausible deletions. Do not turn “this qualification appears elsewhere” into
permission to remove it beside the result where readers need it. Reject when
combined meaning cannot be established confidently.

Carry actual coverage and limitations forward; preserve unresolved wording.
Zero accepted edits is valid. Do not infer AI authorship, assign AI scores,
judge scientific validity, or add research demands.

SOURCE CONTEXT
{orientation}
Readable document: {paper_path}
Document index: {document_bundle}
Original source: {source_path}

SUBJECT EDITOR 1
{step:declaude_referee_1}

SUBJECT EDITOR 2
{step:declaude_referee_2}

SUBJECT EDITOR 3
{step:declaude_referee_3}


CONSOLIDATED CANDIDATES
{step:declaude_editor}
