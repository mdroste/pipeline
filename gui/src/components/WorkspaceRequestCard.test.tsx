import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { it, expect, vi, afterEach } from "vitest";
import RequestCard from "./WorkspaceRequestCard";
afterEach(cleanup);
it("permission review shows the complete scope and duration", () => {
  const onResolve = vi.fn();
  const permissions = {
    fileSystem: { write: ["/private/research"] },
    network: { enabled: true },
  };
  const event: any = {
    epoch: 1,
    kind: "serverRequest",
    requestId: 1,
    method: "item/permissions/requestApproval",
    params: { threadId: "t", turnId: "u", cwd: "/tmp", permissions },
  };
  render(<RequestCard event={event} onResolve={onResolve} />);
  expect(screen.getByText(/private\/research/)).toBeTruthy();
  expect(screen.getByText(/Working directory: \/tmp/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Allow for this turn" }));
  expect(onResolve).toHaveBeenCalledWith(event, { permissions, scope: "turn" });
});
