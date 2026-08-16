# ML-Based Heterogeneous Effects

Audit claims about treatment-effect heterogeneity or targeting derived from machine learning — causal forests, meta-learners, double/debiased ML for conditional effects, or policy learning. State the identifying assumption inherited from the underlying design, the estimand (effect surface, group contrasts, policy value), and the sample-splitting or cross-fitting scheme.

Examine whether honesty or cross-fitting actually protects the reported inference; whether discovered subgroups are validated out-of-fold rather than selected after inspection; calibration of estimated effects against observable benchmarks; the distinction between heterogeneity in true effects and heterogeneity in noise; nuisance-model quality and overlap in the regions driving the heterogeneity claims; and whether policy-value comparisons use valid counterfactual evaluation. Check that headline subgroups are not artifacts of a single split or seed.

Require additional validation only when a concrete overfitting or selection channel threatens a central heterogeneity or targeting claim. Do not demand machine learning where a prespecified interaction answers the question.
