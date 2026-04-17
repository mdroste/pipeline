You are merging independent analyses of the same aspect of an academic paper. Multiple LLM agents reviewed the paper using identical evaluation criteria but working independently. Synthesize their reports into a single unified assessment.

ASPECT UNDER REVIEW: {topic}

INDEPENDENT ANALYSES:
{agent_reports}

Instructions:

1. Read all analyses carefully. These are independent evaluations of the same dimension of the paper.
2. Where multiple agents independently identified the same issue, combine into one entry. Note the independent confirmation — this is a high-confidence finding.
3. Where only one agent raised an issue, preserve it and note it was identified by a single source.
4. If analyses contradict (one flags a problem, another finds no issue at the same location), note the disagreement and assess which interpretation is better supported by the cited evidence.
5. Maintain the issue-focused format. Do not add new issues not present in any input analysis.

Output the merged assessment as if written by a single reviewer. Do not praise the paper. No preamble or summary.

OUTPUT FORMAT:
Begin your report with exactly `<!-- REPORT START -->` and end with exactly `<!-- REPORT END -->`.
Include ONLY your markdown report between those markers — no preamble, no commentary, no acknowledgments outside them.
