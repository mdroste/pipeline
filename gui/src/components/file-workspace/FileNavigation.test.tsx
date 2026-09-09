import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import ReactMarkdown from "react-markdown";
import { FileNavigationScope } from "./FileNavigation";
import { fileMarkdownComponents } from "./markdownComponents";
it("opens scoped source locations and retains loaded relative images across rerenders", async () => {
  const open = vi.fn();
  const image = vi.fn(async () => "data:image/png;base64,AA==");
  const content =
    "[Source](../code/estimate.do#L3)\n\n![Figure](../figures/plot.png)\n\n[Unavailable](../../private.txt)\n\n```stata\nlocal beta 0.96\n```";
  const body = () => (
    <FileNavigationScope value={{ path: "reports/note.md", open, image }}>
      <ReactMarkdown components={fileMarkdownComponents}>
        {content}
      </ReactMarkdown>
    </FileNavigationScope>
  );
  const { rerender, container } = render(body());
  fireEvent.click(screen.getByRole("link", { name: "Source" }));
  expect(open).toHaveBeenCalledWith({ path: "code/estimate.do", line: 3 });
  expect(screen.queryByRole("link", { name: "Unavailable" })).toBeNull();
  expect(await screen.findByRole("img", { name: "Figure" })).toHaveAttribute(
    "src",
    "data:image/png;base64,AA==",
  );
  rerender(body());
  await waitFor(() => expect(image).toHaveBeenCalledTimes(1));
  expect(container.querySelector(".hljs-keyword")).not.toBeNull();
});

it("loads an absolute image through a conversation-scoped backend", async () => {
  const image = vi.fn(async () => "data:image/png;base64,AA==");
  render(
    <FileNavigationScope
      value={{
        path: "conversation.md",
        open: vi.fn(),
        openAbsolute: vi.fn(),
        image,
      }}
    >
      <ReactMarkdown components={fileMarkdownComponents}>
        {"![Generated](/private/session/figure.png)"}
      </ReactMarkdown>
    </FileNavigationScope>,
  );
  expect(await screen.findByRole("img", { name: "Generated" })).toBeVisible();
  expect(image).toHaveBeenCalledWith("/private/session/figure.png");
});
