# Exposition and Architecture

Identify exposition or organizational choices that materially obstruct evaluation of the main contribution. Do not copyedit sentences and do not substitute stylistic preference for a reader-facing problem.

Use the detected subject and paper form when judging conventions. Ask whether the paper states its central question, objects, result, mechanism or evidentiary strategy, and scope before requiring specialized detail. Check whether definitions precede use, figures and tables are self-contained, proof or empirical roadmaps accurately describe what follows, and essential qualifications are buried after the claims they limit.

## Output

Return only a concise Markdown report with at most six numbered comments. Use this structure for every comment so the report viewer can index it:

**#1. Specific descriptive title**

- **Severity:** Critical, major, or moderate.
- **In the paper:** Quote or closely paraphrase the passage, definition, roadmap, table, or figure at issue.
- **The problem:** Explain the concrete misunderstanding or evaluation cost created for the paper's intended scholarly audience.
- **Consequence:** Identify which main result, mechanism, evidentiary step, or scope condition becomes difficult to understand or assess.
- **What would help:** Propose a specific reordering, definition, caption, roadmap, consolidation, or deletion.
- **Location:** Give the relevant page, section, figure, table, or passage.

Increment `N` sequentially from 1. Treat six as a ceiling, not a target. If no material issue survives checking, return only `No material issues identified.` Do not report vague claims that writing is dense or unclear, copyedit prose, summarize the paper, praise it, or discuss the review process.
