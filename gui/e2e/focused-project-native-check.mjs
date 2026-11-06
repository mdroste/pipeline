// Run against a Tauri e2e development instance with disposable Workspace data.
// Creates two example projects and a chat; no model calls or external tools.
import assert from "node:assert/strict";
import { remote } from "webdriverio";
const port = Number(process.env.TAURI_WEBDRIVER_PORT);
assert.ok(
  port && process.env.PIPELINE_WORKBENCH_DEV_ROOT,
  "Specify the disposable development root and WebDriver port",
);
const app = await remote({
  hostname: "127.0.0.1",
  port,
  path: "/",
  capabilities: { browserName: "tauri" },
  logLevel: "error",
});
const call = (command, args) =>
  app
    .executeAsync(
      (command, args, done) => {
        window.__TAURI_INTERNALS__.invoke(command, args).then(
          (value) => done({ value }),
          (failure) => done({ failure: String(failure) }),
        );
      },
      command,
      args,
    )
    .then((r) => {
      assert.ok(!r.failure, r.failure);
      return r.value;
    });
const invoke = (command, request) =>
  call(command, { request }).then((value) => value.record);
const visible = async (selector) => {
  const e = await app.$(selector);
  await e.waitForDisplayed({ timeout: 20000 });
  return e;
};
const click = async (selector) => (await visible(selector)).click();
const rail = (label) =>
  "//aside//button[normalize-space(.)=" + JSON.stringify(label) + "]";
const picture = async (name) => {
  if (process.env.PIPELINE_E2E_SHOTS) {
    // Capture the settled UI even if the test window's compositor is suspended.
    await app.execute(() => {
      for (const a of document.getAnimations()) {
        if (a.effect?.getComputedTiming().endTime < Infinity) a.finish();
      }
    });
    await app.saveScreenshot(
      process.env.PIPELINE_E2E_SHOTS + "/focused-" + name + ".png",
    );
  }
};
const savedRoute = () =>
  app.execute(() => localStorage.getItem("pipeline.ui.route") ?? "");
const reaches = async (text) =>
  app.waitUntil(async () => (await savedRoute()).includes(text), {
    timeout: 10000,
    timeoutMsg: "Saved route did not reach " + text,
  });
try {
  await visible('[data-testid="app-shell"]');
  const expand = await app.$('button[aria-label="Expand sidebar"]');
  if (await expand.isDisplayed()) await expand.click();
  const dismiss = await app.$('[data-testid="dependencies-dismiss"]');
  if (await dismiss.isDisplayed()) await dismiss.click();
  const known = (
    await call("workbench_list_workspaces", { includeArchived: false })
  ).workspaces;
  const create = (name) =>
    known.find((p) => p.name === name) ??
    invoke("workbench_create_workspace", {
      name,
      operationId: crypto.randomUUID(),
    });
  const one = await create("Production networks"),
    two = await create("Household portfolios");
  if (
    !(
      await call("workbench_list_sessions", {
        workspaceId: one.id,
        includeArchived: false,
      })
    ).sessions.length
  )
    await invoke("workbench_create_session", {
      workspaceId: one.id,
      title: "Aggregate elasticity",
      operationId: crypto.randomUUID(),
    });
  await app.execute((id) => {
    location.hash = "#/project/" + id + "?dest=overview&surface=project";
  }, one.id);
  await visible(
    'aside button[aria-label="Switch project: Production networks"]',
  );
  await click(rail("Project tools"));
  await visible('input[aria-label="Search project tools"]');
  assert.equal(await app.$$(".focused-tool").length, 34);
  await (
    await visible('input[aria-label="Search project tools"]')
  ).setValue("Symbols & assumptions");
  await app.waitUntil(
    async () => (await app.$$(".focused-tool").length) === 1,
    { timeout: 8000, timeoutMsg: "Symbol search did not narrow to one result" },
  );
  await click(".focused-tool");
  await visible(".focused-location");
  assert.equal(
    await (await app.$(".focused-project-scope [aria-current=page]")).getText(),
    "Analysis",
  );
  console.log("Specialist destination opens from the complete directory.");
  await click(rail("Chats"));
  await visible(".focused-chat-open");
  assert.match(
    await (await app.$(".focused-chat-open")).getText(),
    /Aggregate elasticity/,
  );
  await click(".focused-chat-open");
  await visible('button[aria-label="Conversation menu"]');
  await click(rail("Reviews & edits"));
  await visible('nav[aria-label="Reviews & edits views"]');
  await click(
    '//nav[@aria-label="Reviews & edits views"]//button[normalize-space(.)="Proposed edits"]',
  );
  await reaches("dest=edits");
  console.log("Project chat and existing edit route remain accessible.");
  await click('aside button[aria-label="Switch project: Production networks"]');
  await (
    await visible('input[aria-label="Find a project"]')
  ).setValue("Household");
  await click(
    '//div[@role="dialog"]//button[contains(normalize-space(.),"Household portfolios")]',
  );
  await visible(
    'aside button[aria-label="Switch project: Household portfolios"]',
  );
  await reaches(two.id);
  await click(rail("Files"));
  await visible('nav[aria-label="Files views"] button[aria-current="page"]');
  await reaches("dest=documents");
  await click(
    'aside button[aria-label="Switch project: Household portfolios"]',
  );
  await click(
    '//div[@role="dialog"]//button[contains(normalize-space(.),"Production networks")]',
  );
  await visible(
    'nav[aria-label="Reviews & edits views"] button[aria-current="page"]',
  );
  await reaches("dest=edits");
  console.log(
    "Switching restores the project view; Files opens imported documents without a folder.",
  );
  await click(rail("Overview"));
  const hideConversation = await app.$(
    'button[aria-label="Hide conversation"]',
  );
  if (await hideConversation.isDisplayed()) await hideConversation.click();
  await visible('.workspace-desk-panes[data-layout="project"]');
  await reaches("dest=overview");
  await visible(rail("Overview") + '[@aria-current="page"]');
  await picture("overview");
  const { width, height } = await app.getWindowSize();
  const dark = await app.execute(() =>
    document.documentElement.classList.contains("dark"),
  );
  try {
    await app.setWindowSize(1024, 700);
    await app.execute(() => document.documentElement.classList.remove("dark"));
    // The native resize resolves before WebKit updates its media query.
    // Wait for React to replace the wide sidebar before choosing a row.
    await visible('aside button[aria-label="Production networks"]');
    // WebKit may suspend compositor animations in a background test window.
    // Complete the rail's finite width animation before WebDriver hit-tests it.
    await app.execute(() =>
      document
        .querySelector("aside")
        .getAnimations()
        .forEach((a) => a.finish()),
    );
    const chooseNarrow = async (label) => {
      const drawer = '[role="dialog"][aria-label="Production networks"]';
      if (!(await (await app.$(drawer)).isDisplayed()))
        await click('aside button[aria-label="Production networks"]');
      return click(
        '//*[@role="dialog" and @aria-label="Production networks"]//button[normalize-space(.)=' +
          JSON.stringify(label) +
          "]",
      );
    };
    await chooseNarrow("Project tools");
    await visible('input[aria-label="Search project tools"]');
    assert.ok(
      await app.execute(() => {
        const pane = document.querySelector(".focused-page");
        return pane.clientWidth > 0 && pane.scrollWidth <= pane.clientWidth + 1;
      }),
      "Tools overflowed horizontally",
    );
    await click('aside button[aria-label="Production networks"]');
    await visible(
      '[role="dialog"][aria-label="Production networks"] nav[aria-label="Project sections"]',
    );
    await picture("narrow-tools");
    await chooseNarrow("Project settings");
    await reaches("dest=project-settings");
    assert.equal(
      await (
        await app.$('[role="dialog"][aria-label="Production networks"]')
      ).isExisting(),
      false,
    );
    await click('aside button[aria-label="Projects"]');
    await visible("#project-index-search");
    console.log(
      "Tools and settings fit the minimum window width in light mode.",
    );
  } catch (error) {
    console.log(
      await app.execute(() => ({
        width: innerWidth,
        dialogs: [...document.querySelectorAll('[role="dialog"]')].map((d) => [
          d.ariaLabel,
          d.innerText,
        ]),
        rail: [...document.querySelectorAll("aside button")].map((b) => [
          b.ariaLabel,
          b.ariaExpanded,
        ]),
      })),
    );
    await picture("compact-diagnostic");
    throw error;
  } finally {
    await app.execute(
      (value) => document.documentElement.classList.toggle("dark", value),
      dark,
    );
    await app.setWindowSize(width, height);
  }
} catch (error) {
  console.log(
    await app.execute(() => ({
      hash: location.hash,
      rail: document.querySelector(".focused-project-scope [aria-current=page]")
        ?.textContent,
      headings: [
        ...document.querySelectorAll(
          "h1,h2,[role=alert],.workspace-panel-loading,.focused-location",
        ),
      ]
        .filter((e) => e.getClientRects().length)
        .map((e) => e.textContent)
        .slice(0, 15),
      query: document.querySelector(".focused-page input")?.value,
    })),
  );
  await picture("diagnostic");
  throw error;
} finally {
  await app.deleteSession();
}
