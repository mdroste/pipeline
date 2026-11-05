import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Theory from "./Theory";

const mocks = vi.hoisted(() => ({ call: vi.fn(), records: vi.fn() }));
vi.mock("../../lib/programClient", () => ({
  programClient: { call: mocks.call },
}));
vi.mock("../../lib/deskClient", async (original) => ({
  ...(await original<typeof import("../../lib/deskClient")>()),
  deskClient: { records: mocks.records },
}));

const symbol = {
  id: "s1",
  revision: 1,
  body: {
    notation: "\\beta",
    scope: "Section 2",
    definition: "The elasticity",
    domain: "Reals",
    units: "percent",
    aliases: [],
    sources: [],
  },
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.call.mockResolvedValue({
    objects: [],
    theory: [],
    executions: [],
    plans: [],
    profiles: [],
    checkpoints: [],
  });
  mocks.records.mockImplementation(async (_w: string, kind: string) =>
    kind === "symbol" ? [symbol] : [],
  );
});
afterEach(cleanup);

it("brings the definition form into view when revising a symbol", async () => {
  const scroll = vi.fn();
  Element.prototype.scrollIntoView = scroll;
  render(<Theory workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Revise definition" }),
  );
  const form = screen.getByText("Symbols and assumptions").closest("section");
  await waitFor(() => expect(form).toHaveFocus());
  expect(scroll.mock.contexts).toContain(form);
  expect(screen.getByLabelText("Notation")).toHaveValue("\\beta");
  delete (Element.prototype as Partial<Element>).scrollIntoView;
});
