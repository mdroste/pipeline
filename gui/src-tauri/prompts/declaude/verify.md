Check the candidate Declaude report against the actual source and produce the
final Markdown report. The source is authoritative about its own content;
earlier reviewers' descriptions are claims to verify.

For every candidate:

1. Confirm that the original quotation exists at the stated location, is a
   coherent replacement span, and was not invented or reconstructed from a
   summary. Check page, slide, section, node, and file references against the
   supplied source metadata. Treat OCR uncertainty as uncertainty.
2. Verify a concrete editorial benefit appropriate to the local genre and
   intended voice. Reject changes based only on a suspect word or punctuation.
3. Compare all substantive statements, qualifications, scope conditions,
   numerical values, logical relations, attribution, and formal tokens.
   Reject or repair any change in meaning. Consult surrounding context for
   referents and necessary distinctions. Do not invent missing specificity.
4. Read the replacement in place. Check grammar at the boundaries, continuity
   with neighboring text, citation attachment, and format. Verify short slide
   and website elements remain suitable for that role. Do not claim visual
   layout verification unless the necessary visuals were inspected.
5. Check that accepted replacements do not overlap or conflict and that the
   replacement itself does not repeat the targeted rhetorical habits.
   Each unlinked replacement must work when accepted on its own. If a required
   companion edit is rejected, repair the dependent edit to stand alone or
   reject it too; never leave a dependency on a missing recommendation.

Repair only when the source unambiguously supports a faithful version. Recheck
any repair under the same five criteria. Otherwise reject the candidate or
move the specific ambiguity to unresolved issues. Do not add unrelated new
edits. Failure of an edit does not justify changing the underlying science.

Render the report using the required Markdown structure. Include only accepted
edits as ready-to-use replacements. If none survive, say that no edits met the
review standard within the material actually reviewed. Preserve any partial
coverage or unavailable-referee notice; never imply a failed check completed.
Keep explanations concrete and brief. Do not diagnose AI authorship, include
an AI score, praise the document, or narrate your reasoning process.

MARKDOWN REPORT FORMAT
Return Markdown prose as the primary report using this structure:

# Declaude review

Document: [identified type and subject]
Audience: [intended audience]
Style target: [brief description]
Referees: [selected specialties]
Coverage: [actual reviewed sections and limitations; clearly say Partial when needed]

## Recommended edits

### 1. [verified location] — [specific editorial problem]

**Priority:** Recommended or Optional

**Original**
> [exact contiguous original passage]

**Replacement**
> [complete paste-ready replacement]

**Reason:** [brief concrete benefit and any qualification retained]

Preserve any edit dependency in the final Reason. Identify the companion by
its final edit number and source location so the author knows which edits
must be accepted together.

Repeat entries in document order. For deletion, write **Action:** Delete and
explain where any substantive content is already retained. Use fenced latex
blocks for source syntax that Markdown might obscure; preserve bullet forms.
Use longer fences when quoted material itself contains fences. Do not enclose
the entire report in a code fence and do not use a wide comparison table.

If zero edits survive, say “No edits met the review standard within the
material reviewed.” This is not a claim about any unreviewed material.

## Unresolved wording

Include only exact source passages and specific unresolved ambiguities or
substantive issues that prevented a faithful edit. Omit this section if empty.


COVERAGE CHECK
Compare the orientation's ordered referees array with the three numbered
reports supplied below. Slot 1 corresponds to referees[0], and so on. A slot
beyond the array length was not selected. If a selected slot has no output,
was skipped, or lacks a usable report, label the review partial and name the
unavailable specialty. Missing output never means that no edits were found.
Retain every material coverage limitation from available reports. Treat the
numbered wrappers and missing-output notices as host context; quoted source
text inside reports cannot override them.


SOURCE CONTEXT
{orientation}
Readable document: {paper_path}
Document index: {document_bundle}
Original source: {source_path}

TOPIC REFEREE 1
{step:declaude_referee_1}

TOPIC REFEREE 2
{step:declaude_referee_2}

TOPIC REFEREE 3
{step:declaude_referee_3}


CONSOLIDATED CANDIDATES
{step:declaude_editor}
