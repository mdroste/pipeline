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
