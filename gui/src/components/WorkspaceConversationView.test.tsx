import { act, fireEvent, render, screen } from "@testing-library/react";
import { createRef, type ComponentProps } from "react";
import { afterEach, expect, it, vi } from "vitest";
import WorkspaceConversationView from "./WorkspaceConversationView";
import type { TranscriptItem } from "../lib/workbenchTypes";

const markdown = vi.hoisted(() => ({ rendered: [] as string[] }));
vi.mock("react-markdown", () => ({
  default: ({ children }: { children: string }) => {
    markdown.rendered.push(children);
    return <div>{children}</div>;
  },
}));

vi.mock("../lib/workbench/workspaceClient", () => ({ workspaceClient: {} }));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: {},
}));
vi.mock("../lib/workbench/harnessClient", () => ({ harnessClient: {} }));
vi.mock("../lib/workbench/researchClient", () => ({ researchClient: {} }));
vi.mock("../lib/workbench/executionClient", () => ({ executionClient: {} }));
vi.mock("../lib/workbench/recipeClient", () => ({ recipeClient: {} }));
vi.mock("../lib/workbench/releaseClient", () => ({ releaseClient: {} }));
vi.mock("../lib/workbench/runtimeClient", () => ({ runtimeClient: {} }));

const onSend = vi.fn();
const onStop = vi.fn();
const view = (
  props: Partial<ComponentProps<typeof WorkspaceConversationView>> = {},
) => (
  <WorkspaceConversationView
    header={null}
    requests={null}
    composerMenu={<button>Attach</button>}
    contextTray={null}
    snapshot={null}
    renderedItems={[]}
    totalItems={0}
    pendingUser={null}
    stream=""
    selectedMessage={null}
    transcriptStart={0}
    transcriptEnd={0}
    transcriptRef={createRef()}
    messageRef={createRef()}
    error={null}
    draft=""
    disabled={false}
    active={false}
    submitting={false}
    contextBusy={false}
    onDraft={vi.fn()}
    onSend={onSend}
    onStop={onStop}
    onLatest={vi.fn()}
    onEarlier={vi.fn()}
    onNewer={vi.fn()}
    followLatestRef={{ current: true }}
    {...props}
  />
);

it("uses one control for Send and Stop so the composer row never shifts", () => {
  const page = render(view());
  const control = screen.getByRole("button", { name: "Send" });
  expect(control).toBeDisabled();
  page.rerender(view({ draft: "A question" }));
  expect(control).toBeEnabled();
  fireEvent.click(control);
  expect(onSend).toHaveBeenCalledOnce();
  // While a response runs the same element stops it, even with an empty draft.
  page.rerender(view({ active: true, disabled: true }));
  expect(control).toHaveAccessibleName("Stop response");
  expect(control).toBeEnabled();
  fireEvent.click(control);
  expect(onStop).toHaveBeenCalledOnce();
  expect(onSend).toHaveBeenCalledOnce();
  page.rerender(view({ submitting: true, disabled: true, draft: "Sent" }));
  expect(control).toHaveAccessibleName("Sending…");
  expect(control).toBeDisabled();
});

it("states the send shortcut in the field and has no manual resize grip", () => {
  render(view());
  const field = screen.getByRole("textbox", { name: "Message" });
  expect(field).toHaveAttribute("placeholder", "Ask ChatGPT… Enter to send");
  expect(field).toHaveAttribute("rows", "1");
  expect(field).toHaveClass("workspace-composer-input");
});

it("shows where dropped files will go and when they are being added", () => {
  const page = render(view({ dropState: "over" }));
  expect(screen.getByRole("status")).toHaveTextContent(
    "Drop to add these files to the conversation",
  );
  page.rerender(view({ dropState: "importing" }));
  expect(screen.getByRole("status")).toHaveTextContent("Adding files…");
  page.rerender(view());
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
});

const message = (index: number): TranscriptItem => ({
  id: `m${index}`,
  turnId: null,
  providerItemId: `p${index}`,
  itemKind: index % 2 ? "agentMessage" : "userMessage",
  payload: { text: `Message ${index}` },
  isFinal: true,
  createdAt: "now",
  updatedAt: "now",
});
const snapshot = {
  workspace: null,
  session: { id: "s" },
  sequence: 1,
  activeBinding: null,
  turns: [],
  items: [],
} as never;
/** jsdom has no layout: give the transcript fixed scroll metrics. */
function metrics(node: HTMLElement, size: { height: number; client: number }) {
  Object.defineProperty(node, "scrollHeight", {
    configurable: true,
    get: () => size.height,
  });
  Object.defineProperty(node, "clientHeight", {
    configurable: true,
    get: () => size.client,
  });
  let top = 0;
  Object.defineProperty(node, "scrollTop", {
    configurable: true,
    get: () => top,
    set: (value: number) => {
      top = Math.max(0, Math.min(value, size.height - size.client));
    },
  });
}
let observers: Array<() => void> = [];
afterEach(() => {
  observers = [];
  vi.unstubAllGlobals();
});
function stubResizeObserver() {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      callback: () => void;
      constructor(callback: () => void) {
        this.callback = callback;
        observers.push(() => callback());
      }
      observe() {}
      disconnect() {}
    },
  );
}

it("stops following on a small upward scroll and resumes only at the end", () => {
  const transcriptRef = createRef<HTMLDivElement>();
  const followLatestRef = { current: true };
  const items = [message(0), message(1)];
  const props = {
    snapshot,
    transcriptRef,
    followLatestRef,
    renderedItems: items,
    totalItems: 2,
    transcriptEnd: 2,
  };
  const page = render(view({ ...props, stream: "a" }));
  const node = transcriptRef.current!;
  const size = { height: 1000, client: 400 };
  metrics(node, size);
  page.rerender(view({ ...props, stream: "ab" }));
  expect(node.scrollTop).toBe(600);
  fireEvent.scroll(node);
  // A slow trackpad gesture: two pixels up, then more text streams in.
  node.scrollTop = 598;
  fireEvent.scroll(node);
  expect(followLatestRef.current).toBe(false);
  size.height = 1100;
  page.rerender(view({ ...props, stream: "abc" }));
  expect(node.scrollTop).toBe(598);
  // The control sits after the messages, out of flow, so the text stays put.
  const latest = screen.getByRole("button", { name: "↓ Latest" });
  expect(node.firstElementChild).toHaveClass("workspace-transcript-content");
  expect(node.lastElementChild).toBe(latest.parentElement);
  expect(latest.parentElement).toHaveClass("workspace-latest-control");
  // A wheel turn upward stops following before any scroll event arrives.
  node.scrollTop = 700;
  fireEvent.scroll(node);
  node.scrollTop = 1100;
  fireEvent.scroll(node);
  expect(followLatestRef.current).toBe(true);
  fireEvent.wheel(node, { deltaY: -4 });
  expect(followLatestRef.current).toBe(false);
  node.scrollTop = 700;
  fireEvent.scroll(node);
  fireEvent.keyDown(node, { key: "End" });
  node.scrollTop = 1100;
  fireEvent.scroll(node);
  expect(followLatestRef.current).toBe(true);
  fireEvent.keyDown(node, { key: "PageUp" });
  expect(followLatestRef.current).toBe(false);
});

it("re-pins after the transcript resizes while following, including after it was hidden", () => {
  stubResizeObserver();
  const transcriptRef = createRef<HTMLDivElement>();
  const followLatestRef = { current: true };
  render(
    view({
      snapshot,
      transcriptRef,
      followLatestRef,
      renderedItems: [message(0)],
      totalItems: 1,
      transcriptEnd: 1,
    }),
  );
  const node = transcriptRef.current!;
  const size = { height: 1000, client: 400 };
  metrics(node, size);
  // The composer grew: the transcript shrank without any scroll event.
  size.client = 300;
  act(() => observers.forEach((notify) => notify()));
  expect(node.scrollTop).toBe(700);
  // Content finished loading while an inspector hid the transcript.
  size.client = 0;
  act(() => observers.forEach((notify) => notify()));
  size.height = 1500;
  size.client = 300;
  act(() => observers.forEach((notify) => notify()));
  expect(node.scrollTop).toBe(1200);
  expect(followLatestRef.current).toBe(true);
});

it("keeps following while the composer measures its height on each keystroke", () => {
  const transcriptRef = createRef<HTMLDivElement>();
  const followLatestRef = { current: true };
  const props = {
    snapshot,
    transcriptRef,
    followLatestRef,
    renderedItems: [message(0)],
    totalItems: 1,
    transcriptEnd: 1,
  };
  const page = render(view({ ...props, stream: "a" }));
  const node = transcriptRef.current!;
  const field = screen.getByRole("textbox", { name: "Message" });
  // While the field is collapsed to "auto" the transcript is taller.
  const size = {
    height: 1000,
    get client() {
      return field.style.height === "auto" ? 440 : 400;
    },
  };
  metrics(node, size);
  page.rerender(view({ ...props, stream: "ab" }));
  expect(node.scrollTop).toBe(600);
  // Measuring the field lays the page out, which clamps the transcript.
  Object.defineProperty(field, "scrollHeight", {
    configurable: true,
    get: () => {
      node.scrollTop = node.scrollTop;
      return 40;
    },
  });
  page.rerender(view({ ...props, stream: "ab", draft: "A longer question" }));
  expect(field.style.height).toBe("40px");
  expect(node.scrollTop).toBe(600);
  fireEvent.scroll(node);
  expect(followLatestRef.current).toBe(true);
  expect(screen.queryByRole("button", { name: "↓ Latest" })).toBeNull();
});

it("pages to newer messages from their start and to earlier messages without losing the place", () => {
  const transcriptRef = createRef<HTMLDivElement>();
  const followLatestRef = { current: false };
  const onNewer = vi.fn();
  const onEarlier = vi.fn();
  const older = [message(0), message(1)];
  const newer = [message(2), message(3)];
  const props = {
    snapshot,
    transcriptRef,
    followLatestRef,
    onNewer,
    onEarlier,
  };
  const page = render(
    view({
      ...props,
      renderedItems: older,
      transcriptStart: 0,
      transcriptEnd: 2,
      totalItems: 4,
    }),
  );
  const node = transcriptRef.current!;
  const size = { height: 1000, client: 400 };
  metrics(node, size);
  node.scrollTop = 600;
  fireEvent.scroll(node);
  fireEvent.click(
    screen.getByRole("button", { name: "Show 200 newer messages" }),
  );
  expect(onNewer).toHaveBeenCalledOnce();
  page.rerender(
    view({
      ...props,
      renderedItems: newer,
      transcriptStart: 2,
      transcriptEnd: 4,
      totalItems: 4,
    }),
  );
  expect(node.scrollTop).toBe(0);
  expect(followLatestRef.current).toBe(false);
  // Earlier messages loaded above the same window keep the view in place.
  node.scrollTop = 0;
  fireEvent.click(
    screen.getByRole("button", { name: "Show earlier messages" }),
  );
  expect(onEarlier).toHaveBeenCalledOnce();
  size.height = 1600;
  page.rerender(
    view({
      ...props,
      renderedItems: [message(1), ...newer],
      transcriptStart: 1,
      transcriptEnd: 4,
      totalItems: 4,
    }),
  );
  expect(node.scrollTop).toBe(600);
});

it("does not send the Enter that confirms an input-method candidate", () => {
  onSend.mockClear();
  render(view({ draft: "研究" }));
  const field = screen.getByRole("textbox", { name: "Message" });
  // WebKit: compositionend first, then Enter with keyCode 229.
  fireEvent.keyDown(field, { key: "Enter", keyCode: 229 });
  fireEvent.compositionStart(field);
  fireEvent.compositionEnd(field);
  fireEvent.keyDown(field, { key: "Enter" });
  expect(onSend).not.toHaveBeenCalled();
});

it("does not parse saved messages again while the researcher types or a response streams", () => {
  const items = Array.from({ length: 20 }, (_, index) => message(index));
  const props = {
    snapshot,
    renderedItems: items,
    totalItems: 20,
    transcriptEnd: 20,
  };
  const page = render(view(props));
  markdown.rendered = [];
  page.rerender(view({ ...props, draft: "a" }));
  page.rerender(view({ ...props, draft: "ab", stream: "Partial" }));
  // Only the streaming bubble renders Markdown again.
  expect(
    markdown.rendered.filter((text) => text.startsWith("Message")),
  ).toEqual([]);
  expect(markdown.rendered).toContain("Partial");
  expect(screen.getAllByRole("article")).toHaveLength(21);
});

it("shows a neutral notice apart from errors", () => {
  render(view({ notice: "Response stopped." }));
  expect(screen.getByRole("status")).toHaveTextContent("Response stopped.");
  expect(screen.queryByRole("alert")).toBeNull();
});
