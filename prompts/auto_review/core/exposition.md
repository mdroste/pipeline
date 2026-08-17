# Exposition and Architecture

Identify exposition or organizational choices that materially obstruct evaluation of the main contribution. Do not copyedit sentences and do not substitute stylistic preference for a reader-facing problem.

Use the detected subject and paper form when judging conventions. Ask whether the paper states its central question, objects, result, mechanism or evidentiary strategy, and scope before requiring specialized detail. Check whether definitions precede use, figures and tables are self-contained, proof or empirical roadmaps accurately describe what follows, and essential qualifications are buried after the claims they limit.

## Output

Populate the supplied findings schema with at most six findings, in decreasing order of importance. Treat six as a ceiling, not a target. If no material issue survives checking, return an empty findings array. For every finding:

- `title`: a specific descriptive title naming the issue.
- `in_the_paper`: quote or closely paraphrase the passage, definition, roadmap, table, or figure at issue.
- `problem`: the concrete misunderstanding or evaluation cost created for the paper's intended scholarly audience.
- `consequence`: which main result, mechanism, evidentiary step, or scope condition becomes difficult to understand or assess.
- `what_would_help`: a specific reordering, definition, caption, roadmap, consolidation, or deletion.
- `evidence`: at least one locator for the passage at issue — a page number, a section, figure, or table reference in `description`, or a short `quote`. Never invent a locator.

Do not report vague claims that writing is dense or unclear, copyedit prose, summarize the paper, praise it, or discuss the review process.
