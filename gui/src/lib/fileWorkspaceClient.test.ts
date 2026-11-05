import { afterEach, describe, expect, it, vi } from "vitest";
import { artifactFileAdapter, fetchRange } from "./fileWorkspaceClient";

const content = vi.hoisted(() => ({ value: null as unknown }));
const stream = vi.hoisted(() => vi.fn());
const pdfBytes = vi.hoisted(() => vi.fn());
vi.mock("./artifactClient", () => ({
  artifactClient: {
    read: async () => content.value,
    stream,
    pdfBytes,
  },
}));

afterEach(() => vi.unstubAllGlobals());
const part = (bytes: number[], status = 206) =>
  new Response(new Uint8Array(bytes), { status });

describe("fetchRange", () => {
  it("continues when the server answers a long range in parts", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(part([1, 2]))
      .mockResolvedValueOnce(part([3, 4]));
    vi.stubGlobal("fetch", fetchMock);
    expect(await fetchRange("url", 10, 14)).toEqual(
      new Uint8Array([1, 2, 3, 4]),
    );
    expect(fetchMock.mock.calls[1][1].headers.Range).toBe("bytes=12-13");
  });

  it("stops at the end of a file shorter than requested", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValueOnce(part([1]))
        .mockResolvedValueOnce(part([])),
    );
    expect(await fetchRange("url", 0, 4)).toEqual(new Uint8Array([1]));
  });

  it("explains a file that changed under the reader", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(part([], 409)));
    await expect(fetchRange("url", 0, 4)).rejects.toThrow("changed on disk");
  });
});

describe("run artifacts", () => {
  const artifact = (kind: string, text: string | null = null) => ({
    kind,
    bytes: 10,
    text,
    base64: kind === "image" ? "QUJD" : null,
    truncated: false,
    abs_path: "/runs/x/a",
  });

  it("streams retained PDFs and refuses paths outside the manifest", async () => {
    content.value = artifact("pdf");
    stream.mockResolvedValue("token");
    const adapter = artifactFileAdapter("run", ["a.pdf"]);
    const file = await adapter.read("a.pdf");
    expect(file.streamToken).toBe("token");
    expect(pdfBytes).not.toHaveBeenCalled();
    await expect(adapter.read("other.pdf")).rejects.toThrow("manifest");
  });

  it("falls back to inline bytes when no stream is available", async () => {
    content.value = artifact("pdf");
    stream.mockRejectedValue(new Error("Not retained"));
    pdfBytes.mockResolvedValue("JVBERg==");
    const file = await artifactFileAdapter("run", ["a.pdf"]).read("a.pdf");
    expect(file.streamToken).toBeNull();
    expect(file.base64).toBe("JVBERg==");
  });
});
