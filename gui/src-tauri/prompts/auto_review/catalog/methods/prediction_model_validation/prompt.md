# Prediction-Model Development and Validation

Audit models whose contribution is predicting an outcome for individuals — clinical risk scores, credit or recidivism models, or comparable prediction tools. State the prediction target and horizon, the intended use and population, the development and validation samples, and the performance claims.

Examine discrimination and calibration in validation data — calibration especially, since it decays first outside development samples; optimism from in-sample or weakly separated validation; outcome and predictor leakage, including variables unavailable at prediction time; effective sample size relative to events; handling of missing predictors as they would occur at deployment; performance heterogeneity across relevant subgroups; and comparison against existing tools and simple baselines. Check that decision-utility claims rest on decision-analytic evidence rather than discrimination alone.

Request an external validation, recalibration, or baseline comparison only when a concrete gap between development and deployment conditions threatens the claimed usefulness.
