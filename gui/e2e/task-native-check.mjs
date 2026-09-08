// Attach to `npm run tauri dev -- --features e2e` with an isolated
// PIPELINE_E2E_TASK_ROOT and TAURI_WEBDRIVER_PORT=4449. No model calls.
import assert from "node:assert/strict";
import { remote } from "webdriverio";

const app = await remote({ hostname: "127.0.0.1", port: Number(process.env.TAURI_WEBDRIVER_PORT ?? 4449), path: "/", capabilities: { browserName: "tauri" }, logLevel: "error" });
const invoke = (command, args = {}) => app.executeAsync((name, input, done) => {
  window.__TAURI_INTERNALS__.invoke(name, input).then(value => done({ value })).catch(error => done({ error: String(error) }));
}, command, args).then(result => { if (result.error) throw new Error(result.error); return result.value; });
const title = `Native task smoke ${Date.now()}`;
try {
  await app.$('[data-testid="app-shell"]').waitForDisplayed({ timeout: 30000 });
  const dismiss = await app.$('[data-testid="dependencies-dismiss"]');
  if (await dismiss.isExisting()) await dismiss.click();
  await app.$("button=Tasks").click();
  await app.$("h1=Tasks").waitForDisplayed();
  await app.$("button=+ New task").click();
  await app.$("button=Edit definition").click();
  const chain = { schemaVersion: 1, name: title, description: "Native timer and input integration check", steps: [
    { id: "timer", label: "Wait briefly", kind: "delay", seconds: 0 },
    { id: "answer", label: "Wait for the paper", kind: "input", prompt: "Supply the test paper" },
    { id: "paper", label: "Save the supplied paper", kind: "snapshot", input: { kind: "output", step: "answer" }, filename: "paper.md" },
  ] };
  await app.$("textarea.task-code").setValue(JSON.stringify(chain));
  await app.$("button=Prepare task chain →").click();
  await app.$("button=Start task chain").waitForDisplayed();
  const draft = (await invoke("task_list", { view: "active" })).find(run => run.name === title);
  assert.equal(draft.state, "draft");
  await app.$("button=Start task chain").click();
  await app.$("button=Provide input").waitForDisplayed({ timeout: 15000 });
  await app.$(".task-activity textarea").setValue("A deterministic native smoke-test paper.");
  await app.$("button=Provide input").click();
  await app.waitUntil(async () => (await invoke("task_get", { id: draft.id })).state === "finished", { timeout: 15000 });
  const finished = await invoke("task_get", { id: draft.id });
  assert.equal(finished.progress.actions, 3);
  assert.equal(finished.progress.outputs.paper.kind, "artifact");
  assert.equal(Object.keys(finished.progress.receipts).length, 3);
  const full = await invoke("task_step_output", { id: draft.id, address: "paper" });
  assert.equal(full.hash, finished.progress.outputs.paper.hash);
  await app.$(".task-detail .task-status--finished").waitForDisplayed({ timeout: 10000 });
  await app.execute(() => { document.querySelector(".tasks-page").scrollTo(0, 0); });
  await app.saveScreenshot(process.env.PIPELINE_TASK_SCREENSHOT ?? "/private/tmp/pipeline-task-native.png");
  const trigger = { kind: "interval", anchor: Math.floor(Date.now() / 1000) + 86400, seconds: 86400 };
  const schedule = await invoke("task_prepare", { request: { chain: { ...chain, name: `${title} scheduled` }, sessionId: null, inputs: {}, operationId: `schedule-${Date.now()}`, trigger } });
  await invoke("task_control", { id: schedule.id, revision: schedule.revision, action: "start" });
  await app.$('[role="tab"]=Scheduled').click();
  await app.$(`h3=${title} scheduled`).waitForDisplayed();
  const planned = (await invoke("task_schedules")).find(row => row.id === schedule.id);
  assert.ok(planned.enabled); assert.ok(planned.nextDueAt > Date.now() / 1000);
  await invoke("task_update_schedule", { id: planned.id, revision: planned.revision, enabled: false, trigger: null });
  assert.equal((await invoke("task_schedules")).find(row => row.id === planned.id).enabled, false);
  const background = await app.$('.task-background input[type="checkbox"]');
  await background.click();
  await app.waitUntil(async () => await invoke("task_background"), { timeout: 5000 });
  await background.click();
  await app.waitUntil(async () => !(await invoke("task_background")), { timeout: 5000 });
  console.log(JSON.stringify({ status: "passed", taskId: draft.id, checks: ["native task page", "prepare before start", "durable timer", "input wait and delivery", "artifact receipt", "completion", "schedule creation and pause", "native background tray setting"] }));
} finally {
  await app.deleteSession();
}
