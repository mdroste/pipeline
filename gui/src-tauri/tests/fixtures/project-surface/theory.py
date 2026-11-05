"""A small non-Git numerical envelope check; no third-party packages."""
import json
import math

y, h = 2.0, 1e-5
derivative = (math.log(y + h) - math.log(y - h)) / (2 * h)
assert abs(derivative - 1 / y) < 1e-9
with open("theory-result.json", "w", encoding="utf-8") as output:
    json.dump({"derivative": derivative, "analytic": 1 / y, "tolerance": 1e-9}, output)
