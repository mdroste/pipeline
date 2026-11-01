import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import Bindings from "./Bindings";
import type { ProjectHome } from "../../lib/projectClient";

const mocks = vi.hoisted(() => ({
  coverage: vi.fn(),
  mutate: vi.fn(),
  listStructuredResults: vi.fn(),
}));
vi.mock("../../lib/studioClient", async (original) => ({
  ...(await original<typeof import("../../lib/studioClient")>()),
  studioClient: { coverage: mocks.coverage, mutate: mocks.mutate },
}));
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: { listStructuredResults: mocks.listStructuredResults },
}));

const data = {
  changes: [],
  anchors: [
    {
      id: "a1",
      body: {
        selection: { revisionId: "rev", quote: "an effect of 0.42" },
        body: "an effect of 0.42",
      },
    },
  ],
} as unknown as ProjectHome;

it("updates the saved binding when it is saved again", async () => {
  mocks.coverage.mockResolvedValue([]);
  mocks.listStructuredResults.mockResolvedValue([
    { sourceExecutionId: "exec-1", resultId: "effect", units: "pp" },
  ]);
  mocks.mutate.mockResolvedValue({ id: "b1", revision: 1 });
  render(
    <Bindings
      workspaceId="ws"
      data={data}
      onRefresh={async () => {}}
      onDocument={() => {}}
    />,
  );
  const result = screen.getByLabelText("Saved result");
  await waitFor(() =>
    expect(result.querySelectorAll("option").length).toBeGreaterThan(0),
  );
  fireEvent.change(result, { target: { value: "exec-1/effect" } });
  fireEvent.change(screen.getByLabelText("Exact manuscript selection"), {
    target: { value: "a1" },
  });
  const save = screen.getByText("Save numeric binding");
  fireEvent.click(save);
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenLastCalledWith(
      "ws",
      expect.objectContaining({ action: "bindNumber", id: null }),
    ),
  );
  await waitFor(() => expect(save).not.toBeDisabled());
  fireEvent.click(save);
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenLastCalledWith(
      "ws",
      expect.objectContaining({ action: "bindNumber", id: "b1" }),
    ),
  );
});

const view = (active?: boolean) => (
  <Bindings
    workspaceId="ws"
    data={data}
    onRefresh={async () => {}}
    onDocument={() => {}}
    active={active}
  />
);
const saved = (resultId: string) => ({
  sourceExecutionId: "exec-1",
  resultId,
  units: "pp",
});

it("lists newly imported results after the refresh button", async () => {
  mocks.coverage.mockResolvedValue([]);
  mocks.listStructuredResults.mockResolvedValue([saved("effect")]);
  render(view());
  await screen.findByRole("option", { name: /effect/ });
  mocks.listStructuredResults.mockResolvedValue([
    saved("effect"),
    saved("later"),
  ]);
  fireEvent.click(
    screen.getByRole("button", {
      name: "Refresh dependent results and passages",
    }),
  );
  expect(await screen.findByRole("option", { name: /later/ })).toBeTruthy();
});

it("reloads results when the view comes back and keeps the typed value", async () => {
  mocks.coverage.mockResolvedValue([]);
  mocks.listStructuredResults.mockResolvedValue([saved("effect")]);
  const page = render(view(true));
  await screen.findByRole("option", { name: /effect/ });
  fireEvent.change(screen.getByLabelText("Exact printed value in passage"), {
    target: { value: "0.42" },
  });
  mocks.listStructuredResults.mockResolvedValue([
    saved("effect"),
    saved("later"),
  ]);
  page.rerender(view(false));
  page.rerender(view(true));
  expect(await screen.findByRole("option", { name: /later/ })).toBeTruthy();
  expect(screen.getByLabelText("Exact printed value in passage")).toHaveValue(
    "0.42",
  );
});

const covered = {
  record: {
    id: "b1",
    body: { anchorId: "a1", role: "headline", origin: "user" },
  },
  anchor: { selection: { revisionId: "rev", quote: "an effect of 0.42" } },
  expected: null,
  state: "current",
  reasons: [],
};

it("opens the saved passage itself from a result link", async () => {
  mocks.coverage.mockResolvedValue([covered]);
  mocks.listStructuredResults.mockResolvedValue([]);
  const onAnchors = vi.fn();
  const onDocument = vi.fn();
  render(
    <Bindings
      workspaceId="ws"
      data={data}
      onRefresh={async () => {}}
      onDocument={onDocument}
      onAnchors={onAnchors}
    />,
  );
  fireEvent.click(await screen.findByText("Open exact passage"));
  expect(onAnchors).toHaveBeenCalledWith("a1", null);
  expect(onDocument).not.toHaveBeenCalled();
});

it("shows the message of a structured load failure", async () => {
  mocks.coverage.mockResolvedValue([]);
  mocks.listStructuredResults.mockRejectedValue({ message: "Busy" });
  render(view());
  expect(await screen.findByRole("alert")).toHaveTextContent(/^Busy$/);
});
