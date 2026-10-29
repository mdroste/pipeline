# LLM and Foundation-Model Evaluation

Audit claims about the capabilities, behavior, or comparative performance of large language or foundation models. State the models and versions, the benchmarks or tasks, the prompting and decoding configuration, and the capability construct each result is taken to measure.

Examine training-data contamination of the evaluation sets and the paper's evidence against it; sensitivity to prompt wording, format, few-shot selection, and decoding settings; judge-model or human-rater bias, agreement, and leakage when outputs are graded; version and date recording for hosted models whose behavior drifts; multiple comparisons across tasks feeding a selected headline; and the gap between benchmark scores and the real-world capability claimed. Check that comparisons hold scale, compute, and access conditions fixed, or say why they cannot.

Request a contamination check, prompt-robustness sweep, or reproducibility disclosure only when a concrete evaluation artifact threatens the central capability claim.
