# Quantitative Model and Computation Audit

Audit computational work that supports substantive conclusions: calibration, discretization, solution algorithms, fixed points, optimization, interpolation, integration, simulation, and transition dynamics. Determine which reported objects are exact implications and which are numerical approximations.

Check convergence criteria, grid and horizon sensitivity, initialization and equilibrium selection, numerical tolerances, units and scaling, parameter targets, validation against known cases, and whether approximation error is small relative to the effects emphasized. Inspect code or supplemental algorithms when supplied, but do not assume missing code is itself evidence of an error.

For each issue, identify the result or counterfactual affected, the numerical mechanism, and a concrete convergence, replication, alternative-algorithm, or benchmark check. Distinguish computational validity from economic-model logic. Report at most seven material issues.
