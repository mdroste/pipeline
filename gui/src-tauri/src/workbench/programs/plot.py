"""Pipeline scientific figure adapter v1. Inputs and script are captured together.
Requires an explicitly selected Python environment with matplotlib. No installs.
"""
import json
import textwrap
from pathlib import Path
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

spec = json.loads(Path("figure.json").read_text(encoding="utf-8"))
plt.rcParams.update({"font.size": 10, "pdf.fonttype": 42, "svg.fonttype": "none"})
entries = spec["entries"]
def label(entry):
    return entry["label"] + (" (manual)" if entry.get("manual") else "")
fig, ax = plt.subplots(figsize=(7.0, max(3.4, min(10, .5 * len(entries) + 1.8))))
styles = ["-", "--", "-.", ":"]
if spec["kind"] == "irf":
    for index, entry in enumerate(entries):
        series = entry["value"]["series"]
        # Missing points remain gaps, including leading/trailing observations.
        ys = [float("nan") if v is None else v for v in series["values"]]
        ax.plot(series["horizons"], ys, styles[index % 4], color=str(.1 + (index % 4) * .18), label=textwrap.fill(label(entry), 38))
    ax.set_xlabel(entries[0]["value"]["series"]["horizonUnit"])
    ax.set_ylabel(entries[0]["value"]["series"]["units"])
    ax.axhline(0, color="0.7", linewidth=.6)
    ax.legend(loc="best", frameon=False)
else:
    for index, entry in enumerate(entries):
        result = entry["value"]
        estimate = result["estimate"]
        ax.plot([estimate], [index], "o", color="0.1")
        interval = result.get("confidenceInterval") if spec["uncertainty"] == "ci" else None
        if interval is not None:
            ax.hlines(index, interval[0], interval[1], color="0.3", linewidth=1.2)
        elif spec["uncertainty"] == "se" and result.get("standardError") is not None:
            se = result["standardError"]
            ax.hlines(index, estimate-se, estimate+se, color="0.3", linewidth=1.2)
    ax.set_yticks(range(len(entries)), [textwrap.fill(label(e), 32) for e in entries])
    ax.set_ylim(len(entries) - .5, -.5)
    ax.set_xlabel(entries[0]["value"]["units"])
    ax.axvline(0, color="0.7", linewidth=.6)
ax.set_title(textwrap.fill(spec["title"], 65))
ax.spines[["top", "right"]].set_visible(False)
if spec["notes"]:
    fig.text(.02, .01, textwrap.fill(spec["notes"], 100), va="bottom", fontsize=8)
# Reserve actual space for wrapped notes rather than a fixed small strip.
note_lines = max(1, len(textwrap.wrap(spec["notes"], 100))) if spec["notes"] else 0
notes_height = note_lines * .13 + (.12 if note_lines else 0)
fig.set_figheight(fig.get_figheight() + notes_height)
fig.tight_layout(rect=(0, notes_height / fig.get_figheight(), 1, 1))
for extension in ("svg", "pdf", "png"):
    fig.savefig("figure." + extension, dpi=180, bbox_inches="tight", metadata={"Creator": "Pipeline figure adapter v1"} if extension != "svg" else None)
Path("figure-receipt.json").write_text(json.dumps({"adapter":"pipeline-matplotlib-v1","matplotlibVersion":matplotlib.__version__,"sourceCount":len(entries),"missingPolicy":"gaps","uncertainty":spec["uncertainty"]}), encoding="utf-8")
plt.close(fig)
