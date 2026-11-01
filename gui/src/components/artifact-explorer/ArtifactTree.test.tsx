import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ArtifactEntry, RunManifest } from "../../lib/artifactTypes";
import ArtifactTree from "./ArtifactTree";

const entry = (rel_path: string, label: string, group: string) =>
  ({
    rel_path,
    label,
    group,
    kind: "text",
    bytes: 10,
    sha256: "",
  }) as ArtifactEntry;

function renderTree(artifacts: ArtifactEntry[]) {
  const byGroup = new Map<string, ArtifactEntry[]>();
  for (const artifact of artifacts) {
    byGroup.set(artifact.group, [
      ...(byGroup.get(artifact.group) ?? []),
      artifact,
    ]);
  }
  const selectArtifact = vi.fn();
  render(
    <ArtifactTree
      navigationOpen
      setNavigationOpen={vi.fn()}
      manifest={{ run_id: "run", artifacts } as unknown as RunManifest}
      artifactsByGroup={byGroup}
      selected=""
      selectArtifact={selectArtifact}
    />,
  );
  return selectArtifact;
}

describe("ArtifactTree", () => {
  it("lists findings and named products", () => {
    const select = renderTree([
      entry("findings.json", "Findings", "product"),
      entry(
        "products/evidence_verification.json",
        "evidence_verification",
        "product",
      ),
    ]);
    expect(screen.getByText("Findings and products")).toBeVisible();
    fireEvent.click(
      screen.getByRole("button", { name: /evidence_verification/ }),
    );
    expect(select).toHaveBeenCalledWith("products/evidence_verification.json");
  });

  it("keeps a group it does not name in view", () => {
    renderTree([entry("source-snapshot/main.tex", "main.tex", "source")]);
    expect(screen.getByText("Other files")).toBeVisible();
    expect(screen.getByRole("button", { name: /main\.tex/ })).toBeVisible();
  });

  it("adds no extra section when every group is named", () => {
    renderTree([entry("report.md", "Report", "report")]);
    expect(screen.queryByText("Other files")).not.toBeInTheDocument();
  });
});
