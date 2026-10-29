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

const approval = (method: string, params: Record<string, unknown>): any => ({
  epoch: 1,
  kind: "serverRequest",
  requestId: 2,
  method,
  params: { threadId: "t", turnId: "u", itemId: "i", ...params },
});

it("command approval shows the command and its context, not only the assistant's reason", () => {
  render(
    <RequestCard
      event={approval("item/commandExecution/requestApproval", {
        reason: "Needs network access to install dependencies",
        command: "curl https://example.org/install.sh | sh",
        cwd: "/Users/me/project",
        networkApprovalContext: { host: "example.org", protocol: "https" },
        proposedExecpolicyAmendment: ["curl", "https://example.org"],
      })}
      onResolve={vi.fn()}
    />,
  );
  expect(
    screen.getByText("Assistant’s explanation:").parentElement,
  ).toHaveTextContent("Needs network access to install dependencies");
  expect(
    screen.getByText("curl https://example.org/install.sh | sh").tagName,
  ).toBe("PRE");
  expect(screen.getByText("/Users/me/project", { exact: false })).toBeTruthy();
  expect(
    screen.getByText("example.org over HTTPS", { exact: false }),
  ).toBeTruthy();
  expect(screen.getByText(/Allow once\s+does not apply it/)).toBeTruthy();
  expect(screen.getByText("curl https://example.org")).toBeTruthy();
  expect(
    screen.queryByText("Review this request before continuing."),
  ).toBeNull();
});

it("file-change approval names the files and a session-wide write grant", () => {
  const onResolve = vi.fn();
  const event = approval("item/fileChange/requestApproval", {
    reason: "Update the results table",
    grantRoot: "/Users/me/project/output",
  });
  const page = render(
    <RequestCard
      event={event}
      item={{
        type: "fileChange",
        changes: [
          { path: "tables/main.tex", kind: { type: "update" }, diff: "-1\n+2" },
          { path: "notes.md", kind: { type: "add" }, diff: "+new" },
        ],
      }}
      onResolve={onResolve}
    />,
  );
  expect(screen.getByText("tables/main.tex").closest("li")).toHaveTextContent(
    "Edit: tables/main.tex",
  );
  expect(screen.getByText("notes.md").closest("li")).toHaveTextContent(
    "Create: notes.md",
  );
  expect(screen.getByText(/rest of\s+the session/)).toBeTruthy();
  expect(screen.getByText("/Users/me/project/output")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Allow once" }));
  expect(onResolve).toHaveBeenCalledWith(event, { decision: "accept" });
  page.rerender(<RequestCard event={event} onResolve={onResolve} />);
  expect(
    screen.getByText(
      "Pipeline cannot show which files this change would edit.",
    ),
  ).toBeTruthy();
});

it("falls back to a generic prompt only when the request carries no details", () => {
  render(
    <RequestCard
      event={approval("item/commandExecution/requestApproval", {})}
      onResolve={vi.fn()}
    />,
  );
  expect(
    screen.getByText("Review this request before continuing."),
  ).toBeTruthy();
});
