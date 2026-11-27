import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import ConversationTabs from "./ConversationTabs";
import useConversationTabs, {
  loadConversationTabs,
  neighbourTab,
} from "../../hooks/workspace/useConversationTabs";

beforeEach(() => localStorage.clear());

const sessions = [
  { id: "a", title: "Alpha" },
  { id: "b", title: "Beta" },
  { id: "c", title: "Gamma" },
];

let api: ReturnType<typeof useConversationTabs>;
function Harness({
  workspaceId = "project",
  sessionId,
  list = sessions,
}: {
  workspaceId?: string | null;
  sessionId: string | null;
  list?: { id: string; title: string }[];
}) {
  api = useConversationTabs(workspaceId, sessionId, list);
  return <p>{api.tabs.map((tab) => tab.title).join(", ")}</p>;
}

it("opens a tab for each conversation selected and remembers them per project", () => {
  const view = render(<Harness sessionId="a" />);
  view.rerender(<Harness sessionId="b" />);
  view.rerender(<Harness sessionId="a" />);
  expect(screen.getByText("Alpha, Beta")).toBeInTheDocument();
  expect(loadConversationTabs("project")).toEqual([
    { id: "a", title: "Alpha" },
    { id: "b", title: "Beta" },
  ]);
  // Another project has its own tabs; coming back restores these.
  view.rerender(<Harness workspaceId="other" sessionId="c" />);
  expect(screen.getByText("Gamma")).toBeInTheDocument();
  view.rerender(<Harness sessionId="a" />);
  expect(screen.getByText("Alpha, Beta")).toBeInTheDocument();
});

it("keeps a closed-over title for conversations outside the loaded list", () => {
  const view = render(<Harness sessionId="a" />);
  view.rerender(
    <Harness sessionId="a" list={[{ id: "a", title: "Alpha renamed" }]} />,
  );
  expect(screen.getByText("Alpha renamed")).toBeInTheDocument();
  // The list paged the conversation out; its tab keeps the last title.
  view.rerender(<Harness sessionId={null} list={[]} />);
  expect(screen.getByText("Alpha renamed")).toBeInTheDocument();
});

it("reorders, closes and forgets tabs, including another project's", () => {
  localStorage.setItem(
    "pipeline.workspace.openConversations.other",
    JSON.stringify([{ id: "x", title: "Elsewhere" }]),
  );
  const view = render(<Harness sessionId="a" />);
  view.rerender(<Harness sessionId="b" />);
  view.rerender(<Harness sessionId="c" />);
  act(() => api.move("c", 0));
  expect(screen.getByText("Gamma, Alpha, Beta")).toBeInTheDocument();
  act(() => api.close("a"));
  expect(screen.getByText("Gamma, Beta")).toBeInTheDocument();
  act(() => api.forget("b", "project"));
  expect(screen.getByText("Gamma")).toBeInTheDocument();
  act(() => api.forget("x", "other"));
  expect(loadConversationTabs("other")).toEqual([]);
});

it("ignores damaged saved tabs", () => {
  localStorage.setItem(
    "pipeline.workspace.openConversations.project",
    JSON.stringify([{ id: "a", title: "Alpha" }, { id: 3 }, "b", null]),
  );
  expect(loadConversationTabs("project")).toEqual([
    { id: "a", title: "Alpha" },
  ]);
  localStorage.setItem("pipeline.workspace.openConversations.project", "{");
  expect(loadConversationTabs("project")).toEqual([]);
});

it("hands a closed tab's place to the next tab, else the previous one", () => {
  expect(neighbourTab(sessions, "a")).toBe("b");
  expect(neighbourTab(sessions, "c")).toBe("b");
  expect(neighbourTab([sessions[0]], "a")).toBeNull();
});

it("selects, closes and reorders tabs from the keyboard and pointer", () => {
  const select = vi.fn();
  const close = vi.fn();
  const move = vi.fn();
  render(
    <ConversationTabs
      tabs={sessions}
      active="b"
      working
      disabled={false}
      onSelect={select}
      onClose={close}
      onMove={move}
    />,
  );
  const strip = screen.getByRole("tablist", { name: "Open conversations" });
  const tabs = within(strip).getAllByRole("tab");
  expect(tabs.map((tab) => tab.textContent)).toEqual([
    "Alpha",
    "Beta",
    "Gamma",
  ]);
  expect(tabs[1]).toHaveAttribute("aria-selected", "true");
  // Only the selected, replying conversation shows the working dot.
  expect(document.querySelectorAll(".workspace-tab-working")).toHaveLength(1);
  expect(tabs[1].querySelector(".workspace-tab-working")).not.toBeNull();
  fireEvent.click(tabs[2]);
  expect(select).toHaveBeenCalledWith("c");
  fireEvent.keyDown(tabs[1], { key: "Delete" });
  expect(close).toHaveBeenCalledWith("b");
  fireEvent.keyDown(tabs[1], {
    key: "PageDown",
    ctrlKey: true,
    shiftKey: true,
  });
  expect(move).toHaveBeenCalledWith("b", 2);
  fireEvent.click(screen.getByRole("button", { name: "Close Gamma" }));
  expect(close).toHaveBeenCalledWith("c");
});

it("keeps the selected tab usable while switching is unavailable", () => {
  render(
    <ConversationTabs
      tabs={sessions}
      active="a"
      working={false}
      disabled
      onSelect={vi.fn()}
      onClose={vi.fn()}
      onMove={vi.fn()}
    />,
  );
  const tabs = screen.getAllByRole("tab");
  expect(tabs[0]).toBeEnabled();
  expect(tabs[1]).toBeDisabled();
  expect(screen.getByRole("button", { name: "Close Alpha" })).toBeDisabled();
});
