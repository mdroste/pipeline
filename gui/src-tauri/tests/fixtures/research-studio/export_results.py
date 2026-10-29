"""Run as a locally configured Python profile; declare this file as an input."""
import json
from pathlib import Path

x = [1, 2, 3, 4, 5, 6]
y = [2.8, 5.1, 7.2, 8.9, 11.1, 12.9]
xbar, ybar = sum(x) / len(x), sum(y) / len(y)
sxx = sum((value - xbar) ** 2 for value in x)
slope = sum((a - xbar) * (b - ybar) for a, b in zip(x, y)) / sxx
intercept = ybar - slope * xbar
residual_variance = sum((b - intercept - slope * a) ** 2 for a, b in zip(x, y)) / (len(x) - 2)
se = (residual_variance / sxx) ** 0.5
payload = {
    "schema": "research-results-v2",
    "results": [{
        "resultId": "slope", "estimand": "OLS slope of y on x",
        "specificationId": "ols-intercept", "sampleId": "six-observations",
        "estimate": slope, "standardError": se, "confidenceInterval": None,
        "n": len(x), "units": "y units per x unit", "transformation": None,
        "uncertaintyMethod": "homoskedastic OLS standard error",
    }],
    "series": [{
        "resultId": "illustrative-irf", "variable": "output",
        "units": "percent", "shockNormalization": "one percentage-point policy shock",
        "horizonUnit": "quarters", "horizons": [0, 1, 2, 3],
        "values": [0, -0.2, None, -0.1], "specificationId": "illustration",
        "sampleId": "not-estimated",
    }],
}
# No sourceExecutionId is supplied: Pipeline binds provenance to the adopted file.
Path("results.json").write_text(json.dumps(payload, indent=2, allow_nan=False) + "\n")
print(f"OLS slope: {slope}; standard error: {se}. IRF is illustrative, not estimated.")
