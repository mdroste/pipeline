# Scientific ML and Surrogate Models

Audit scientific conclusions supported by learned surrogates or emulators — physics-informed networks, machine-learned force fields, emulated simulators, or neural solvers. State what the surrogate replaces, its training data and domain, and which scientific claims rest on surrogate output.

Examine train-test leakage across simulation runs, trajectories, or parameter regimes; extrapolation beyond the training domain in exactly the regions where conclusions are drawn; violation of conservation laws or physical constraints and its accumulation over rollouts; error metrics that average away scientifically decisive failures; baseline comparisons against the numerical methods practitioners actually use, at matched cost; and whether uncertainty estimates on surrogate predictions are calibrated. Check that speedup claims include training and data-generation cost where the comparison depends on them.

Request an out-of-domain validation, constraint diagnostic, or numerical-baseline comparison only when a concrete surrogate failure mode threatens the scientific claim rather than the engineering convenience.
