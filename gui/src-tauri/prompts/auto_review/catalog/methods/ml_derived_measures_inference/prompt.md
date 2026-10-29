# ML-Derived Measures and Downstream Inference

Audit analyses whose variables are themselves machine-learning outputs — predicted attributes, imputed classes, or measures extracted from text, images, or sensors — used in downstream estimation. State which variables are model-generated, the generator's training data and validated accuracy, and how the downstream analysis treats the generated values.

Examine nonclassical measurement error correlated with outcomes or covariates; error heterogeneity across groups that manufactures spurious differentials; training-set contamination by, or selection on, downstream variables; propagation of first-stage uncertainty into reported standard errors; validation-sample representativeness for the population where the measure is deployed; and threshold choices converting scores into classes. Check whether the headline estimate needs bias correction or prediction-aware inference rather than treating predictions as observed data.

Require a validation subsample, correction, or sensitivity analysis only when a concrete error structure in the generated measure threatens a central estimate.
