import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import IssuesTable from "./IssuesTable";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

const issues = [
  {
    id: "i1",
    title: "First issue",
    severity: "high",
    section: "1",
    body: "Details",
  },
];

describe("IssuesTable annotation lifecycle", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("ignores a stale annotation load after switching runs", async () => {
    const resolvers = new Map<string, (value: string) => void>();
    invoke.mockImplementation((command: string, args: { runId: string }) => {
      if (command !== "get_annotations") return Promise.resolve();
      return new Promise<string>((resolve) =>
        resolvers.set(args.runId, resolve),
      );
    });

    const view = render(<IssuesTable issues={issues} runId="run-a" />);
    view.rerender(<IssuesTable issues={issues} runId="run-b" />);
    await act(async () =>
      resolvers.get("run-b")?.(
        JSON.stringify({ i1: { status: "", note: "new" } }),
      ),
    );
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: /^First issue/i }));
    expect(screen.getByPlaceholderText("Add a note…")).toHaveValue("new");

    await act(async () =>
      resolvers.get("run-a")?.(
        JSON.stringify({ i1: { status: "", note: "stale" } }),
      ),
    );
    expect(screen.getByPlaceholderText("Add a note…")).toHaveValue("new");
  });

  it("flushes a dirty annotation when the component unmounts", async () => {
    invoke.mockImplementation((command: string) =>
      command === "get_annotations" ? Promise.resolve("{}") : Promise.resolve(),
    );
    const view = render(<IssuesTable issues={issues} runId="run-a" />);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("get_annotations", {
        runId: "run-a",
      }),
    );
    await userEvent.setup().click(screen.getByTitle("accept"));
    view.unmount();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "save_annotations",
        expect.objectContaining({ runId: "run-a" }),
      ),
    );
  });

  it("shows save failures and retries without losing the edit", async () => {
    let saves = 0;
    invoke.mockImplementation((command: string) => {
      if (command === "get_annotations") return Promise.resolve("{}");
      if (command === "save_annotations" && saves++ === 0) {
        return Promise.reject(new Error("disk full"));
      }
      return Promise.resolve();
    });
    render(<IssuesTable issues={issues} runId="run-a" />);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("get_annotations", {
        runId: "run-a",
      }),
    );
    await userEvent.setup().click(screen.getByTitle("accept"));
    expect(
      await screen.findByRole("alert", {}, { timeout: 1500 }),
    ).toHaveTextContent("disk full");

    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() =>
      expect(screen.queryByRole("alert")).not.toBeInTheDocument(),
    );
    expect(invoke).toHaveBeenLastCalledWith(
      "save_annotations",
      expect.objectContaining({
        runId: "run-a",
        content: expect.stringContaining('"status":"accept"'),
      }),
    );
  });

  it("does not let an old run's save clear the current run's persistence state", async () => {
    let finishOldSave!: () => void;
    let saveCalls = 0;
    invoke.mockImplementation((command: string, args: { runId: string }) => {
      if (command === "get_annotations") {
        return args.runId === "run-a"
          ? Promise.resolve("{}")
          : Promise.reject(new Error("run-b load failed"));
      }
      if (command === "save_annotations" && saveCalls++ === 0) {
        return new Promise<void>((resolve) => {
          finishOldSave = resolve;
        });
      }
      return Promise.resolve();
    });

    const view = render(<IssuesTable issues={issues} runId="run-a" />);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("get_annotations", {
        runId: "run-a",
      }),
    );
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "accept issue: First issue" }));
    await waitFor(
      () =>
        expect(invoke).toHaveBeenCalledWith(
          "save_annotations",
          expect.objectContaining({ runId: "run-a" }),
        ),
      { timeout: 1500 },
    );

    view.rerender(<IssuesTable issues={issues} runId="run-b" />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "run-b load failed",
    );
    await act(async () => finishOldSave());

    expect(screen.getByRole("alert")).toHaveTextContent("run-b load failed");
    expect(screen.queryByText("saved ✓")).not.toBeInTheDocument();
  });

  it("accurately surfaces a decision-export save failure", async () => {
    invoke.mockImplementation((command: string) =>
      command === "save_text_file"
        ? Promise.reject(new Error("dialog plugin unavailable"))
        : Promise.resolve(undefined),
    );
    const user = userEvent.setup();
    render(<IssuesTable issues={issues} runId="" />);

    await user.click(
      screen.getByRole("button", { name: "accept issue: First issue" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Export all decisions" }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Decision export failed: dialog plugin unavailable",
    );
  });

  it("labels each quote with Pipeline's own check", async () => {
    const user = userEvent.setup();
    render(
      <IssuesTable
        issues={[
          {
            ...issues[0],
            evidence: [
              { page: 3, quote: "exists", verificationStatus: "verified" },
              { page: 4, quote: "invented", verificationStatus: "not_found" },
              { page: 5, quote: "unchecked" },
            ],
          },
        ]}
        runId=""
      />,
    );
    await user.click(screen.getByRole("button", { name: /^First issue/i }));
    expect(screen.getByText("Quote found")).toHaveAttribute(
      "title",
      expect.stringContaining("not whether the comment is correct"),
    );
    expect(screen.getByText("Quote not found")).toBeInTheDocument();
    expect(screen.queryByText("Not checked")).not.toBeInTheDocument();
  });

  it("opens an evidence-linked page from an expanded issue", async () => {
    const onOpenEvidence = vi.fn();
    const user = userEvent.setup();
    render(
      <IssuesTable
        issues={[
          {
            ...issues[0],
            evidence: [
              {
                page: 7,
                nodeId: "paragraph-12",
                description: "Identification claim",
              },
            ],
          },
        ]}
        runId=""
        onOpenEvidence={onOpenEvidence}
      />,
    );
    await user.click(screen.getByRole("button", { name: /^First issue/i }));
    await user.click(
      screen.getByRole("button", { name: "View Page 7 evidence" }),
    );
    expect(onOpenEvidence).toHaveBeenCalledWith({
      page: 7,
      nodeId: "paragraph-12",
      description: "Identification claim",
    });
  });

  it("retains concrete evidence identifiers and every decision in exports", async () => {
    invoke.mockResolvedValueOnce("/tmp/accepted.md");
    const user = userEvent.setup();
    render(
      <IssuesTable
        issues={[
          {
            ...issues[0],
            evidence: [
              {
                page: 7,
                nodeId: "paragraph-12",
                artifactPath: "artifacts/pages/page-7.png",
                description: "Identification claim",
              },
            ],
          },
        ]}
        runId=""
      />,
    );
    await user.click(
      screen.getByRole("button", { name: "accept issue: First issue" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Export all decisions" }),
    );

    expect(invoke).toHaveBeenCalledWith("save_text_file", {
      suggestedName: "issue-decisions.md",
      content: expect.stringContaining(
        "p. 7, node paragraph-12, artifact artifacts/pages/page-7.png — Identification claim",
      ),
    });
  });

  it("shows the original source path and opens its durable run artifact", async () => {
    const user = userEvent.setup();
    const onOpenEvidence = vi.fn();
    const evidence = {
      sourcePath: "chapters/model.tex",
      artifactPath: "context/source-evidence/chapters/model.tex",
      lineStart: 42,
      lineEnd: 44,
    };
    render(
      <IssuesTable
        issues={[
          {
            ...issues[0],
            title: "Source invariant",
            section: "Correctness",
            evidence: [evidence],
          },
        ]}
        runId=""
        onOpenEvidence={onOpenEvidence}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /^Source invariant/i }),
    );
    const button = screen.getByRole("button", {
      name: "View chapters/model.tex:42–44 evidence",
    });
    expect(button).toBeVisible();
    expect(
      screen.queryByText(/context\/source-evidence/),
    ).not.toBeInTheDocument();
    await user.click(button);
    expect(onOpenEvidence).toHaveBeenCalledWith(evidence);
  });
});

describe("IssuesTable sticky controls", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("keeps filters and errors on top and the bulk bar below the rows", async () => {
    invoke.mockImplementation((command: string) =>
      command === "save_text_file"
        ? Promise.reject(new Error("disk full"))
        : Promise.resolve(undefined),
    );
    const user = userEvent.setup();
    const many = Array.from({ length: 30 }, (_, index) => ({
      ...issues[0],
      id: `i${index + 1}`,
      title: `Issue ${index + 1}`,
    }));
    render(<IssuesTable issues={many} runId="" />);

    const toolbar = screen
      .getByRole("heading", { name: "Issues (30)" })
      .closest(".sticky");
    expect(toolbar).toHaveClass("top-0");
    await user.click(
      screen.getByRole("button", { name: "Export all decisions" }),
    );
    expect(toolbar).toContainElement(await screen.findByRole("alert"));

    const row = screen.getByRole("checkbox", {
      name: "Select issue: Issue 25",
    });
    await user.click(row);
    const bar = screen.getByRole("group", {
      name: "Decision for selected issues",
    });
    expect(bar).toHaveClass("sticky", "bottom-0");
    expect(bar).toHaveTextContent("1 selected");
    // Inserted after the list, so no row above it moves.
    const lastRow = screen.getByRole("checkbox", {
      name: "Select issue: Issue 30",
    });
    expect(
      lastRow.compareDocumentPosition(bar) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});
