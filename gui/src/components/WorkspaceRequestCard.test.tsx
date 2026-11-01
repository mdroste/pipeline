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

const commandApproval = (params: Record<string, unknown>) =>
  approval("item/commandExecution/requestApproval", {
    command: "python fetch.py",
    ...params,
  });

it("command approval shows the extra permissions Allow once would grant", () => {
  const onResolve = vi.fn();
  const event = commandApproval({
    additionalPermissions: {
      fileSystem: { read: ["/data/survey"], write: ["/private/keys"] },
      network: { enabled: true },
    },
    proposedNetworkPolicyAmendments: [{ action: "allow", host: "example.org" }],
  });
  render(<RequestCard event={event} onResolve={onResolve} />);
  expect(
    screen.getByText("Allow once also grants this command these permissions"),
  ).toBeTruthy();
  expect(screen.getByText(/\/private\/keys/)).toBeTruthy();
  expect(screen.getByText(/\/data\/survey/)).toBeTruthy();
  expect(screen.getByText(/"enabled": true/)).toBeTruthy();
  expect(screen.getByText("allow example.org")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Allow once" }));
  expect(onResolve).toHaveBeenCalledWith(event, { decision: "accept" });
});

it.each([
  [{ macos: { accessibility: true } }],
  [{ fileSystem: { execute: ["/usr/bin"] } }],
  [{ network: "everywhere" }],
])("withholds Allow once from a grant it can't describe: %j", (permissions) => {
  render(
    <RequestCard
      event={commandApproval({ additionalPermissions: permissions })}
      onResolve={vi.fn()}
    />,
  );
  expect(screen.queryByRole("button", { name: "Allow once" })).toBeNull();
  expect(screen.getByText(/cannot describe everything/)).toBeTruthy();
  expect(screen.getByRole("button", { name: "Decline" })).toBeTruthy();
});

it("offers only the decisions the request lists", () => {
  const onResolve = vi.fn();
  const event = commandApproval({ availableDecisions: ["cancel"] });
  render(<RequestCard event={event} onResolve={onResolve} />);
  expect(screen.queryByRole("button", { name: "Allow once" })).toBeNull();
  expect(screen.getByText(/not available for this request/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Decline" }));
  expect(onResolve).toHaveBeenCalledWith(event, { decision: "cancel" });
});

it("keeps Allow once when the request lists it among richer decisions", () => {
  render(
    <RequestCard
      event={commandApproval({
        additionalPermissions: null,
        availableDecisions: [
          "accept",
          { acceptWithExecpolicyAmendment: { execpolicy_amendment: ["ls"] } },
          "decline",
        ],
      })}
      onResolve={vi.fn()}
    />,
  );
  expect(screen.getByRole("button", { name: "Allow once" })).toBeTruthy();
  expect(
    screen.queryByText("Allow once also grants this command these permissions"),
  ).toBeNull();
});

const question = (fields: Record<string, unknown>) =>
  approval("item/tool/requestUserInput", {
    questions: [{ id: "q", question: "Which sample?", ...fields }],
  });
const submit = () => screen.getByRole("button", { name: "Submit" });
const listed = [{ label: "Full" }, { label: "Restricted" }];

it("takes a free-text answer when a question allows one", () => {
  const onResolve = vi.fn();
  const event = question({ options: listed, isOther: true });
  render(<RequestCard event={event} onResolve={onResolve} />);
  expect(submit()).toBeDisabled();

  fireEvent.change(screen.getByRole("combobox"), {
    target: { value: "other" },
  });
  const field = screen.getByRole("textbox", {
    name: "Your answer to: Which sample?",
  });
  expect(submit()).toBeDisabled();
  fireEvent.change(field, { target: { value: "   " } });
  expect(submit()).toBeDisabled();
  fireEvent.change(field, { target: { value: "Women under 40" } });
  expect(submit()).toBeEnabled();
  fireEvent.click(submit());
  expect(onResolve).toHaveBeenCalledWith(event, {
    answers: { q: { answers: ["Women under 40"] } },
  });
});

it("sends the option chosen last, not words typed before changing course", () => {
  const onResolve = vi.fn();
  const event = question({ options: listed, isOther: true });
  render(<RequestCard event={event} onResolve={onResolve} />);
  fireEvent.change(screen.getByRole("combobox"), {
    target: { value: "other" },
  });
  fireEvent.change(screen.getByRole("textbox"), {
    target: { value: "Women under 40" },
  });
  fireEvent.change(screen.getByRole("combobox"), { target: { value: "1" } });
  expect(screen.queryByRole("textbox")).toBeNull();
  fireEvent.click(submit());
  expect(onResolve).toHaveBeenCalledWith(event, {
    answers: { q: { answers: ["Restricted"] } },
  });
});

it("offers no free-text choice unless the question allows one", () => {
  render(
    <RequestCard event={question({ options: listed })} onResolve={vi.fn()} />,
  );
  expect(screen.queryByRole("option", { name: "Other…" })).toBeNull();
});

it("holds Submit until every question has an answer", () => {
  const onResolve = vi.fn();
  const event = approval("item/tool/requestUserInput", {
    questions: [
      { id: "a", question: "First?", options: listed },
      { id: "b", question: "Second?" },
    ],
  });
  render(<RequestCard event={event} onResolve={onResolve} />);
  fireEvent.change(screen.getByRole("combobox"), { target: { value: "0" } });
  expect(submit()).toBeDisabled();
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "Yes" } });
  fireEvent.click(submit());
  expect(onResolve).toHaveBeenCalledWith(event, {
    answers: { a: { answers: ["Full"] }, b: { answers: ["Yes"] } },
  });
});

it("can still decline a question left unanswered", () => {
  const onResolve = vi.fn();
  const event = question({ options: listed });
  render(<RequestCard event={event} onResolve={onResolve} />);
  fireEvent.click(screen.getByRole("button", { name: "Decline" }));
  expect(onResolve).toHaveBeenCalledWith(event, undefined);
});
