// Attach to `npm run tauri dev -- --features e2e` with a disposable
// PIPELINE_WORKBENCH_DEV_ROOT and TAURI_WEBDRIVER_PORT=4451. PIPELINE_E2E_FILES
// names a folder made by file-browser-fixture.py. No model calls. This proves
// what jsdom cannot: range requests, media, CSP and the sandbox in a real webview.
// Keep the app window uncovered: WebKit pauses painting for a covered window.
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { remote } from "webdriverio";

const folder = process.env.PIPELINE_E2E_FILES;
const shots = process.env.PIPELINE_E2E_SHOTS ?? "/private/tmp";
assert.ok(folder, "Set PIPELINE_E2E_FILES to the fixture folder");
const app = await remote({ hostname: "127.0.0.1", port: Number(process.env.TAURI_WEBDRIVER_PORT ?? 4451), path: "/", capabilities: { browserName: "tauri" }, logLevel: "error" });
const invoke = (command, args = {}) => app.executeAsync((name, input, done) => {
  // Not `error`: WebDriver would treat that key as a failed call and resend it.
  window.__TAURI_INTERNALS__.invoke(name, input).then(value => done({ value })).catch(failure => done({ failure: JSON.stringify(failure) }));
}, command, args).then(result => { if (result.failure) throw new Error(`${command}: ${result.failure}`); return result.value; });
const page = (script, ...args) => app.executeAsync(`const done=arguments[arguments.length-1];(async(...a)=>{${script}})(...Array.from(arguments).slice(0,-1)).then(value=>done({value}),error=>done({error:String(error)}))`, ...args).then(result => { if (result.error) throw new Error(result.error); return result.value; });
const results = [];
const check = (name, passed, detail = "") => { results.push({ name, passed, detail }); console.log(`${passed ? "ok  " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`); };
const tree = name => app.$(`//*[@role="treeitem"][.//*[normalize-space(.)=${JSON.stringify(name)}]]`);
const open = async name => { await (await tree(name)).click(); };
const shot = name => app.saveScreenshot(`${shots}/files-${name}.png`);
try {
  await app.$('[data-testid="app-shell"]').waitForDisplayed({ timeout: 60000 });
  // A covered window never paints PDF pages, so this run then checks everything else.
  const covered = (await page("return document.visibilityState")) !== "visible";
  if (covered) console.log("The app window is covered: skipping the PDF paint check.");
  const dismiss = await app.$('[data-testid="dependencies-dismiss"]');
  if (await dismiss.isExisting()) await dismiss.click();
  const workspace = await invoke("workbench_create_workspace", { request: { name: `Files ${Date.now()}`, root: folder, operationId: `e2e-${Date.now()}` } });
  const workspaceId = workspace.id ?? workspace.record?.id;
  assert.ok(workspaceId, JSON.stringify(workspace));
  const file = (path) => invoke("workbench_file_open", { request: { workspaceId, path, checkpointId: null, revisionId: null } });
  const url = token => page(`return window.__TAURI_INTERNALS__.convertFileSrc(a[0],"pipeline-file")`.replace("a[0]", JSON.stringify(token)));

  // The scheme itself, in the real webview.
  const pdf = await file("huge.pdf");
  assert.equal(pdf.kind, "pdf");
  assert.ok(pdf.bytes > 1024 ** 3, "the fixture must exceed 1 GiB");
  assert.equal(pdf.base64, null);
  const pdfUrl = await url(pdf.streamToken);
  check("URL form", /^pipeline-file:|^http:\/\/pipeline-file\.localhost/.test(pdfUrl), pdfUrl.slice(0, 40));
  const tail = await page(`const r=await fetch(a[0],{headers:{Range:"bytes=-16"}});return {status:r.status,range:r.headers.get("content-range"),length:(await r.arrayBuffer()).byteLength}`.replaceAll("a[0]", JSON.stringify(pdfUrl)));
  check("fetch with Range in the webview", tail.status === 206 && tail.length === 16, JSON.stringify(tail));
  const refused = await page(`try{const r=await fetch(a[0]);return r.status}catch(e){return "blocked"}`.replaceAll("a[0]", JSON.stringify(pdfUrl.replace(pdf.streamToken, "0".repeat(64)))));
  check("unknown token refused", refused === 410 || refused === "blocked", String(refused));

  // The app itself.
  await invoke("workbench_project_mutate", { request: { workspaceId, operationId: `refresh-${Date.now()}`, action: "refresh" } });
  await page(`location.hash=${JSON.stringify(`#/project/${encodeURIComponent(workspaceId)}?dest=files&surface=project`)}`);
  await app.waitUntil(async () => (await tree("code")).isExisting(), { timeout: 30000, timeoutMsg: "The folder tree did not appear" });
  check("tree lists the folder", await (await tree("figure.png")).isExisting());
  await shot("tree");

  await page(`window.__bytes=0;const f=window.fetch;window.fetch=async(...x)=>{const r=await f(...x);if(String(x[0]).includes("pipeline-file")){const c=r.clone();c.arrayBuffer().then(b=>window.__bytes+=b.byteLength)}return r}`);
  const started = Date.now();
  await open("huge.pdf");
  if (!covered) await app.$(".textLayer span, .pdfViewer .page canvas").waitForExist({ timeout: 30000 });
  await app.$("//*[contains(normalize-space(.), 'of 3')]").waitForExist({ timeout: 30000 });
  const read = await page(`return window.__bytes`);
  check("1 GiB PDF opens quickly", Date.now() - started < 15000, `${Date.now() - started} ms`);
  check("1 GiB PDF reads under 64 MB", read < 64 * 1024 * 1024, `${(read / 1024 / 1024).toFixed(1)} MB read`);
  await shot("pdf");

  await open("figure.png");
  const png = await app.$('img[alt="figure.png"]');
  await png.waitForExist({ timeout: 10000 });
  await app.waitUntil(() => page(`return document.querySelector('img[alt="figure.png"]').naturalWidth>0`), { timeout: 10000, timeoutMsg: "The PNG did not decode: CSP may block the scheme" });
  await app.$("//*[contains(., '640 × 480')]").waitForExist({ timeout: 5000 });
  check("PNG loads through the scheme and reports its size", true);
  await app.$('button[aria-label="Zoom in"]').click();
  await shot("image");

  await open("scan.tif");
  await app.$('img[alt="scan.tif"]').waitForExist({ timeout: 10000 });
  await app.waitUntil(() => page(`return document.querySelector('img[alt="scan.tif"]').naturalWidth>0`), { timeout: 10000, timeoutMsg: "The TIFF preview did not decode" });
  check("TIFF converts and shows", true);

  await open("tone.wav");
  const media = await app.$("audio");
  await media.waitForExist({ timeout: 10000 });
  await app.waitUntil(() => page(`return document.querySelector("audio").readyState>=1`), { timeout: 15000, timeoutMsg: "Audio metadata did not load" });
  const seek = await page(`const m=document.querySelector("audio");m.currentTime=m.duration/2;await new Promise(r=>setTimeout(r,800));return {duration:m.duration,current:m.currentTime,error:m.error?.code??null}`);
  check("audio loads and seeks", seek.error === null && seek.current > 0, JSON.stringify(seek));

  if (await (await tree("clip.mp4")).isExisting()) {
    await open("clip.mp4");
    await app.$("video").waitForExist({ timeout: 10000 });
    await app.waitUntil(() => page(`return document.querySelector("video").readyState>=1`), { timeout: 15000, timeoutMsg: "Video metadata did not load" });
    const video = await page(`const m=document.querySelector("video");m.currentTime=m.duration/2;await new Promise(r=>setTimeout(r,800));return {duration:m.duration,current:m.currentTime,width:m.videoWidth,error:m.error?.code??null}`);
    check("video loads and seeks", video.error === null && video.current > 0, JSON.stringify(video));
    await shot("video");
  }

  await open("unsafe.html");
  const frame = await app.$("iframe[sandbox]");
  await frame.waitForExist({ timeout: 10000 });
  await app.pause(1500);
  const sandbox = await page(`const f=document.querySelector("iframe[sandbox]");let reachable=true;try{f.contentDocument.body}catch{reachable=false}return {attribute:f.getAttribute("sandbox"),document:f.contentDocument===null?"opaque":"reachable",hacked:Boolean(window.__hacked),cookie:document.title.includes("hacked")}`);
  check("HTML runs no script and stays opaque", sandbox.attribute === "" && sandbox.document === "opaque" && !sandbox.hacked && !sandbox.cookie, JSON.stringify(sandbox));
  await shot("html");

  const dataStarted = Date.now();
  const dataFile = await file("panel.dta");
  const dataMs = Date.now() - dataStarted;
  check("a sparse 2 GiB dataset opens without being read", dataFile.kind === "data" && dataFile.text === null && dataFile.bytes > 2 * 1024 ** 3 - 1 && dataMs < 2000, `${dataMs} ms`);
  for (const name of ["data.csv", "panel.dta"]) {
    await open(name);
    await app.$("//*[normalize-space(.)='Data files are not shown here']").waitForExist({ timeout: 10000 });
    check(`${name} shows its size and actions, not its rows`, !(await (await app.$("table")).isExisting()) && (await (await app.$("//button[normalize-space(.)='Open externally']")).isExisting()));
  }
  await open("book.xlsx");
  await app.$("//*[contains(normalize-space(.), 'rows ·')]").waitForExist({ timeout: 10000 });
  check("XLSX shows rows", true);
  await shot("table");
  await open("analysis.ipynb");
  await app.$(".file-notebook-cell").waitForExist({ timeout: 10000 });
  check("notebook renders cells", true);
  await shot("notebook");
  await open("report.docx");
  await app.$(".file-document article").waitForExist({ timeout: 10000 });
  check("DOCX shows its text", (await (await app.$(".file-document article")).getText()).length > 0);

  // Polling pauses while the window is covered. From here on nothing depends on
  // painting, so report visible to keep the result independent of the desktop.
  await page(`Object.defineProperty(document,"visibilityState",{configurable:true,get:()=>"visible"})`);
  // Changes from outside the app reach an open, clean file by themselves.
  await open("notes.md");
  await app.$("button=Source").click();
  await app.$(".cm-content").waitForExist({ timeout: 10000 });
  fs.appendFileSync(path.join(folder, "notes.md"), "Added outside the app.\n");
  await app.waitUntil(async () => (await (await app.$(".cm-content")).getText()).includes("Added outside the app."), { timeout: 8000, timeoutMsg: "The open file did not update" });
  check("open file follows a change on disk", true);

  // A draft survives one, and the app says so.
  await app.execute(() => {
    const view = document.querySelector(".cm-content");
    view.focus();
    document.execCommand("insertText", false, "My draft ");
  });
  await app.waitUntil(async () => (await (await app.$(".cm-content")).getText()).includes("My draft"), { timeout: 5000 });
  fs.appendFileSync(path.join(folder, "notes.md"), "Changed again.\n");
  await app.$("//*[contains(., 'The file changed on disk')]").waitForExist({ timeout: 8000 });
  check("draft kept when its file changes", (await (await app.$(".cm-content")).getText()).includes("My draft"));
  await shot("draft");

  // File operations, through the same commands the tree uses.
  const mutate = (action) => invoke("workbench_studio_mutate", { request: { workspaceId, operationId: `e2e-${crypto.randomUUID()}`, ...action } });
  await mutate({ action: "createFolder", path: "archive" });
  await mutate({ action: "moveEntry", from: "code/run.do", to: "archive/run.do" });
  check("create folder and move", fs.existsSync(path.join(folder, "archive/run.do")) && !fs.existsSync(path.join(folder, "code/run.do")));
  let collision = "";
  fs.writeFileSync(path.join(folder, "other.do"), "other");
  await mutate({ action: "moveEntry", from: "other.do", to: "archive/run.do" }).catch(error => { collision = String(error); });
  check("move never replaces a file", collision.includes("Already exists") && fs.readFileSync(path.join(folder, "archive/run.do"), "utf8").includes("regress"), collision.slice(0, 160));
  await app.waitUntil(async () => (await tree("archive")).isExisting(), { timeout: 8000, timeoutMsg: "The tree did not show a folder made elsewhere" });
  check("tree follows the folder", true);
  const started2 = Date.now();
  await mutate({ action: "trashEntry", path: "other.do" });
  check("Trash moves without a prompt", !fs.existsSync(path.join(folder, "other.do")) && Date.now() - started2 < 5000, `${Date.now() - started2} ms`);
  await shot("operations");

  // Search and the programs Pipeline will not open.
  const found = await invoke("workbench_file_search", { request: { workspaceId, checkpointId: null, query: "regress wage" } });
  check("search inside files", found.matches.some(m => m.path === "archive/run.do" && m.line === 1), JSON.stringify(found.matches[0]));
  fs.writeFileSync(path.join(folder, "run.command"), "echo unsafe\n");
  const blocked = await invoke("workbench_file_external", { request: { workspaceId, path: "run.command", checkpointId: null, revisionId: null } }).then(() => "opened", error => String(error));
  check("programs never open externally", blocked.includes("will not run"), blocked.slice(0, 70));
  const plugin = await invoke("plugin:shell|open", { path: path.join(folder, "figure.png") }).then(() => "opened", error => String(error));
  check("the shell plugin refuses local paths from JS, as diagnosed", plugin !== "opened", plugin.slice(0, 90));
  // Opt-in: the system viewer covers this window, and WebKit stops painting a
  // covered window, so later runs would hang on the first PDF page.
  if (process.env.PIPELINE_E2E_OPEN_EXTERNALLY) {
    await invoke("workbench_file_external", { request: { workspaceId, path: "figure.png", checkpointId: null, revisionId: null } });
    check("Open externally works for a local file", true);
  }

  const failures = results.filter(r => !r.passed);
  console.log(JSON.stringify({ passed: results.length - failures.length, failed: failures.length }));
  process.exitCode = failures.length ? 1 : 0;
} catch (error) {
  await shot("failure").catch(() => undefined);
  console.error(error);
  process.exitCode = 1;
} finally {
  await app.deleteSession().catch(() => undefined);
}
