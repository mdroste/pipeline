// Run against Tauri dev --features e2e with disposable stores and
// PIPELINE_E2E_MISSION_SCRIPT pointing to fixtures/mission-cycle.json.
// The provider responses are scripted; UI, IPC, admission, journals, evidence,
// validation and completion use the real desktop coordinator. No model calls.
import assert from 'node:assert/strict';
import { remote } from 'webdriverio';
const app = await remote({ hostname: '127.0.0.1', port: Number(process.env.TAURI_WEBDRIVER_PORT ?? 4457), path: '/', capabilities: { browserName: 'tauri' }, logLevel: 'error' });
const invoke = (command, args = {}) => app.executeAsync((name, input, done) => {
  window.__TAURI_INTERNALS__.invoke(name, input).then(value => done({ value })).catch(error => done({ error: String(error) }));
}, command, args).then(r => { if (r.error) throw new Error(r.error); return r.value; });
const field = label => app.$(`//label[starts-with(normalize-space(.),${JSON.stringify(label)})]/*[self::input or self::textarea or self::select]`);
const id = `mission-native-${Date.now()}`;
try {
  await app.$('[data-testid="app-shell"]').waitForDisplayed({ timeout: 30000 });
  const ws = (await invoke('workbench_create_workspace', { request: { name: 'Mission qualification project', root: null, operationId: id } })).record.id;
  const session = (await invoke('workbench_create_session', { request: { workspaceId: ws, title: 'Aggregation conjecture', operationId: `${id}-session` } })).record.id;
  await app.execute(() => localStorage.removeItem('pipeline.researchMission.draft.v1'));
  await app.refresh();
  await app.$('[data-testid="app-shell"]').waitForDisplayed({timeout:30000});
  const dismiss = await app.$('[data-testid="dependencies-dismiss"]');
  if (await dismiss.isExisting()) await dismiss.click();
  await app.$('button=Tasks').click();
  await app.$('[role="tab"]=Missions').click();
  await app.$('button=+ New mission').click();
  await app.$(`option[value="${session}"]`).waitForExist();
  await app.execute(value => { const select=document.querySelector('.mission-builder select'); select.value=value; select.dispatchEvent(new Event('change',{bubbles:true})); }, session);
  await field('Mission name').setValue('Investigate the aggregation identity');
  await field('Research question and remit').setValue('Determine whether E[x²] = E[x]² holds for heterogeneous types. Supply an analytical counterexample or a derivation.');
  await field('Completion criteria').setValue('Resolve the identity using a checked analytical example and retain the reasoning.');
  await app.$('button=Prepare mission →').scrollIntoView();
  await app.$('button=Prepare mission →').waitForEnabled();
  await app.$('button=Prepare mission →').click();
  await app.$('button=Start mission').waitForDisplayed({ timeout: 15000 });
  const draft = (await invoke('mission_list', { workspaceId: ws, offset: 0 }))[0];
  const prepared = await invoke('mission_get', { id: draft.id });
  assert.equal(prepared.state, 'draft'); assert.equal(prepared.actionsReserved, 0); assert.equal(prepared.childIds.length, 0);
  assert.equal(new Set([prepared.scope.sessionId, prepared.plannerScope.sessionId, prepared.challengerScope.sessionId]).size, 3);
  await app.saveScreenshot('/private/tmp/pipeline-mission-prepared.png');
  await app.$('button=Start mission').click();
  await app.waitUntil(async () => {
    const m = await invoke('mission_get', { id: draft.id });
    if (m.state === 'attention') throw new Error(m.reason);
    return m.state === 'completed';
  }, { timeout: 60000, interval: 250, timeoutMsg: 'Scripted mission did not complete' });
  const finished = await invoke('mission_get', { id: draft.id });
  assert.equal(finished.actionsReserved, 3); assert.equal(finished.rounds.length, 1);
  assert.equal(finished.goals[0].state, 'refuted'); assert.equal(finished.rounds[0].challenge.missionComplete, true);
  assert.equal(finished.activeChild, null); assert.equal(finished.methods[0].retained, false);
  for (const childId of finished.childIds) {
    const child = await invoke('task_get', { id: childId });
    assert.equal(child.state, 'finished'); assert.equal(child.missionId, finished.id);
    assert.equal(child.progress.outputs.action.qualification, 'scripted-native-development');
  }
  assert.equal((await invoke('task_list', { view: 'history' })).some(t => finished.childIds.includes(t.id)), false);
  const evidence = await invoke('mission_evidence', { id: finished.id, evidenceId: 'r1-memo' });
  assert.ok(evidence.text.includes('Var(x)=1')); assert.equal(evidence.truncated, false);
  await app.$('[role="tab"]=Findings').click();
  await app.$('h4=Independent challenge · refuted').waitForDisplayed();
  await app.saveScreenshot('/private/tmp/pipeline-mission-findings.png');
  await app.$('[role="tab"]=Methods').click();
  await app.$('button=Save for reuse').click();
  await app.$('button=Remove from retained methods').waitForDisplayed();
  assert.equal((await invoke('mission_methods', { workspaceId: ws })).length, 1);
  await app.$('[role="tab"]=Brief').click();
  await app.$('.mission-brief h1').waitForExist({timeout:15000});
  await app.execute(() => document.querySelector('.mission-brief').scrollIntoView({block:'start'}));
  await app.saveScreenshot('/private/tmp/pipeline-mission-brief.png');
  console.log(JSON.stringify({ status: 'passed', missionId: finished.id, checks: ['prepared scope', 'separate roles', 'three durable child actions', 'negative-result completion', 'exact evidence', 'method retention', 'native mission interface'], provider: 'scripted' }));
} finally { await app.deleteSession(); }
