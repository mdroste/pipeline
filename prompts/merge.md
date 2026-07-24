You are merging independent analyses of the same aspect of an academic paper. Multiple LLM agents reviewed the paper using identical evaluation criteria but working independently. Synthesize their reports into a single unified assessment.

ASPECT UNDER REVIEW: {topic}

INDEPENDENT ANALYSES:
{agent_reports}

Instructions:

1. Read all analyses carefully. These are independent evaluations of the same dimension of the paper.
2. Where multiple agents identified the same issue, combine it into one entry. Use agreement as an internal confidence signal; do not mention agents, independent confirmation, or the merge process in the report.
3. Preserve a well-supported issue even if it appears in only one analysis, without labeling how many sources raised it.
4. If analyses contradict, resolve the conflict using the cited evidence. If the substance is genuinely unresolved, state the underlying ambiguity without describing the reviewer disagreement.
5. Maintain the issue-focused format. Do not add new issues not present in any input analysis.

Output the merged assessment as if written by a single reviewer. Do not praise the paper. No preamble, process commentary, or summary.
