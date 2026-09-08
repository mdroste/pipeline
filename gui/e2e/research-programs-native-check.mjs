// Attach only to Tauri dev --features e2e, with disposable
// PIPELINE_WORKBENCH_DEV_ROOT / PIPELINE_E2E_TASK_ROOT. No model or remote calls.
import assert from "node:assert/strict";
import { remote } from "webdriverio";
const app = await remote({
  hostname: "127.0.0.1",
  port: Number(process.env.TAURI_WEBDRIVER_PORT ?? 4451),
  path: "/",
  capabilities: { browserName: "tauri" },
  logLevel: "error",
});
const invoke = (command, args = {}) =>
  app
    .executeAsync(
      (name, input, done) => {
        window.__TAURI_INTERNALS__
          .invoke(name, input)
          .then((value) => done({ value }))
          .catch((error) => done({ error: String(error) }));
      },
      command,
      args,
    )
    .then((r) => {
      if (r.error) throw new Error(r.error);
      return r.value;
    });
const id = `native-programs-${Date.now()}`;
const field = (label) =>
  app.$(
    `//label[span[normalize-space(.)=${JSON.stringify(label)}]]/*[self::input or self::textarea or self::select]`,
  );
const click = async (text) => {
  await app.$(`button=${text}`).waitForExist({ timeout: 15000 });
  const buttons = await app.$$(`button=${text}`);
  const b = buttons[buttons.length - 1];
  await b.waitForDisplayed();
  await b.waitForEnabled({ timeout: 10000 });
  await b.click();
};
let ws;
try {
  await app.$('[data-testid="app-shell"]').waitForDisplayed({ timeout: 30000 });
  ws = (
    await invoke("workbench_create_workspace", {
      request: {
        name: "Test research project",
        root: null,
        operationId: id,
      },
    })
  ).record.id;
  const session = (
    await invoke("workbench_create_session", {
      request: {
        workspaceId: ws,
        title: "Test conversation",
        operationId: `${id}-session`,
      },
    })
  ).record.id;
  await app.execute(
    (workspace, conversation) => {
      localStorage.setItem("pipeline.ui.page", "workspace");
      localStorage.setItem("pipeline.workspace.workspaceId", workspace);
      localStorage.setItem("pipeline.workspace.sessionId", conversation);
      localStorage.setItem("pipeline.workspace.surface", "project");
    },
    ws,
    session,
  );
  await app.refresh();
  await app.$('[data-testid="app-shell"]').waitForDisplayed({ timeout: 30000 });
  const dismiss = await app.$('[data-testid="dependencies-dismiss"]');
  if (await dismiss.isExisting()) await dismiss.click();
  await click("Writing");
  await click("Deliverables & kits");
  await app.$("h2=Research starter kits").waitForDisplayed();
  await click("Theory note");
  await field("Edit the kit instructions").setValue(
    "A rootless theory project with explicit assumptions and a testable mechanism.",
  );
  await click("Install the previewed structure");
  await app.waitUntil(
    async () => {
      const c = await invoke("workbench_program", {
        workspaceId: ws,
        action: { action: "choices" },
      });
      return c.objects.some((o) => o.reference.kind === "note");
    },
    { timeout: 10000 },
  );
  await field("Your section prose").setValue(
    "A seminar outline assembled without a manuscript or model call.",
  );
  await click("Generate deliverable draft");
  await app
    .$("button=Export draft with all asset files")
    .waitForExist({ timeout: 10000 });
  await app.$("button=Export draft with all asset files").scrollIntoView();
  await app.saveScreenshot(
    "/private/tmp/pipeline-programs-native-delivery.png",
  );
  await click("Publication assets");
  await app.$("h2=Publication assets").waitForDisplayed();
  await click("Revision campaigns");
  await app.$("h2=Revision campaigns").waitForDisplayed();
  await click("Coauthors & replication");
  await app.$("h2=Prepare for coauthor review").waitForDisplayed();
  await app.$("h2=Replication capsule").waitForExist();
  await click("Analyses");
  await click("Specification grids");
  await app.$("h2=Specification grids").waitForDisplayed();
  await click("Symbols & assumptions");
  await field("Notation").setValue("x");
  await field("Definition").setValue("Consumption");
  await field("Domain").setValue("Positive reals");
  await field("Units").setValue("Goods");
  await click("Save notation revision");
  await app.$("p=Consumption").waitForDisplayed();
  await click("Tasks");
  await click("Scheduled checks");
  await app.$("h2=Scheduled research checks").waitForDisplayed();
  await app.execute(
    (element) => {
      element.value = "results";
      element.dispatchEvent(new Event("change", { bubbles: true }));
    },
    await field("What to check"),
  );
  await app.$("button=Save and enable check").waitForEnabled();
  await click("Save and enable check");
  await app.$("button=Pause check").waitForDisplayed();
  await click("Pause check");
  await app.$("button=Resume check").waitForDisplayed();
  const program = (action) =>
    invoke("workbench_program", { workspaceId: ws, action });
  assert.equal(
    (await program({ action: "monitors" })).checks[0].enabled,
    false,
  );
  await app.execute(
    (conversation) =>
      window.dispatchEvent(
        new CustomEvent("pipeline:open-session", { detail: conversation }),
      ),
    session,
  );
  await app.$("summary=Follow-up messages").click();
  await app
    .$('textarea[aria-label="Next message draft"]')
    .setValue("Explore a second mechanism after reviewing the evidence.");
  await click("Queue next message");
  await app.$("button=Run next").waitForDisplayed();
  const queue = await invoke("workbench_followups", {
    sessionId: session,
    action: { action: "list" },
  });
  assert.equal(queue.length, 1);
  assert.equal(queue[0].state, "queued");
  assert.equal(
    (await invoke("workbench_conversation_snapshot", { sessionId: session }))
      .items.length,
    0,
  );
  await click("Activity");
  await app
    .$('[role="dialog"][aria-label="Suite activity"]')
    .waitForDisplayed();
  await click("Close");
  await app.saveScreenshot("/private/tmp/pipeline-programs-native-queue.png");
  console.log(
    JSON.stringify({
      status: "passed",
      workspaceId: ws,
      checks: [
        "seven research destinations",
        "rootless kit installation",
        "deliverable generation",
        "notation revision",
        "app-open check pause",
        "explicit queue without transport",
        "suite activity drawer",
      ],
    }),
  );
} catch (e) {
  await app
    .saveScreenshot("/private/tmp/pipeline-programs-native-failure.png")
    .catch(() => {});
  throw e;
} finally {
  await app.deleteSession();
}
