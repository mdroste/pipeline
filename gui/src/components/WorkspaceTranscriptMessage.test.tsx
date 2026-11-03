import { fireEvent, render } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import TranscriptMessage, {
  TranscriptMarkdown,
} from "./WorkspaceTranscriptMessage";

const footnoted = (id: string) => (
  <TranscriptMessage
    id={id}
    text={"A claim[^1]\n\n[^1]: The source."}
    role="ChatGPT"
    selected={false}
  />
);

it("keeps each message's footnotes to itself", () => {
  const scroll = vi.fn();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: scroll,
  });
  const { container } = render(
    <>
      {footnoted("first")}
      {footnoted("second")}
    </>,
  );
  const ids = [...container.querySelectorAll("[id*='fn-1']")].map(
    (element) => element.id,
  );
  expect(ids.length).toBeGreaterThanOrEqual(2);
  expect(new Set(ids).size).toBe(ids.length);

  // The note in view is this message's, and the app's route stays in the hash.
  const hash = window.location.hash;
  const second = container.querySelector("#workspace-message-second")!;
  fireEvent.click(second.querySelector("a[data-footnote-ref]")!);
  expect(scroll).toHaveBeenCalledOnce();
  expect(second).toContainElement(scroll.mock.contexts[0] as HTMLElement);
  expect(window.location.hash).toBe(hash);
});

it("renders each math form and leaves dollar amounts as text", () => {
  const math = render(
    <TranscriptMarkdown
      text={"Inline \\( \\beta_1 \\) and $x_i$.\n\n\\[ y = \\E[x] \\]"}
    />,
  );
  expect(math.container.querySelectorAll(".katex")).toHaveLength(3);
  const money = render(
    <TranscriptMarkdown text="Costs were $5 to $8 per unit." />,
  );
  expect(money.container.querySelector(".katex")).toBeNull();
  expect(money.container.textContent).toBe("Costs were $5 to $8 per unit.");
});
