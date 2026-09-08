import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PdfPosition } from "./PdfReader";
const mocks = vi.hoisted(() => ({
  getDocument: vi.fn(),
  destroy: vi.fn(),
  sizes: [] as number[][],
}));
vi.mock("pdfjs-dist/legacy/build/pdf.mjs", () => ({
  getDocument: mocks.getDocument,
}));
vi.mock("../../lib/fileWorkspaceClient", () => ({
  workspaceFileAdapter: ({ revisionId }: { revisionId: string }) => ({
    read: async () => ({ base64: revisionId }),
  }),
}));
vi.mock("./PdfReader", () => ({
  decodePdf: (value: string) => value,
  default: ({
    title,
    position,
    onPosition,
  }: {
    title: string;
    position?: PdfPosition;
    onPosition?: (p: PdfPosition) => void;
  }) => (
    <div>
      <output aria-label={title}>
        {position
          ? `${position.page}/${position.scale}/${position.rotation}`
          : "independent"}
      </output>
      <button
        onClick={() => onPosition?.({ page: 2, scale: "1.5", rotation: 90 })}
      >
        Navigate {title}
      </button>
    </div>
  ),
}));
import PdfComparison from "./PdfComparison";
beforeEach(() => {
  vi.clearAllMocks();
  mocks.sizes = [];
  vi.stubGlobal("crypto", {
    subtle: {
      digest: async (_algorithm: string, pixels: Uint8ClampedArray) =>
        pixels.slice().buffer,
    },
  });
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(
    function (this: HTMLCanvasElement) {
      return {
        getImageData: () => ({
          data: new Uint8ClampedArray([Number(this.dataset.pixel), 0, 0, 255]),
        }),
      } as unknown as CanvasRenderingContext2D;
    },
  );
  mocks.getDocument.mockImplementation(({ data }: { data: string }) => ({
    promise: Promise.resolve({
      numPages: 2,
      getPage: async (page: number) => ({
        getViewport: ({ scale }: { scale: number }) => ({
          width: 100 * scale,
          height: 10000 * scale,
        }),
        render: ({ canvas }: { canvas: HTMLCanvasElement }) => {
          mocks.sizes.push([canvas.width, canvas.height]);
          canvas.dataset.pixel = String(
            page === 2 && data === "after" ? 3 : page,
          );
          return { promise: Promise.resolve() };
        },
        cleanup: vi.fn(),
      }),
    }),
    destroy: mocks.destroy,
  }));
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
it("links navigation and finds changed pages with bounded canvases", async () => {
  render(
    <PdfComparison
      workspaceId="w"
      primary={{ id: "before", title: "Before" }}
      secondary={{ id: "after", title: "After" }}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Navigate Before" }));
  expect(screen.getByLabelText("After")).toHaveTextContent("2/1.5/90");
  fireEvent.click(screen.getByRole("button", { name: "Find changed pages" }));
  await screen.findByText(/2 pages at thumbnail resolution; 1 pages differ/);
  expect(mocks.sizes).toHaveLength(4);
  for (const [width, height] of mocks.sizes) {
    expect(width).toBeLessThanOrEqual(320);
    expect(height).toBeLessThanOrEqual(480);
  }
  fireEvent.click(screen.getByRole("button", { name: "Next changed page" }));
  expect(screen.getByLabelText("Before")).toHaveTextContent("2/");
  await waitFor(() => expect(mocks.destroy).toHaveBeenCalledTimes(2));
  fireEvent.click(screen.getByRole("checkbox"));
  expect(screen.getByLabelText("After")).toHaveTextContent("independent");
});
it("refuses an oversized comparison without reporting a partial scan as complete", async () => {
  mocks.getDocument.mockReturnValue({
    promise: Promise.resolve({ numPages: 5001 }),
    destroy: mocks.destroy,
  });
  render(
    <PdfComparison
      workspaceId="w"
      primary={{ id: "before", title: "Before" }}
      secondary={{ id: "after", title: "After" }}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Find changed pages" }));
  await screen.findByText(/at most 5,000 pages/);
  expect(mocks.sizes).toHaveLength(0);
  await waitFor(() => expect(mocks.destroy).toHaveBeenCalledTimes(2));
});
