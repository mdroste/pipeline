import test from "node:test";
import assert from "node:assert/strict";
import {
  answerRoute,
  publicRequest,
  RedirectChains,
  RequestQueue,
} from "./public-request.mjs";
import {
  fakeRoute,
  fixture,
  pageTarget,
  redirect,
} from "./public-request.fixture.mjs";

// A navigation chain of `hops` redirects on one site, ending in a real page.
function chain(hops) {
  const routes = { [`paper.example/${hops}`]: { status: 200, body: "last" } };
  for (let hop = 0; hop < hops; hop++)
    routes[`paper.example/${hop}`] = redirect(`/${hop + 1}`);
  return routes;
}

// Follows redirect pages the way Chromium does, until something else answers.
async function navigate(url, network, chains = new RedirectChains()) {
  const frame = {};
  for (;;) {
    const route = fakeRoute({ url, frame });
    await answerRoute(route, chains, network);
    const target = pageTarget(route.fulfilled[0]?.body ?? "");
    if (target === undefined) return route;
    url = target;
  }
}

test("a long sign-in chain reaches its page", async (t) => {
  const { network } = await fixture(t, chain(20));
  const route = await navigate("http://paper.example/0", network);
  assert.equal(route.fulfilled[0].body.toString(), "last");
});

test("a navigation stops after twenty redirects", async (t) => {
  const { site, network } = await fixture(t, chain(21));
  const route = await navigate("http://paper.example/0", network);
  assert.equal(route.fulfilled[0].status, 403);
  assert.match(route.fulfilled[0].body, /more than 20 redirects/);
  assert.equal(site.requests.length, 21);
});

test("a repeated POST in a navigation has the navigation limit", async (t) => {
  const routes = { "paper.example/20": { status: 200, body: "saved" } };
  for (let hop = 0; hop < 20; hop++)
    routes[`paper.example/${hop}`] = redirect(`/${hop + 1}`, 307);
  const { network } = await fixture(t, routes);
  const response = await publicRequest(
    "http://paper.example/0",
    { method: "POST", resourceType: "document" },
    network,
  );
  assert.equal(response.body.toString(), "saved");
});

test("the last hop of a long chain still gets its address check", async (t) => {
  const routes = chain(19);
  routes["paper.example/19"] = redirect("http://127.0.0.1/latest/meta-data/");
  const { loopback, network } = await fixture(t, routes);
  const route = await navigate("http://paper.example/0", network);
  assert.equal(route.fulfilled[0].status, 403);
  assert.match(route.fulfilled[0].body, /not followed/);
  assert.equal(loopback.requests.length, 0);
});

test("parts of a page keep the limit of five", async (t) => {
  for (const resourceType of ["image", "script", "xhr"]) {
    const { site, network } = await fixture(t, chain(6));
    await assert.rejects(
      publicRequest("http://paper.example/0", { resourceType }, network),
      (error) => error.blockedRedirect && /more than 5/.test(error.message),
    );
    assert.equal(site.requests.length, 6);
  }
});

function held() {
  let release;
  const done = new Promise((resolve) => (release = resolve));
  return { done, release };
}
const settled = () => new Promise((resolve) => setImmediate(resolve));

test("requests past the limit wait and all finish", async () => {
  const queue = new RequestQueue(2);
  const started = [];
  const blocks = Array.from({ length: 5 }, held);
  const runs = blocks.map((block, index) =>
    queue.run(async () => {
      started.push(index);
      await block.done;
    }),
  );
  await settled();
  assert.deepEqual(started, [0, 1]);
  blocks[0].release();
  await settled();
  assert.deepEqual(started, [0, 1, 2]);
  for (const block of blocks) block.release();
  assert.deepEqual(await Promise.all(runs), [true, true, true, true, true]);
  assert.deepEqual(started, [0, 1, 2, 3, 4]);
});

test("a navigation goes ahead of waiting assets", async () => {
  const queue = new RequestQueue(1);
  const started = [];
  const first = held();
  const runs = [
    queue.run(() => first.done),
    queue.run(async () => started.push("asset")),
    queue.run(async () => started.push("navigation"), { first: true }),
  ];
  await settled();
  first.release();
  await Promise.all(runs);
  assert.deepEqual(started, ["navigation", "asset"]);
});

test("a failed request gives up its place", async () => {
  const queue = new RequestQueue(1);
  await assert.rejects(
    queue.run(async () => {
      throw new Error("failed");
    }),
  );
  assert.equal(await queue.run(async () => {}), true);
});

test("a flood of requests is refused rather than held without limit", async () => {
  const queue = new RequestQueue(1);
  const block = held();
  const runs = [queue.run(() => block.done)];
  for (let waiting = 0; waiting < 2048; waiting++)
    runs.push(queue.run(async () => {}));
  assert.equal(await queue.run(async () => {}), false);
  block.release();
  assert.ok((await Promise.all(runs)).every(Boolean));
});
