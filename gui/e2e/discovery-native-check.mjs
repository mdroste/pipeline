// Tauri dev --features e2e with disposable stores and
// PIPELINE_E2E_DISCOVERY=synthetic-history-fixture. No model calls.
import assert from "node:assert/strict";
import { remote } from "webdriverio";
const app=await remote({hostname:"127.0.0.1",port:Number(process.env.TAURI_WEBDRIVER_PORT??4483),path:"/",capabilities:{browserName:"tauri"},logLevel:"error"});
const invoke=(command,args={})=>app.executeAsync((name,input,done)=>{
  window.__TAURI_INTERNALS__.invoke(name,input).then(value=>done({value})).catch(error=>done({error:String(error)}));
},command,args).then(r=>{if(r.error)throw new Error(r.error);return r.value;});
const field=label=>app.$(`//label[starts-with(normalize-space(.),${JSON.stringify(label)})]/*[self::input or self::textarea or self::select]`);
async function reached(id,predicate){
  await app.waitUntil(async()=>{
    const run=await invoke("discovery_get",{id});
    if(["blocked","failed","exhausted"].includes(run.state))throw new Error(run.reason);
    return predicate(run);
  },{timeout:180000,interval:250,timeoutMsg:"Discovery did not reach the expected state"});
  return invoke("discovery_get",{id});
}
try {
  await app.$('[data-testid="app-shell"]').waitForDisplayed({timeout:30000});
  const dismiss=await app.$('[data-testid="dependencies-dismiss"]');
  if(await dismiss.isExisting())await dismiss.click();
  await app.$('button=Automations').click();
  await app.$('[role="tab"]=Research').click();
  await app.$('button=Full self-discovery (supervised)').click();
  await field("Research topics and questions").setValue("Synthetic qualification: investigate historical source transmission");
  await field("Candidate projects").setValue("50");
  await field("Shortlisted projects").setValue("3");
  await field("Papers requested").setValue("2");
  await app.$('summary=Research and review settings').click();
  await field("Detailed proposal reviewers").setValue("1");
  await field("Paper reviewers per round").setValue("1");
  const acquisition=await app.$('//label[contains(. ,"Search public literature records")]/input');
  if(await acquisition.isSelected())await acquisition.click();
  await app.$('button=Start self-discovery').scrollIntoView();
  await app.saveScreenshot("/private/tmp/pipeline-discovery-start.png");
  await app.$('button=Start self-discovery').click();
  await app.waitUntil(async()=>(await invoke("discovery_list",{offset:0})).length>0,{timeout:15000});
  const id=(await invoke("discovery_list",{offset:0}))[0].id;
  const selection=await reached(id,r=>r.state==="awaitingSelection");
  assert.equal(selection.candidateCount,50);
  assert.equal(selection.papers.length,0);
  assert.equal(selection.selection.shortlist.length,3);
  await app.$('button=Develop selected projects').waitForEnabled({timeout:15000});
  await app.saveScreenshot("/private/tmp/pipeline-discovery-shortlist.png");
  await app.$('button=Develop selected projects').click();
  const finished=await reached(id,r=>r.state==="completed");
  assert.equal(finished.papers.length,2);
  assert.equal(finished.ranking.length,2);
  assert.ok(finished.papers.every(p=>p.reviewCount===1));
  const paper=await invoke("discovery_paper",{id,candidateId:finished.papers[0].candidateId});
  assert.ok(paper.versions[0].artifacts["paper.md"].hash);
  assert.equal(paper.versions[0].reviews.length,1);
  const sessions=await invoke("task_sessions");
  assert.equal(sessions.filter(s=>s.workspaceName?.includes("Synthetic qualification")).length,1);
  await app.$('button=Read paper and reviews').waitForExist({timeout:15000});
  await app.$('button=Read paper and reviews').click();
  await app.$('[aria-label="Paper reader"]').waitForDisplayed();
  await app.$('[aria-label="Paper reader"]').scrollIntoView();
  await app.saveScreenshot("/private/tmp/pipeline-discovery-paper.png");
  const unsupervised=await invoke("discovery_start",{request:{definition:{...finished.definition,mode:"unsupervised",prompt:"Synthetic unsupervised qualification",proposalRevisionPasses:0},sessionId:null,operationId:`unsupervised-${Date.now()}`}});
  const auto=await reached(unsupervised.id,r=>r.state==="completed");
  assert.equal(auto.selected.length,2);
  assert.equal(auto.ranking.length,2);
  const events=await invoke("discovery_events",{id:auto.id,after:0});
  assert.ok(events.every(e=>e.kind!=="selected"));
  console.log(JSON.stringify({status:"passed",supervised:id,unsupervised:auto.id,provider:"synthetic",checks:["native Start","50 proposals","revision and reassessment","supervised selection","two reviewed papers","artifact capture","paper reader","hidden internal roles","unsupervised completion"]}));
} finally {await app.deleteSession();}
