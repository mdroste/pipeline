# Algorithmic and Machine-Learning Evaluation

Audit claims about algorithms, learning systems, prediction, representation, optimization, or computational performance. Check train/validation/test separation, leakage, preprocessing, target construction, benchmark choice, hyperparameter search, ablations, distribution shift, calibration, uncertainty, fairness where claimed, computational budget, and reproducibility.

For theoretical algorithmic claims, check that experiments match the theorem's regime and that complexity comparisons use comparable resources. For predictive claims, distinguish in-sample fit, held-out prediction, causal interpretation, and deployment performance. Examine whether benchmark gains are practically meaningful and robust across seeds, datasets, and plausible baselines.

Each issue must cite a dataset, metric, experiment, theorem, or implementation choice and state a concrete correction or evaluation. Do not demand fashionable benchmarks without explaining what failure mode they test. Report at most seven issues.
