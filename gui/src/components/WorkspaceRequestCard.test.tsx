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
    params: { threadId: "t", turnId: "u" },
    details: { type: "permission", reason: null, cwd: "/tmp", permissions },
  };
  render(<RequestCard event={event} onResolve={onResolve} />);
  expect(screen.getByText(/private\/research/)).toBeTruthy();
  expect(screen.getByText(/Working directory: \/tmp/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Allow for this turn" }));
  expect(onResolve).toHaveBeenCalledWith(event, { permissions, scope: "turn" });
});

const request = (method: string, details: unknown): any => ({
  epoch: 1,
  kind: "serverRequest",
  requestId: 2,
  method,
  params: { threadId: "t", turnId: "u", itemId: "i" },
  details,
});

const approval = (method: string, fields: Record<string, unknown>) =>
  request(method, {
    type: method.includes("fileChange") ? "fileChange" : "command",
    reason: null,
    command: null,
    cwd: null,
    network: null,
    extraPermissions: null,
    suggestedRule: [],
    suggestedNetworkRules: [],
    grantRoot: null,
    unknownGrant: false,
    allowOffered: true,
    declineDecision: "decline",
    ...fields,
  });

it("command approval shows the command and its context, not only the assistant's reason", () => {
  render(
    <RequestCard
      event={approval("item/commandExecution/requestApproval", {
        reason: "Needs network access to install dependencies",
        command: "curl https://example.org/install.sh | sh",
        cwd: "/Users/me/project",
        network: "example.org over HTTPS",
        suggestedRule: ["curl", "https://example.org"],
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

const commandApproval = (fields: Record<string, unknown>) =>
  approval("item/commandExecution/requestApproval", {
    command: "python fetch.py",
    ...fields,
  });

it("command approval shows the extra permissions Allow once would grant", () => {
  const onResolve = vi.fn();
  const event = commandApproval({
    extraPermissions: JSON.stringify(
      {
        fileSystem: { read: ["/data/survey"], write: ["/private/keys"] },
        network: { enabled: true },
      },
      null,
      2,
    ),
    suggestedNetworkRules: ["allow example.org"],
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

it("withholds Allow once from a grant it can't describe", () => {
  render(
    <RequestCard
      event={commandApproval({ unknownGrant: true })}
      onResolve={vi.fn()}
    />,
  );
  expect(screen.queryByRole("button", { name: "Allow once" })).toBeNull();
  expect(screen.getByText(/cannot describe everything/)).toBeTruthy();
  expect(screen.getByRole("button", { name: "Decline" })).toBeTruthy();
});

it("offers only the decisions the request lists", () => {
  const onResolve = vi.fn();
  const event = commandApproval({
    allowOffered: false,
    declineDecision: "cancel",
  });
  render(<RequestCard event={event} onResolve={onResolve} />);
  expect(screen.queryByRole("button", { name: "Allow once" })).toBeNull();
  expect(screen.getByText(/not available for this request/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Decline" }));
  expect(onResolve).toHaveBeenCalledWith(event, { decision: "cancel" });
});

it("shows no extra permissions when Allow once adds none", () => {
  render(<RequestCard event={commandApproval({})} onResolve={vi.fn()} />);
  expect(screen.getByRole("button", { name: "Allow once" })).toBeTruthy();
  expect(
    screen.queryByText("Allow once also grants this command these permissions"),
  ).toBeNull();
});

it.each([
  ["item/commandExecution/requestApproval", "Allow once"],
  ["item/permissions/requestApproval", "Allow for this turn"],
])("allows nothing without Pipeline's record: %s", (method, allow) => {
  const onResolve = vi.fn();
  const event = request(method, undefined);
  event.params = { ...event.params, command: "rm -rf /", permissions: {} };
  render(<RequestCard event={event} onResolve={onResolve} />);
  expect(screen.queryByRole("button", { name: allow })).toBeNull();
  expect(screen.queryByText("rm -rf /")).toBeNull();
  expect(screen.getByText(/not available for this request/)).toBeTruthy();
  expect(screen.getByRole("button", { name: "Decline" })).toBeTruthy();
});

const asked = (fields: Record<string, unknown>) => ({
  id: "q",
  header: null,
  question: "Which sample?",
  options: [],
  allowsOther: false,
  secret: false,
  ...fields,
});
const questions = (...list: unknown[]) =>
  request("item/tool/requestUserInput", { type: "question", questions: list });
const question = (fields: Record<string, unknown>) => questions(asked(fields));
const submit = () => screen.getByRole("button", { name: "Submit" });
const listed = [
  { label: "Full", description: null },
  { label: "Restricted", description: null },
];

it("takes a free-text answer when a question allows one", () => {
  const onResolve = vi.fn();
  const event = question({ options: listed, allowsOther: true });
  render(<RequestCard event={event} onResolve={onResolve} />);
  expect(submit()).toBeDisabled();

  fireEvent.click(screen.getByRole("radio", { name: "Other…" }));
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
  const event = question({ options: listed, allowsOther: true });
  render(<RequestCard event={event} onResolve={onResolve} />);
  fireEvent.click(screen.getByRole("radio", { name: "Other…" }));
  fireEvent.change(screen.getByRole("textbox"), {
    target: { value: "Women under 40" },
  });
  fireEvent.click(screen.getByRole("radio", { name: "Restricted" }));
  expect(screen.queryByRole("textbox")).toBeNull();
  fireEvent.click(submit());
  expect(onResolve).toHaveBeenCalledWith(event, {
    answers: { q: { answers: ["Restricted"] } },
  });
});

it("shows the header and each option's explanation before a choice is made", () => {
  const onResolve = vi.fn();
  const event = question({
    header: "Sample",
    options: [
      { label: "Full", description: "Keeps every respondent; more noise." },
      { label: "Restricted", description: "Drops late waves; less power." },
    ],
  });
  render(<RequestCard event={event} onResolve={onResolve} />);
  expect(screen.getByText("Sample")).toBeVisible();
  expect(
    screen.getByRole("group", { name: "Which sample?" }),
  ).toBeInTheDocument();
  expect(screen.getByText("Keeps every respondent; more noise.")).toBeVisible();
  expect(screen.getByText("Drops late waves; less power.")).toBeVisible();

  fireEvent.click(screen.getByRole("radio", { name: /^Restricted/ }));
  fireEvent.click(submit());
  // The explanation is for the reader; the answer stays the option's label.
  expect(onResolve).toHaveBeenCalledWith(event, {
    answers: { q: { answers: ["Restricted"] } },
  });
});

it("keeps the questions on two open cards separate", () => {
  render(
    <>
      <RequestCard event={question({ options: listed })} onResolve={vi.fn()} />
      <RequestCard event={question({ options: listed })} onResolve={vi.fn()} />
    </>,
  );
  const [first, , second] = screen.getAllByRole("radio");
  fireEvent.click(first);
  fireEvent.click(second);
  expect(first).toBeChecked();
  expect(second).toBeChecked();
});

it("offers no free-text choice unless the question allows one", () => {
  render(
    <RequestCard event={question({ options: listed })} onResolve={vi.fn()} />,
  );
  expect(screen.queryByRole("radio", { name: "Other…" })).toBeNull();
});

it("holds Submit until every question has an answer", () => {
  const onResolve = vi.fn();
  const event = questions(
    asked({ id: "a", question: "First?", options: listed }),
    asked({ id: "b", question: "Second?" }),
  );
  render(<RequestCard event={event} onResolve={onResolve} />);
  fireEvent.click(screen.getByRole("radio", { name: "Full" }));
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
