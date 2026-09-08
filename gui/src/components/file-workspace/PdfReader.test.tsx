import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
const last = <T,>(values: T[]) => values[values.length - 1];
const mocks = vi.hoisted(() => ({
  getDocument: vi.fn(),
  open: vi.fn(),
  destroy: vi.fn(),
  events: [] as Array<{ name: string; value: unknown }>,
  options: [] as unknown[],
  viewers: [] as Array<{
    setDocument: ReturnType<typeof vi.fn>;
    cleanup: ReturnType<typeof vi.fn>;
    scroll: ReturnType<typeof vi.fn>;
  }>,
  links: [] as Array<{ setDocument: ReturnType<typeof vi.fn> }>,
}));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: mocks.open }));
vi.mock("pdfjs-dist/legacy/build/pdf.mjs", () => ({
  GlobalWorkerOptions: {},
  getDocument: mocks.getDocument,
}));
vi.mock("pdfjs-dist/legacy/web/pdf_viewer.mjs", () => {
  class Bus {
    listeners = new Map<string, Array<(v: unknown) => void>>();
    on(name: string, fn: (v: unknown) => void) {
      this.listeners.set(name, [...(this.listeners.get(name) ?? []), fn]);
    }
    dispatch(name: string, value: unknown) {
      mocks.events.push({ name, value });
      for (const fn of this.listeners.get(name) ?? []) fn(value);
    }
  }
  class Viewer {
    currentScaleValue = "page-width";
    pagesRotation = 0;
    pagesCount = 2;
    currentScale = 1;
    page = 1;
    bus: Bus;
    viewer: HTMLElement;
    scroll = vi.fn();
    constructor(options: {
      eventBus: Bus;
      viewer: HTMLElement;
      container: HTMLElement;
      abortSignal: AbortSignal;
    }) {
      mocks.options.push(options);
      this.bus = options.eventBus;
      this.viewer = options.viewer;
      mocks.viewers.push(this);
      options.container.addEventListener("scroll", this.scroll, {
        signal: options.abortSignal,
      });
    }
    get currentPageNumber() {
      return this.page;
    }
    set currentPageNumber(n: number) {
      this.page = n;
      this.bus.dispatch("pagechanging", {});
    }
    setDocument = vi.fn((document: unknown) => {
      if (document === null) {
        this.viewer.replaceChildren();
        return;
      }
      this.viewer.innerHTML =
        '<div class="page" data-page-number="1"><span>Selected words</span></div>';
      this.bus.dispatch("pagesinit", {});
    });
    cleanup = vi.fn();
    getPageView() {
      return {
        viewport: {
          width: 100,
          height: 200,
          viewBox: [0, 0, 100, 200],
          convertToPdfPoint: (x: number, y: number) => [x, 200 - y],
        },
      };
    }
  }
  return {
    EventBus: Bus,
    PDFViewer: Viewer,
    PDFLinkService: class {
      constructor() {
        mocks.links.push(this);
      }
      setViewer() {}
      setDocument = vi.fn();
      goToDestination() {
        return Promise.resolve();
      }
    },
    PDFFindController: class {},
  };
});
import PdfReader from "./PdfReader";
beforeEach(() => {
  vi.clearAllMocks();
  mocks.open.mockResolvedValue(undefined);
  mocks.events = [];
  mocks.options = [];
  mocks.viewers = [];
  mocks.links = [];
  mocks.destroy.mockResolvedValue(undefined);
  localStorage.clear();
  mocks.getDocument.mockReturnValue({
    promise: Promise.resolve({
      numPages: 2,
      getPageLabels: async () => ["i", "1"],
      getOutline: async () => [
        { title: "Introduction", dest: "intro", items: [] },
      ],
    }),
    destroy: mocks.destroy,
  });
});
it("loads only supplied bytes with text and annotation layers, and routes external links", async () => {
  const { container, unmount } = render(
    <PdfReader
      documentKey="pdf"
      title="Paper"
      load={async () => "JVBERi0xLjQ="}
    />,
  );
  await screen.findByText("of 2");
  expect(mocks.getDocument).toHaveBeenCalledWith(
    expect.objectContaining({
      data: expect.any(Uint8Array),
      cMapUrl: "/pdf-assets/cmaps/",
    }),
  );
  expect(mocks.options[0]).toMatchObject({
    imageResourcesPath: "/pdf-assets/images/",
    textLayerMode: 1,
    annotationMode: 1,
  });
  fireEvent.click(screen.getByLabelText("Next PDF page"));
  expect(screen.getByLabelText("PDF page number")).toHaveValue("1");
  fireEvent.change(screen.getByLabelText("Find in PDF"), {
    target: { value: "Euler" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Find" }));
  expect(mocks.events).toContainEqual({
    name: "find",
    value: expect.objectContaining({ query: "Euler", highlightAll: true }),
  });
  const link = document.createElement("a");
  link.href = "https://example.org/paper";
  link.textContent = "Reference";
  container.querySelector(".pdfViewer")!.append(link);
  fireEvent.click(link);
  expect(mocks.open).toHaveBeenCalledWith("https://example.org/paper");
  link.href = "file:///etc/passwd";
  fireEvent.click(link);
  expect(mocks.open).toHaveBeenCalledTimes(1);
  unmount();
  expect(mocks.destroy).toHaveBeenCalled();
});
it("offers the existing page-image fallback when PDF bytes cannot load", async () => {
  render(
    <PdfReader
      documentKey="missing"
      title="Paper"
      load={async () => {
        throw new Error("Missing captured PDF");
      }}
      fallback={<div>Retained page image</div>}
    />,
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Missing captured PDF",
  );
  fireEvent.click(screen.getByRole("button", { name: "Use page images" }));
  expect(screen.getByText("Retained page image")).toBeVisible();
});

it("aborts replaced viewers, clears document references, and detaches scroll listeners", async () => {
  const load = async () => "JVBERg==";
  const { rerender, container, unmount } = render(
    <PdfReader documentKey="first" title="Paper" load={load} />,
  );
  await screen.findByText("of 2");
  const scrollContainer = container.querySelector(".pdfViewer")!.parentElement!;
  for (const key of ["second", "third"]) {
    const previous = last(mocks.viewers)!;
    const previousLinks = last(mocks.links)!;
    const signal = (last(mocks.options) as { abortSignal: AbortSignal })
      .abortSignal;
    expect(signal.aborted).toBe(false);
    fireEvent.scroll(scrollContainer);
    const calls = previous.scroll.mock.calls.length;
    rerender(<PdfReader documentKey={key} title="Paper" load={load} />);
    await waitFor(() => expect(last(mocks.viewers)).not.toBe(previous));
    await screen.findByText("of 2");
    expect(signal.aborted).toBe(true);
    expect(previous.setDocument).toHaveBeenLastCalledWith(null);
    expect(previousLinks.setDocument).toHaveBeenLastCalledWith(null);
    expect(previous.cleanup).toHaveBeenCalledOnce();
    fireEvent.scroll(scrollContainer);
    expect(previous.scroll).toHaveBeenCalledTimes(calls);
    expect(last(mocks.viewers)!.scroll).toHaveBeenCalledOnce();
  }
  mocks.destroy.mockRejectedValueOnce(new Error("Worker already stopped"));
  unmount();
  await act(async () => {});
  for (const options of mocks.options)
    expect((options as { abortSignal: AbortSignal }).abortSignal.aborted).toBe(
      true,
    );
  expect(last(mocks.viewers)!.setDocument).toHaveBeenLastCalledWith(null);
  expect(last(mocks.links)!.setDocument).toHaveBeenLastCalledWith(null);
  expect(mocks.destroy).toHaveBeenCalledTimes(3);
});

it("cleans up failed viewers on retry and when switching to fallback", async () => {
  mocks.getDocument.mockReturnValue({
    promise: Promise.reject(new Error("Invalid PDF")),
    destroy: mocks.destroy,
  });
  render(
    <PdfReader
      documentKey="broken"
      title="Paper"
      load={async () => "JVBERg=="}
      fallback={<div>Fallback pages</div>}
    />,
  );
  await screen.findByRole("alert");
  mocks.getDocument.mockReturnValue({
    promise: Promise.reject(new Error("Still invalid")),
    destroy: mocks.destroy,
  });
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(mocks.options).toHaveLength(2));
  expect(
    (mocks.options[0] as { abortSignal: AbortSignal }).abortSignal.aborted,
  ).toBe(true);
  await screen.findByText(/Still invalid/);
  fireEvent.click(screen.getByRole("button", { name: "Use page images" }));
  expect(screen.getByText("Fallback pages")).toBeVisible();
  expect(
    (mocks.options[1] as { abortSignal: AbortSignal }).abortSignal.aborted,
  ).toBe(true);
  expect(
    mocks.links.every(
      (link) => last(link.setDocument.mock.calls)?.[0] === null,
    ),
  ).toBe(true);
  expect(mocks.destroy).toHaveBeenCalledTimes(2);
});
it("attaches selected text with its page and normalized PDF region", async () => {
  const selected = vi.fn();
  const { container } = render(
    <PdfReader
      documentKey="selection"
      title="Paper"
      load={async () => "JVBERg=="}
      onSelection={selected}
    />,
  );
  await screen.findByText("Selected words");
  const page = container.querySelector<HTMLElement>(".page")!;
  page.getBoundingClientRect = () => new DOMRect(0, 0, 100, 200);
  const range = document.createRange();
  range.selectNodeContents(page.querySelector("span")!);
  range.getBoundingClientRect = () => new DOMRect(10, 20, 20, 20);
  window.getSelection()!.removeAllRanges();
  window.getSelection()!.addRange(range);
  await act(async () => fireEvent.pointerUp(page));
  await waitFor(() =>
    expect(selected).toHaveBeenCalledWith({
      page: 1,
      quote: "Selected words",
      region: [0.1, 0.1, 0.2, 0.1],
    }),
  );
});
