// Attach to `npm run tauri dev -- --features e2e` with a disposable
// PIPELINE_WORKBENCH_DEV_ROOT and TAURI_WEBDRIVER_PORT=4451. PIPELINE_E2E_TEX names
// a folder holding paper/main.tex, which \inputs paper/sections/intro.tex. Needs
// pdflatex on PATH. No model calls. This drives Write → Manuscript through a real
// build, a broken build, and a repair. Keep the app window uncovered.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { remote } from "webdriverio";

const source = process.env.PIPELINE_E2E_TEX;
const shots = process.env.PIPELINE_E2E_SHOTS ?? "/private/tmp";
assert.ok(source, "Set PIPELINE_E2E_TEX to the fixture folder");
// A folder belongs to one project, so each run works on its own copy.
const folder = fs.mkdtempSync(`${source}-run-`);
fs.cpSync(source, folder, { recursive: true });
const app = await remote({ hostname: "127.0.0.1", port: Number(process.env.TAURI_WEBDRIVER_PORT ?? 4451), path: "/", capabilities: { browserName: "tauri" }, logLevel: "error" });
const invoke = (command, args = {}) => app.executeAsync((name, input, done) => {
  // Not `error`: WebDriver would treat that key as a failed call and resend it.
  window.__TAURI_INTERNALS__.invoke(name, input).then(value => done({ value })).catch(failure => done({ failure: JSON.stringify(failure) }));
}, command, args).then(result => { if (result.failure) throw new Error(`${command}: ${result.failure}`); return result.value; });
const results = [];
const check = (name, passed, detail = "") => { results.push({ name, passed, detail }); console.log(`${passed ? "ok  " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`); };
const shot = name => app.saveScreenshot(`${shots}/manuscript-${name}.png`);
const button = text => app.$(`//button[normalize-space(.)=${JSON.stringify(text)} or @aria-label=${JSON.stringify(text)}]`);
const startingWith = text => app.$(`//*[@role="dialog"]//button[starts-with(normalize-space(.), ${JSON.stringify(text)})]`);
const click = async text => {
  const target = await button(text);
  await target.waitForClickable({ timeout: 20000 });
  await target.click();
};
const field = label => app.$(`//label[span[normalize-space(.)=${JSON.stringify(label)}]]/*[self::input or self::textarea or self::select]`);
const editorText = () => app.execute(() => document.querySelector(".manuscript .cm-content")?.innerText ?? "");
const cursorLine = () => app.execute(() => document.querySelector(".manuscript .file-statusbar-end span")?.textContent ?? "");
const finish = async () => {
  await click("Build");
  const review = await startingWith("Review");
  await review.waitForClickable({ timeout: 20000 });
  const label = await review.getText();
  await review.click();
  const start = await startingWith("Authorize and start");
  await start.waitForClickable({ timeout: 20000 });
  await start.click();
  return label;
};
const waitForOutcome = (before, timeout = 120000) => app.waitUntil(async () => {
  const shown = await app.execute(() => [...document.querySelectorAll(".manuscript-bar button")].map(b => b.textContent).find(t => t.startsWith("Last build")) ?? "");
  return shown && shown !== before ? shown : false;
}, { timeout, interval: 1000, timeoutMsg: "The build's result never appeared" });
const lastBuild = () => app.execute(() => [...document.querySelectorAll(".manuscript-bar button")].map(b => b.textContent).find(t => t.startsWith("Last build")) ?? "");

try {
  await app.$('[data-testid="app-shell"]').waitForDisplayed({ timeout: 60000 });
  await app.waitUntil(async () => (await app.execute(() => document.visibilityState)) === "visible", { timeout: 20000, timeoutMsg: "The app window is covered, so it will not paint" });
  const dismiss = await app.$('[data-testid="dependencies-dismiss"]');
  if (await dismiss.isExisting()) await dismiss.click();
  const created = await invoke("workbench_create_workspace", { request: { name: `Manuscript ${Date.now()}`, root: folder, operationId: `e2e-${Date.now()}` } });
  const workspaceId = created.id ?? created.record?.id;
  assert.ok(workspaceId, JSON.stringify(created));
  await invoke("workbench_project_mutate", { request: { workspaceId, operationId: `refresh-${Date.now()}`, action: "refresh" } });
  await app.execute(id => { location.hash = `#/project/${encodeURIComponent(id)}?dest=writing&surface=project`; }, workspaceId);

  // Opens by itself.
  await app.$('[aria-label="Manuscript source editor"]').waitForExist({ timeout: 30000 });
  const opened = await app.$('select[aria-label="File"]').getValue();
  check("opens the project's main file without asking", opened === "paper/main.tex", opened);
  check("shows the file's text", (await editorText()).includes("documentclass"));
  await shot("opened");

  // First build: suggested settings, then the approval step.
  await click("Build");
  const inputs = await (await field("Declared inputs (one per line, relative to build directory)")).getValue();
  check("suggests the root and its neighbours", inputs.includes("main.tex") && inputs.includes("sections/intro.tex"), inputs.replaceAll("\n", ", "));
  await shot("suggested");
  await click("Save build settings");
  await (await startingWith("Review")).waitForClickable({ timeout: 20000 });
  const first = await (await startingWith("Review")).getText();
  check("offers a test run first", first === "Review test run", first);
  await click(first);
  await (await startingWith("Authorize and start")).waitForClickable({ timeout: 20000 });
  const command = await app.execute(() => document.querySelector('[role="dialog"] pre')?.textContent ?? "");
  check("shows the exact command for approval", /pdflatex/.test(command) && /no-shell-escape/.test(command), command);
  await shot("approval");
  await (await startingWith("Authorize and start")).click();

  // The result opens by itself.
  const success = await waitForOutcome("");
  check("the finished build reports itself", /succeeded/.test(success), success);
  await app.$(".manuscript .pdf-reader").waitForExist({ timeout: 20000 });
  check("its PDF opens beside the source", true);
  if ((await app.execute(() => document.visibilityState)) === "visible") await app.$(".manuscript .pdfViewer .page canvas, .manuscript .textLayer span").waitForExist({ timeout: 30000 });
  await shot("built");
  assert.ok(fs.existsSync(path.join(folder, "paper/main.tex")));

  // A double-click in the PDF finds the source.
  const span = await app.$(".manuscript .textLayer span");
  if (await span.isExisting()) {
    // WebDriver's double click does not reach PDF.js's text layer.
    await app.execute(() => {
      const target = [...document.querySelectorAll(".manuscript .textLayer span")].find(s => /Border/.test(s.textContent));
      const box = target.getBoundingClientRect();
      target.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: box.left + 8, clientY: box.top + box.height / 2 }));
    });
    await app.pause(2000);
    const moved = await cursorLine();
    check("double-clicking the PDF moves the source", !/Ln 1,/.test(moved), moved);
    await shot("synctex");
  } else check("double-clicking the PDF moves the source", false, "no PDF text layer");

  // Break an included file, in the editor.
  await app.execute(() => {
    const select = document.querySelector('select[aria-label="File"]');
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set.call(select, "paper/sections/intro.tex");
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await app.waitUntil(async () => (await editorText()).includes("Border discontinuity"), { timeout: 15000, timeoutMsg: "The included file did not open" });
  await (await app.$(".manuscript .cm-content")).click();
  await app.keys(["Meta", "End"]);
  await app.keys("\\notacommand ");
  await app.waitUntil(async () => (await button("Save working copy")).isExisting(), { timeout: 5000, timeoutMsg: "Editing did not start a draft" });
  await shot("draft");
  await click("Save working copy");
  await app.waitUntil(async () => !(await (await button("Save working copy")).isExisting()), { timeout: 10000 });
  check("saves to the real file", fs.readFileSync(path.join(folder, "paper/sections/intro.tex"), "utf8").includes("\\notacommand"));
  const before = await lastBuild();
  const second = await finish();
  check("a passed test goes straight to a run", second === "Review run", second);
  const failed = await waitForOutcome(before);
  check("the broken build reports itself", !/succeeded/.test(failed), failed);
  await app.pause(1500);
  await shot("failed");
  const message = await app.execute(() => [...document.querySelectorAll(".manuscript-messages li")].map(l => l.textContent));
  check("the error is listed", message.some(m => /Undefined control sequence/.test(m)), message.slice(0, 3).join(" | "));

  // The error's link reaches the file that holds it.
  const link = await app.$(".manuscript-messages li[data-severity='error'] button");
  if (await link.isExisting()) {
    const label = await link.getText();
    await link.click();
    await app.pause(1500);
    const alert = await app.execute(() => document.querySelector('.manuscript [role="alert"]')?.textContent ?? "");
    check("the error link opens the source", !alert && (await app.$('select[aria-label="File"]').getValue()) === "paper/sections/intro.tex", `${label} ${alert} ${await cursorLine()}`);
    await shot("error-link");
  } else check("the error link opens the source", false, "the error has no file and line");

  // Repair it.
  fs.cpSync(path.join(source, "paper/sections/intro.tex"), path.join(folder, "paper/sections/intro.tex"));
  await click("More manuscript actions");
  await (await app.$('//*[@role="menuitem"][normalize-space(.)="Reload file"]')).click();
  await app.pause(1000);
  const seen = await lastBuild();
  await finish();
  const repaired = await waitForOutcome(seen);
  check("the repaired build succeeds", /succeeded/.test(repaired), repaired);
  await app.pause(1500);
  check("the PDF returns", await (await app.$(".manuscript .pdf-reader")).isExisting());
  check("job history steps aside after a clean build", !(await (await app.$(".project-jobs")).isExisting()));
  await shot("repaired");
} catch (error) {
  await shot("error").catch(() => undefined);
  check("run finished", false, String(error));
} finally {
  await app.deleteSession().catch(() => undefined);
}
const failed = results.filter(result => !result.passed);
console.log(`${results.length - failed.length} of ${results.length} checks passed`);
process.exitCode = failed.length ? 1 : 0;
