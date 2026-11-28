import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import {
  defaultProjectEnvironment,
  ProjectEnvironment,
} from "../../lib/projectEnvironment";
import type { WorkspaceConfig } from "../../lib/workbench/harnessTypes";
import ProjectAssistantAccess, {
  FIELD_CONVENTIONS_MAX_BYTES,
} from "./ProjectAssistantAccess";

afterEach(cleanup);

const config = (
  workspaceId: string | null,
  body: Record<string, unknown>,
  revision = 1,
): WorkspaceConfig => ({
  scopeKey: workspaceId ?? "global",
  workspaceId,
  schemaVersion: 1,
  body,
  revision,
  updatedAt: "now",
});

function mount(
  project: Record<string, unknown>,
  global: Record<string, unknown>,
) {
  const harnessClient = {
    getWorkspaceConfig: vi.fn(async (id: string | null) =>
      id ? config(id, project, 4) : config(null, global),
    ),
    saveWorkspaceConfig: vi.fn(async ({ workspaceId, body }) =>
      config(workspaceId, body, 5),
    ),
  };
  const reportError = vi.fn();
  render(
    <ProjectEnvironment.Provider
      value={{ ...defaultProjectEnvironment, harnessClient }}
    >
      <ProjectAssistantAccess workspaceId="ws" reportError={reportError} />
    </ProjectEnvironment.Provider>,
  );
  return { harnessClient, reportError };
}

it("shows the inherited access until the project chooses its own", async () => {
  const { harnessClient } = mount({ webSearch: true }, {});
  const readOnly = screen.getByRole("radio", { name: /Read only/ });
  await waitFor(() => expect(readOnly).toBeEnabled());
  expect(readOnly).toBeChecked();
  fireEvent.click(screen.getByRole("radio", { name: /Allow edits/ }));
  await waitFor(() =>
    expect(harnessClient.saveWorkspaceConfig).toHaveBeenCalledWith(
      expect.objectContaining({
        workspaceId: "ws",
        expectedRevision: 4,
        body: { webSearch: true, mode: "edit" },
      }),
    ),
  );
  expect(
    await screen.findByRole("radio", { name: /Allow edits/ }),
  ).toBeChecked();
  expect(screen.getByRole("note")).toHaveTextContent(
    "Make sure this folder is backed up",
  );
});

it("shows the project's own choice over the global default", async () => {
  mount({ mode: "edit" }, { mode: "inspect" });
  expect(await screen.findByRole("note")).toHaveTextContent(
    "undo any changes an assistant makes to code or files",
  );
  expect(screen.getByRole("radio", { name: /Allow edits/ })).toBeChecked();
});

const conventionsField = () =>
  screen.getByRole("textbox", { name: "Field and conventions" });
const saveConventions = () =>
  screen.getByRole("button", { name: "Save field and conventions" });

it("saves the field and conventions with the project's other settings", async () => {
  const { harnessClient } = mount(
    { mode: "edit", fieldConventions: "Economics." },
    {},
  );
  await waitFor(() => expect(conventionsField()).toHaveValue("Economics."));
  expect(conventionsField()).toHaveAccessibleDescription(/every connection/);
  expect(saveConventions()).toBeDisabled();
  fireEvent.change(conventionsField(), {
    target: {
      value: "  Economics. Cluster standard errors as in the paper.\n",
    },
  });
  fireEvent.click(saveConventions());
  await waitFor(() =>
    expect(harnessClient.saveWorkspaceConfig).toHaveBeenCalledWith(
      expect.objectContaining({
        workspaceId: "ws",
        expectedRevision: 4,
        body: {
          mode: "edit",
          fieldConventions:
            "Economics. Cluster standard errors as in the paper.",
        },
      }),
    ),
  );
  await waitFor(() => expect(saveConventions()).toBeDisabled());
  expect(screen.getByRole("radio", { name: /Allow edits/ })).toBeChecked();
});

it("removes the field and conventions when cleared", async () => {
  const { harnessClient } = mount(
    { webSearch: true, fieldConventions: "Economics." },
    {},
  );
  await waitFor(() => expect(conventionsField()).toHaveValue("Economics."));
  fireEvent.change(conventionsField(), { target: { value: " \n " } });
  fireEvent.click(saveConventions());
  await waitFor(() =>
    expect(harnessClient.saveWorkspaceConfig).toHaveBeenCalledWith(
      expect.objectContaining({ body: { webSearch: true } }),
    ),
  );
});

it("keeps the field and conventions within 4 KiB of text", async () => {
  const { harnessClient } = mount({}, {});
  await waitFor(() => expect(conventionsField()).toBeEnabled());
  // Two bytes each: within the character count, over the byte limit.
  fireEvent.change(conventionsField(), {
    target: { value: "é".repeat(FIELD_CONVENTIONS_MAX_BYTES / 2 + 1) },
  });
  expect(screen.getByRole("alert")).toHaveTextContent("4 KiB");
  expect(conventionsField()).toHaveAttribute("aria-invalid", "true");
  expect(saveConventions()).toBeDisabled();
  fireEvent.change(conventionsField(), {
    target: { value: "é".repeat(FIELD_CONVENTIONS_MAX_BYTES / 2) },
  });
  expect(screen.queryByRole("alert")).toBeNull();
  expect(saveConventions()).toBeEnabled();
  fireEvent.click(saveConventions());
  await waitFor(() =>
    expect(harnessClient.saveWorkspaceConfig).toHaveBeenCalledTimes(1),
  );
});

const aboutYou = () =>
  screen.getByRole("checkbox", { name: /^Use About you in this project/ });

it("uses About you unless the project turns it off", async () => {
  const { harnessClient } = mount({ mode: "edit" }, {});
  await waitFor(() => expect(aboutYou()).toBeEnabled());
  expect(aboutYou()).toBeChecked();
  expect(aboutYou()).toHaveAccessibleDescription(/Settings → About you/);
  fireEvent.click(aboutYou());
  await waitFor(() =>
    expect(harnessClient.saveWorkspaceConfig).toHaveBeenCalledWith(
      expect.objectContaining({
        workspaceId: "ws",
        expectedRevision: 4,
        body: { mode: "edit", aboutYou: false },
      }),
    ),
  );
  await waitFor(() => expect(aboutYou()).not.toBeChecked());
});

it("turning About you back on leaves the project with no setting of its own", async () => {
  const { harnessClient } = mount({ mode: "edit", aboutYou: false }, {});
  await waitFor(() => expect(aboutYou()).not.toBeChecked());
  fireEvent.click(aboutYou());
  await waitFor(() =>
    expect(harnessClient.saveWorkspaceConfig).toHaveBeenCalledWith(
      expect.objectContaining({ body: { mode: "edit" } }),
    ),
  );
  await waitFor(() => expect(aboutYou()).toBeChecked());
});
