"""Pipeline result export v1. Standard-library only; provenance is attached by import."""
import json
import math
from pathlib import Path


def coefficient(result_id, estimate, *, estimand, specification_id, sample_id, units,
                standard_error=None, confidence_interval=None, n=None,
                transformation=None, uncertainty_method=None):
    values = [estimate] + ([] if standard_error is None else [standard_error])
    values += list(confidence_interval or [])
    if not all(math.isfinite(v) for v in values):
        raise ValueError("Estimates and uncertainty must be finite; missing is None")
    if standard_error is not None and standard_error < 0:
        raise ValueError("Standard errors must be nonnegative")
    if confidence_interval is not None and (len(confidence_interval) != 2 or confidence_interval[0] > confidence_interval[1]):
        raise ValueError("Confidence interval must be an ordered pair")
    if n is not None and (isinstance(n, bool) or not isinstance(n, int) or n < 0):
        raise ValueError("Sample size must be a nonnegative integer or None")
    return dict(resultId=result_id, estimate=estimate, estimand=estimand,
                specificationId=specification_id, sampleId=sample_id, units=units,
                standardError=standard_error, confidenceInterval=confidence_interval,
                n=n, transformation=transformation, uncertaintyMethod=uncertainty_method,
                sourceExecutionId="", artifactLocator="")


def write_results(path, results, *, convergence=None, diagnostics=None):
    if not 1 <= len(results) <= 1000:
        raise ValueError("Export 1–1000 explicitly identified results")
    if len({r['resultId'] for r in results}) != len(results):
        raise ValueError("Result IDs must be unique within an execution")
    payload = dict(schema="research-results-v1", results=results,
                   diagnostics=dict(convergence=convergence, declared=diagnostics or {}))
    Path(path).write_text(json.dumps(payload, allow_nan=False, indent=2), encoding="utf-8")
