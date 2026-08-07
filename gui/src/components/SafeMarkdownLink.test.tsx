import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import SafeMarkdownLink, { safeExternalHref } from "./SafeMarkdownLink";

const openExternal = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openExternal }));

describe("SafeMarkdownLink", () => {
  beforeEach(() => {
    openExternal.mockReset();
    openExternal.mockResolvedValue(undefined);
  });

  it.each([
    ["http", "http://example.com/paper"],
    ["https", "https://example.com/paper"],
    ["mailto", "mailto:editor@example.com"],
  ])(
    "opens an allowed %s link outside the WebView",
    (_scheme, href) => {
      render(<SafeMarkdownLink href={href}>External source</SafeMarkdownLink>);
      const link = screen.getByRole("link", { name: "External source" });
      const event = new MouseEvent("click", {
        bubbles: true,
        cancelable: true,
      });

      link.dispatchEvent(event);

      expect(event.defaultPrevented).toBe(true);
      expect(openExternal).toHaveBeenCalledOnce();
      expect(openExternal).toHaveBeenCalledWith(href);
    },
  );

  it.each([
    "javascript:alert(document.domain)",
    "file:///Users/example/private.txt",
    "custom-scheme://dangerous/action",
    "/relative/path",
    "//example.com/protocol-relative",
  ])("renders an unsafe or local URL inert: %s", (href) => {
    render(<SafeMarkdownLink href={href}>Untrusted target</SafeMarkdownLink>);

    expect(
      screen.queryByRole("link", { name: "Untrusted target" }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("Untrusted target"));
    expect(openExternal).not.toHaveBeenCalled();
  });

  it("preserves same-document fragments without invoking the shell", () => {
    render(<SafeMarkdownLink href="#footnote-1">Footnote</SafeMarkdownLink>);
    const link = screen.getByRole("link", { name: "Footnote" });

    expect(link).toHaveAttribute("href", "#footnote-1");
    fireEvent.click(link);
    expect(openExternal).not.toHaveBeenCalled();
  });

  it("reports a shell failure instead of silently dropping the click", async () => {
    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});
    openExternal.mockRejectedValueOnce(new Error("viewer unavailable"));
    render(
      <SafeMarkdownLink href="https://example.com/paper">
        External source
      </SafeMarkdownLink>,
    );

    fireEvent.click(screen.getByRole("link", { name: "External source" }));

    await waitFor(() => {
      expect(alertSpy).toHaveBeenCalledWith(
        "Could not open the external link: viewer unavailable",
      );
    });
    alertSpy.mockRestore();
  });

  it("rejects malformed absolute URLs before rendering", () => {
    expect(safeExternalHref("https://")).toBeNull();
    expect(safeExternalHref("java\nscript:alert(1)")).toBeNull();
    expect(safeExternalHref(undefined)).toBeNull();
  });
});
