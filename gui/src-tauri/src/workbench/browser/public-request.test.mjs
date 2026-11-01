import test from "node:test";
import assert from "node:assert/strict";
import { lookup } from "node:dns/promises";
import http from "node:http";
import { connect } from "node:net";
import vm from "node:vm";
import {
  answerRoute,
  publicAddress,
  publicRequest,
  publicUrl,
  RedirectChains,
  redirectPage,
} from "./public-request.mjs";

test("public browser transport rejects local, reserved and disguised addresses", async () => {
  for (const ip of [
    "0.0.0.0",
    "127.0.0.1",
    "10.1.2.3",
    "172.16.0.1",
    "192.168.1.1",
    "169.254.169.254",
    "100.64.0.1",
    "198.18.0.1",
    "192.0.2.1",
    "192.88.99.1",
    "224.0.0.1",
    "::1",
    "fc00::1",
    "fe80::1",
    "::ffff:127.0.0.1",
    "2002:7f00:1::",
    "2001:db8::1",
  ])
    assert.equal(publicAddress(ip), false, ip);
  assert.equal(publicAddress("93.184.216.34"), true);
  assert.equal(publicAddress("192.0.43.8"), true);
  assert.equal(publicAddress("2606:4700:4700::1111"), true);
  for (const url of [
    "http://127.1/",
    "http://2130706433/",
    "http://0x7f000001/",
    "http://[::1]/",
    "http://169.254.169.254/",
  ])
    await assert.rejects(publicRequest(url), /blocked/);
});

test("browser navigation cannot select local files, protocols, credentials or extra ports", () => {
  for (const url of [
    "file:///etc/passwd",
    "javascript:alert(1)",
    "ftp://example.org/",
    "https://user:pass@example.org/",
    "http://example.org:8080/",
  ])
    assert.throws(() => publicUrl(url));
  assert.equal(publicUrl("https://example.org/a?q=1").hostname, "example.org");
});

// The real transport against local servers. Only name resolution and the final
// socket are replaced: hosts below resolve as stated, and each connection lands
// on the local server standing in for its host, so ports 80 and 443 stay in
// force and the address checks see the same answers they would in production.
const PUBLIC = [{ address: "93.184.216.34", family: 4 }];
const names = {
  "paper.example": PUBLIC,
  "mirror.example": PUBLIC,
  "rebind.example": [{ address: "127.0.0.1", family: 4 }],
  "split.example": [...PUBLIC, { address: "10.0.0.8", family: 4 }],
};

class FixtureAgent extends http.Agent {
  constructor(ports) {
    super();
    this.ports = ports;
  }
  createConnection(options, callback) {
    const port = this.ports[options.host] ?? this.ports.loopback;
    return connect(port, "127.0.0.1", callback);
  }
}

async function listen(handler) {
  const requests = [];
  const server = http.createServer((request, response) => {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => {
      const seen = {
        method: request.method,
        url: request.url,
        headers: request.headers,
        body: Buffer.concat(chunks).toString(),
      };
      requests.push(seen);
      handler(seen, response);
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return { requests, server, port: server.address().port };
}

// `routes` answers the public hosts; everything else reaches `loopback`.
async function fixture(t, routes) {
  const site = await listen((request, response) => {
    const route = routes[`${request.headers.host}${request.url}`];
    if (!route) response.writeHead(404).end("missing");
    else response.writeHead(route.status, route.headers).end(route.body ?? "");
  });
  const loopback = await listen((_request, response) =>
    response.writeHead(200).end("private-fixture"),
  );
  const agent = new FixtureAgent({
    "paper.example": site.port,
    "mirror.example": site.port,
    loopback: loopback.port,
  });
  t.after(() => {
    agent.destroy();
    site.server.close();
    loopback.server.close();
  });
  const network = {
    agent,
    lookup: async (host, options) => names[host] ?? lookup(host, options),
  };
  return { site, loopback, network };
}

const redirect = (location, status = 302, headers = {}) => ({
  status,
  headers: { location, ...headers },
});

const BLOCKED_TARGETS = [
  "http://127.0.0.1/latest/meta-data/",
  "http://169.254.169.254/latest/meta-data/",
  "http://[::1]/",
  "http://2130706433/",
  "http://rebind.example/",
  "http://split.example/",
  "file:///etc/passwd",
  "javascript:alert(1)",
  "http://mirror.example:8080/",
  "http://user:pass@mirror.example/",
];
const SUBRESOURCES = [
  "fetch",
  "xhr",
  "image",
  "script",
  "stylesheet",
  "font",
  "media",
  "eventsource",
  "other",
  undefined,
];

test("a redirect to a local or reserved address reaches nothing", async (t) => {
  for (const resourceType of ["document", "fetch"])
    for (const target of BLOCKED_TARGETS) {
      const { loopback, network } = await fixture(t, {
        "paper.example/start": redirect(target),
      });
      await assert.rejects(
        publicRequest("http://paper.example/start", { resourceType }, network),
        (error) => {
          assert.equal(error.blockedRedirect, true, target);
          assert.match(error.message, /^Browser redirect not followed/);
          return true;
        },
      );
      assert.equal(loopback.requests.length, 0, target);
    }
});

test("a blocked hop later in a same-origin chain stops there too", async (t) => {
  const { site, loopback, network } = await fixture(t, {
    "paper.example/start": redirect("/next"),
    "paper.example/next": redirect("/hop"),
    "paper.example/hop": redirect("http://127.0.0.1/"),
  });
  await assert.rejects(
    publicRequest("http://paper.example/start", {}, network),
    /blocked/,
  );
  assert.equal(site.requests.length, 3);
  assert.equal(loopback.requests.length, 0);
});

test("a same-origin redirect outside a navigation is followed to its answer", async (t) => {
  const { site, network } = await fixture(t, {
    "paper.example/logo": redirect("/assets/logo.png?v=2", 301),
    "paper.example/assets/logo.png?v=2": {
      status: 200,
      headers: { "content-type": "image/png" },
      body: "image",
    },
  });
  const response = await publicRequest(
    "http://paper.example/logo",
    { resourceType: "image" },
    network,
  );
  assert.equal(response.status, 200);
  assert.equal(response.body.toString(), "image");
  assert.equal(response.headers["content-type"], "image/png");
  assert.equal(site.requests.length, 2);
});

test("a chain that changes origin outside a navigation is refused", async (t) => {
  for (const resourceType of SUBRESOURCES)
    for (const target of [
      "http://mirror.example/report",
      "https://paper.example/start",
    ]) {
      const { site, network } = await fixture(t, {
        "paper.example/start": redirect(target),
        "mirror.example/report": { status: 200, body: "licensed" },
      });
      await assert.rejects(
        publicRequest(
          "http://paper.example/start",
          {
            headers: { cookie: "session=1", authorization: "Bearer token" },
            resourceType,
          },
          network,
        ),
        (error) => {
          assert.equal(error.blockedRedirect, true, resourceType);
          assert.match(error.message, /another site/);
          return true;
        },
      );
      assert.equal(site.requests.length, 1, resourceType);
    }
});

test("a chain cannot leave its origin and come back outside a navigation", async (t) => {
  const { site, network } = await fixture(t, {
    "paper.example/start": redirect("/hop"),
    "paper.example/hop": redirect("http://mirror.example/bounce"),
    "mirror.example/bounce": redirect("http://paper.example/end"),
    "paper.example/end": { status: 200, body: "end" },
  });
  await assert.rejects(
    publicRequest(
      "http://paper.example/start",
      { resourceType: "fetch" },
      network,
    ),
    /another site/,
  );
  assert.deepEqual(
    site.requests.map((seen) => seen.url),
    ["/start", "/hop"],
  );
});

test("relative locations resolve against the hop that sent them", async (t) => {
  const { site, network } = await fixture(t, {
    "paper.example/a/b/start": redirect("../c"),
    "paper.example/a/c": redirect("/d?e=1"),
    "paper.example/d?e=1": { status: 200, body: "done" },
  });
  const response = await publicRequest(
    "http://paper.example/a/b/start",
    {},
    network,
  );
  assert.equal(response.body.toString(), "done");
  assert.deepEqual(
    site.requests.map((seen) => seen.url),
    ["/a/b/start", "/a/c", "/d?e=1"],
  );
});

test("five redirects are followed and a sixth is refused", async (t) => {
  const chain = (hops) => {
    const routes = {
      [`paper.example/${hops}`]: { status: 200, body: "last" },
    };
    for (let hop = 0; hop < hops; hop++)
      routes[`paper.example/${hop}`] = redirect(`/${hop + 1}`);
    return routes;
  };
  const followed = await fixture(t, chain(5));
  const response = await publicRequest(
    "http://paper.example/0",
    {},
    followed.network,
  );
  assert.equal(response.body.toString(), "last");
  const refused = await fixture(t, chain(6));
  await assert.rejects(
    publicRequest("http://paper.example/0", {}, refused.network),
    (error) => error.blockedRedirect && /more than 5/.test(error.message),
  );
  assert.equal(refused.site.requests.length, 6);
});

test("method and body change the way they do in browsers", async (t) => {
  for (const [status, sent, expected, body] of [
    [301, "POST", "GET", ""],
    [302, "POST", "GET", ""],
    [303, "POST", "GET", ""],
    [303, "PUT", "GET", ""],
    [301, "PUT", "PUT", "form=1"],
    [302, "PATCH", "PATCH", "form=1"],
    [303, "GET", "GET", ""],
    [307, "POST", "POST", "form=1"],
    [308, "POST", "POST", "form=1"],
  ]) {
    const { site, network } = await fixture(t, {
      "paper.example/form": redirect("/done", status),
      "paper.example/done": { status: 200, body: "ok" },
    });
    const hasBody = sent !== "GET";
    await publicRequest(
      "http://paper.example/form",
      {
        method: sent,
        headers: hasBody
          ? { "content-type": "application/x-www-form-urlencoded" }
          : {},
        body: hasBody ? Buffer.from("form=1") : undefined,
      },
      network,
    );
    const last = site.requests.at(-1);
    assert.equal(last.url, "/done", `${status} ${sent}`);
    assert.equal(last.method, expected, `${status} ${sent}`);
    assert.equal(last.body, body, `${status} ${sent}`);
    assert.equal(
      last.headers["content-type"] !== undefined,
      body !== "",
      `${status} ${sent}`,
    );
  }
});

test("a same-origin chain keeps its credentials and collects its cookies", async (t) => {
  const { site, network } = await fixture(t, {
    "paper.example/login": redirect("/home", 307, {
      "set-cookie": ["session=new; Path=/; HttpOnly", "theme=dark"],
    }),
    "paper.example/home": {
      status: 200,
      headers: { "set-cookie": "seen=1" },
      body: "signed in",
    },
  });
  const response = await publicRequest(
    "http://paper.example/login",
    {
      method: "POST",
      headers: {
        authorization: "Bearer token",
        cookie: "session=old; lang=en",
      },
    },
    network,
  );
  const last = site.requests.at(-1);
  assert.equal(last.headers.authorization, "Bearer token");
  assert.equal(last.headers.cookie, "session=new; lang=en; theme=dark");
  assert.deepEqual(response.headers["set-cookie"].split("\n"), [
    "session=new; Path=/; HttpOnly",
    "theme=dark",
    "seen=1",
  ]);
});

test("a response with no redirect is returned as it arrived", async (t) => {
  const { site, network } = await fixture(t, {
    "paper.example/page": {
      status: 200,
      headers: {
        "set-cookie": "a=1",
        "content-type": "text/plain",
        "content-security-policy": "default-src 'self'",
      },
      body: "plain",
    },
    "paper.example/unchanged": { status: 304 },
    "paper.example/no-location": { status: 302 },
  });
  const response = await publicRequest(
    "http://paper.example/page",
    {},
    network,
  );
  assert.equal(response.headers["set-cookie"], "a=1");
  assert.equal(
    response.headers["content-security-policy"],
    "default-src 'self'",
  );
  for (const [path, status] of [
    ["unchanged", 304],
    ["no-location", 302],
  ]) {
    const answer = await publicRequest(
      `http://paper.example/${path}`,
      {},
      network,
    );
    assert.equal(answer.status, status);
  }
  assert.equal(site.requests.length, 3);
});

// Stands in for a Playwright route. It refuses the one thing the route handler
// must never do, however a test gets there.
function fakeRoute({
  url,
  resourceType = "document",
  method = "GET",
  headers = {},
  body = null,
  frame = {},
}) {
  const route = {
    fulfilled: [],
    aborted: [],
    request: () => ({
      url: () => url,
      method: () => method,
      allHeaders: async () => headers,
      postDataBuffer: () => body,
      resourceType: () => resourceType,
      frame: () => {
        if (frame === null) throw new Error("Frame is not available");
        return frame;
      },
    }),
    fulfill: async (response) => {
      const redirected =
        [301, 302, 303, 307, 308].includes(response.status) &&
        response.headers?.location !== undefined;
      assert.equal(redirected, false, "a 3xx reached Chromium");
      route.fulfilled.push(response);
    },
    abort: async (code) => route.aborted.push(code),
  };
  return route;
}

function pageTarget(body) {
  const script = /<script>(.*)<\/script>/s.exec(body.toString());
  if (!script) return undefined;
  const moves = [];
  vm.runInNewContext(script[1], {
    location: { replace: (to) => moves.push(to) },
  });
  assert.equal(moves.length, 1);
  return moves[0];
}

// What Chromium does with a redirect page: files its cookies under its own URL,
// runs its script, and sends the next navigation back through the route with
// the cookies it holds for that host.
async function browse(start, network) {
  const chains = new RedirectChains();
  const frame = {};
  const jar = {};
  const visited = [];
  let url = start;
  for (let step = 0; step < 20; step++) {
    const host = new URL(url).host;
    const cookie = Object.entries(jar[host] ?? {})
      .map(([name, value]) => `${name}=${value}`)
      .join("; ");
    const route = fakeRoute({
      url,
      frame,
      headers: cookie ? { cookie } : {},
    });
    await answerRoute(route, chains, network);
    assert.equal(route.fulfilled.length, 1, url);
    const answer = route.fulfilled[0];
    const setCookie = answer.headers?.["set-cookie"];
    for (const line of setCookie ? setCookie.split("\n") : []) {
      const [name, value] = line.split(";")[0].split("=");
      (jar[host] ??= {})[name] = value;
    }
    visited.push(url);
    const target = pageTarget(answer.body);
    if (target === undefined) return { answer, url, visited, jar };
    url = target;
  }
  throw new Error("The chain never ended");
}

test("a DOI link arrives at the publisher with its own address and cookies", async (t) => {
  const { site, network } = await fixture(t, {
    "paper.example/10.1000/182": redirect(
      "http://mirror.example/article",
      302,
      {
        "set-cookie": "resolver=1",
      },
    ),
    "mirror.example/article": {
      status: 200,
      headers: { "set-cookie": "publisher=2", "content-type": "text/html" },
      body: "<p>The article</p>",
    },
  });
  const { answer, url, jar } = await browse(
    "http://paper.example/10.1000/182",
    network,
  );
  assert.equal(url, "http://mirror.example/article");
  assert.equal(answer.status, 200);
  assert.equal(answer.body.toString(), "<p>The article</p>");
  assert.deepEqual(jar, {
    "paper.example": { resolver: "1" },
    "mirror.example": { publisher: "2" },
  });
  assert.deepEqual(
    site.requests.map((seen) => seen.headers.host),
    ["paper.example", "mirror.example"],
  );
  assert.equal(site.requests[1].headers.cookie, undefined);
});

test("leaving a site and coming back keeps that site's cookies", async (t) => {
  const { site, network } = await fixture(t, {
    "paper.example/start": redirect("http://mirror.example/bounce", 302, {
      "set-cookie": "visit=1",
    }),
    "mirror.example/bounce": redirect("http://paper.example/end"),
    "paper.example/end": { status: 200, body: "back" },
  });
  const { answer, url } = await browse("http://paper.example/start", network);
  assert.equal(url, "http://paper.example/end");
  assert.equal(answer.body.toString(), "back");
  assert.equal(site.requests.at(-1).headers.cookie, "visit=1");
});

test("a same-origin navigation reaches its real path", async (t) => {
  const { network } = await fixture(t, {
    "paper.example/": redirect("/en/home", 301),
    "paper.example/en/home": { status: 200, body: "<a href=about>About</a>" },
  });
  const { url, visited } = await browse("http://paper.example/", network);
  assert.equal(url, "http://paper.example/en/home");
  assert.equal(visited.length, 2);
});

test("a fragment survives a redirect the way it does in a browser", async (t) => {
  const { network } = await fixture(t, {
    "paper.example/a": redirect("/b"),
    "paper.example/c": redirect("/d#own"),
  });
  const moved = fakeRoute({ url: "http://paper.example/a#section" });
  await answerRoute(moved, new RedirectChains(), network);
  assert.equal(
    pageTarget(moved.fulfilled[0].body),
    "http://paper.example/b#section",
  );
  const own = fakeRoute({ url: "http://paper.example/c#section" });
  await answerRoute(own, new RedirectChains(), network);
  assert.equal(pageTarget(own.fulfilled[0].body), "http://paper.example/d#own");
});

test("a raw UTF-8 Location keeps its bytes", async (t) => {
  const raw = Buffer.from("/caf\u00e9/\u6587?q=\u00fc").toString("latin1");
  const escaped = "/caf%C3%A9/%E6%96%87?q=%C3%BC";
  const { site, network } = await fixture(t, {
    "paper.example/old": redirect(raw),
    [`paper.example${escaped}`]: { status: 200, body: "found" },
  });
  const followed = await publicRequest(
    "http://paper.example/old",
    { resourceType: "image" },
    network,
  );
  assert.equal(followed.body.toString(), "found");
  assert.equal(site.requests.at(-1).url, escaped);
  const route = fakeRoute({ url: "http://paper.example/old" });
  await answerRoute(route, new RedirectChains(), network);
  assert.equal(
    pageTarget(route.fulfilled[0].body),
    `http://paper.example${escaped}`,
  );
});

test("a Location in another encoding keeps its bytes too", async (t) => {
  const { site, network } = await fixture(t, {
    "paper.example/old": redirect("/caf\u00e9"),
    "paper.example/caf%E9": { status: 200, body: "found" },
  });
  await publicRequest("http://paper.example/old", {}, network);
  assert.equal(site.requests.at(-1).url, "/caf%E9");
});

test("a redirect to the same page with a fragment leaves the redirect page", async (t) => {
  for (const [from, location] of [
    ["http://paper.example/a", "#top"],
    ["http://paper.example/a", "/a#top"],
    ["http://paper.example/a", "http://paper.example/a#"],
    ["http://paper.example/a#section", "#top"],
    ["http://paper.example/a#section", "/a"],
  ]) {
    const { network } = await fixture(t, {
      "paper.example/a": redirect(location),
    });
    const route = fakeRoute({ url: from });
    await answerRoute(route, new RedirectChains(), network);
    // No fragment: the browser requests the page again instead of scrolling.
    assert.equal(
      pageTarget(route.fulfilled[0].body),
      "http://paper.example/a",
      `${from} to ${location}`,
    );
  }
});

test("a site that bounces to its own address reaches the page", async (t) => {
  let visits = 0;
  const site = await listen((_request, response) => {
    visits++;
    if (visits === 1) response.writeHead(302, { location: "#top" }).end();
    else response.writeHead(200).end("home");
  });
  const agent = new FixtureAgent({ "paper.example": site.port });
  t.after(() => {
    agent.destroy();
    site.server.close();
  });
  const network = { agent, lookup: async (host) => names[host] };
  const { answer, visited } = await browse("http://paper.example/a", network);
  assert.equal(answer.body.toString(), "home");
  assert.deepEqual(visited, [
    "http://paper.example/a",
    "http://paper.example/a",
  ]);
});

test("a fragment-only redirect loop ends in the plain failure page", async (t) => {
  const { site, network } = await fixture(t, {
    "paper.example/a": redirect("#again"),
  });
  const chains = new RedirectChains();
  const frame = {};
  const url = "http://paper.example/a";
  for (let hop = 0; hop < 5; hop++) {
    const route = fakeRoute({ url, frame });
    await answerRoute(route, chains, network);
    assert.equal(pageTarget(route.fulfilled[0].body), url);
  }
  const route = fakeRoute({ url, frame });
  await answerRoute(route, chains, network);
  assert.equal(route.fulfilled[0].status, 403);
  assert.match(route.fulfilled[0].body, /more than 5 redirects/);
  assert.equal(chains.redirecting(frame, url), false);
  assert.equal(site.requests.length, 6);
});

test("a different page keeps its fragment", async (t) => {
  const { network } = await fixture(t, {
    "paper.example/a": redirect("/b#top"),
  });
  const route = fakeRoute({ url: "http://paper.example/a" });
  await answerRoute(route, new RedirectChains(), network);
  assert.equal(
    pageTarget(route.fulfilled[0].body),
    "http://paper.example/b#top",
  );
});

test("a redirect page refuses anything but a public web address", () => {
  for (const target of [
    "javascript:alert(1)",
    "JaVaScRiPt:alert(1)",
    " javascript:alert(1)",
    "java\nscript:alert(1)",
    "\tjavascript:alert(1)",
    "data:text/html,<script>alert(1)</script>",
    "blob:https://paper.example/1",
    "file:///etc/passwd",
    "vbscript:msgbox(1)",
    "about:blank",
    "ftp://paper.example/",
    "/relative",
    "#fragment",
    "",
    "https://user:pass@paper.example/",
    "https://paper.example:8443/",
  ])
    assert.throws(() => redirectPage(target), target);
  for (const target of [
    "http://paper.example/",
    "https://paper.example/a?b=1#c",
    "HTTPS://paper.example/",
  ])
    assert.equal(redirectPage(target).status, 200, target);
});

test("a navigation stops at its first redirect without touching the target", async (t) => {
  const { site, network } = await fixture(t, {
    "paper.example/start": redirect("http://mirror.example/next"),
    "mirror.example/next": { status: 200, body: "next" },
  });
  const response = await publicRequest(
    "http://paper.example/start",
    { resourceType: "document" },
    network,
  );
  assert.deepEqual(response, {
    redirect: "http://mirror.example/next",
    cookies: [],
  });
  assert.equal(site.requests.length, 1);
});

test("a redirect page carries a hostile target as data", async () => {
  for (const target of [
    "https://paper.example/</script><script>globalThis.owned=1</script>",
    'https://paper.example/"><img src=x onerror=alert(1)>',
    "https://paper.example/'+alert(1)+'",
    "https://paper.example/?a=1&b=2;c",
    "https://paper.example/ line",
    "https://paper.example/\\",
  ]) {
    const page = redirectPage(target);
    const body = page.body.toString();
    assert.equal(body.match(/<script/g).length, 1, target);
    assert.equal(body.match(/<\/script/g).length, 1, target);
    assert.equal(body.match(/<meta/g).length, 2, target);
    assert.equal(body.match(/<img/g), null, target);
    const context = { location: { replace: () => {} } };
    const moves = [];
    context.location.replace = (to) => moves.push(to);
    vm.runInNewContext(/<script>(.*)<\/script>/s.exec(body)[1], context);
    assert.deepEqual(moves, [target]);
    assert.equal(context.owned, undefined);
    const fallback = /content="0;url=([^"]*)"/.exec(body)[1];
    const decoded = fallback.replace(/&#(\d+);/g, (_, code) =>
      String.fromCharCode(Number(code)),
    );
    assert.equal(decoded, target);
  }
});

test("a hostile Location cannot break out of the page it is written into", async (t) => {
  const hostile =
    "http://mirror.example/x</script><script>globalThis.owned=1</script>";
  const { network } = await fixture(t, {
    "paper.example/start": redirect(hostile),
  });
  const route = fakeRoute({ url: "http://paper.example/start" });
  await answerRoute(route, new RedirectChains(), network);
  const page = route.fulfilled[0];
  assert.equal(page.status, 200);
  assert.match(page.headers["content-type"], /^text\/html/);
  assert.equal(page.body.toString().match(/<script/g).length, 1);
  assert.equal(pageTarget(page.body), new URL(hostile).href);
});

test("a redirect loop across redirect pages ends in the plain failure page", async (t) => {
  const { site, network } = await fixture(t, {
    "paper.example/a": redirect("http://mirror.example/b"),
    "mirror.example/b": redirect("http://paper.example/a"),
  });
  const chains = new RedirectChains();
  const frame = {};
  let url = "http://paper.example/a";
  for (let hop = 0; hop < 5; hop++) {
    const route = fakeRoute({ url, frame });
    await answerRoute(route, chains, network);
    url = pageTarget(route.fulfilled[0].body);
    assert.ok(url, `hop ${hop} should still redirect`);
  }
  const route = fakeRoute({ url, frame });
  await answerRoute(route, chains, network);
  const [failure] = route.fulfilled;
  assert.equal(failure.status, 403);
  assert.match(failure.contentType, /^text\/plain/);
  assert.match(failure.body, /more than 5 redirects/);
  assert.equal(site.requests.length, 6);
  // The failure page ended the chain, so the frame can navigate again.
  const again = fakeRoute({ url: "http://paper.example/a", frame });
  await answerRoute(again, chains, network);
  assert.ok(pageTarget(again.fulfilled[0].body));
});

test("a real page resets the limit for the next chain", async (t) => {
  const routes = { "mirror.example/end": { status: 200, body: "end" } };
  for (let hop = 0; hop < 5; hop++)
    routes[`paper.example/${hop}`] = redirect(
      hop === 4 ? "http://mirror.example/end" : `/${hop + 1}`,
    );
  const { network } = await fixture(t, routes);
  const chains = new RedirectChains();
  const frame = {};
  let url = "http://paper.example/0";
  for (let visit = 0; visit < 3; visit++) {
    for (;;) {
      const route = fakeRoute({ url, frame });
      await answerRoute(route, chains, network);
      assert.equal(route.fulfilled[0].status, 200, `visit ${visit}`);
      const target = pageTarget(route.fulfilled[0].body);
      if (target === undefined) break;
      url = target;
    }
    url = "http://paper.example/0";
  }
});

test("each frame has its own redirect limit, and a new window can start", async (t) => {
  const { network } = await fixture(t, {
    "paper.example/a": redirect("/a"),
  });
  const chains = new RedirectChains();
  const main = {};
  const embedded = {};
  for (let hop = 0; hop < 5; hop++)
    for (const frame of [main, embedded]) {
      const route = fakeRoute({ url: "http://paper.example/a", frame });
      await answerRoute(route, chains, network);
      assert.equal(route.fulfilled[0].status, 200);
    }
  const spent = fakeRoute({ url: "http://paper.example/a", frame: main });
  await answerRoute(spent, chains, network);
  assert.equal(spent.fulfilled[0].status, 403);
  const untouched = fakeRoute({ url: "http://paper.example/a", frame: {} });
  await answerRoute(untouched, chains, network);
  assert.equal(untouched.fulfilled[0].status, 200);
  const newWindow = fakeRoute({ url: "http://paper.example/a", frame: null });
  await answerRoute(newWindow, chains, network);
  assert.ok(pageTarget(newWindow.fulfilled[0].body));
});

test("a redirect page stays marked until the page it leads to takes over", async (t) => {
  const { network } = await fixture(t, {
    "paper.example/a": redirect("/b"),
    "paper.example/b": redirect("http://mirror.example/c"),
    "mirror.example/c": { status: 200, body: "c" },
  });
  const chains = new RedirectChains();
  const frame = {};
  const visit = async (url) => {
    const route = fakeRoute({ url, frame });
    await answerRoute(route, chains, network);
    return route;
  };
  const [a, b, c] = [
    "http://paper.example/a",
    "http://paper.example/b",
    "http://mirror.example/c",
  ];
  assert.equal(chains.redirecting(frame, a), false);
  await visit(a);
  assert.equal(chains.redirecting(frame, a), true);
  await visit(b);
  await visit(c);
  // The answer has been served, but the frame still shows a redirect page
  // until it commits, so a snapshot must keep waiting.
  assert.equal(chains.redirecting(frame, a), true);
  assert.equal(chains.redirecting(frame, b), true);
  assert.equal(chains.redirecting(frame, c), false);
  await visit(a);
  assert.equal(chains.redirecting(frame, b), false, "a new chain starts clean");
});

test("coming back to a redirect page's own address as a real page is not a redirect", async (t) => {
  let visits = 0;
  const site = await listen((_request, response) => {
    visits++;
    if (visits === 1)
      response.writeHead(302, { location: "/set-cookie" }).end();
    else if (visits === 2) response.writeHead(302, { location: "/" }).end();
    else response.writeHead(200).end("home");
  });
  const agent = new FixtureAgent({ "paper.example": site.port });
  t.after(() => {
    agent.destroy();
    site.server.close();
  });
  const network = { agent, lookup: async (host) => names[host] };
  const chains = new RedirectChains();
  const frame = {};
  const home = "http://paper.example/";
  for (const url of [home, "http://paper.example/set-cookie", home]) {
    const route = fakeRoute({ url, frame });
    await answerRoute(route, chains, network);
  }
  assert.equal(chains.redirecting(frame, home), false);
});

test("a failed hop leaves nothing for a snapshot to wait on", async (t) => {
  const site = await listen((_request, response) =>
    response.writeHead(302, { location: "/gone" }).end(),
  );
  const agent = new FixtureAgent({ "paper.example": site.port });
  t.after(() => {
    agent.destroy();
    site.server.close();
  });
  const network = { agent, lookup: async (host) => names[host] };
  const chains = new RedirectChains();
  const frame = {};
  const first = "http://paper.example/start";
  await answerRoute(fakeRoute({ url: first, frame }), chains, network);
  assert.equal(chains.redirecting(frame, first), true);
  const failed = fakeRoute({ url: "http://paper.example/gone", frame });
  await answerRoute(failed, chains, {
    ...network,
    agent: new FixtureAgent({ "paper.example": 1 }),
  });
  assert.deepEqual(failed.aborted, ["blockedbyclient"]);
  assert.equal(chains.redirecting(frame, first), false);
});

test("a form POST that 302s or 303s moves on as a GET page", async (t) => {
  for (const status of [301, 302, 303]) {
    const { site, network } = await fixture(t, {
      "paper.example/search": redirect(
        "http://mirror.example/results",
        status,
        {
          "set-cookie": "query=1",
        },
      ),
    });
    const route = fakeRoute({
      url: "http://paper.example/search",
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: Buffer.from("q=1"),
    });
    await answerRoute(route, new RedirectChains(), network);
    const [page] = route.fulfilled;
    assert.equal(pageTarget(page.body), "http://mirror.example/results");
    assert.equal(page.headers["set-cookie"], "query=1");
    assert.equal(site.requests.length, 1);
  }
});

test("a POST that 307 or 308 repeats follows in Node within its origin", async (t) => {
  for (const status of [307, 308]) {
    const { site, network } = await fixture(t, {
      "paper.example/form": redirect("/saved", status),
      "paper.example/saved": { status: 200, body: "saved" },
    });
    const route = fakeRoute({
      url: "http://paper.example/form",
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: Buffer.from("form=1"),
    });
    await answerRoute(route, new RedirectChains(), network);
    assert.equal(route.fulfilled[0].body.toString(), "saved");
    const last = site.requests.at(-1);
    assert.equal(last.method, "POST");
    assert.equal(last.body, "form=1");
  }
});

test("a POST that 307 or 308 sends to another origin stops with a failure page", async (t) => {
  for (const status of [307, 308]) {
    const { site, network } = await fixture(t, {
      "paper.example/form": redirect("http://mirror.example/saved", status),
      "mirror.example/saved": { status: 200, body: "saved" },
    });
    const route = fakeRoute({
      url: "http://paper.example/form",
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: Buffer.from("form=1"),
    });
    await answerRoute(route, new RedirectChains(), network);
    assert.equal(route.fulfilled[0].status, 403);
    assert.match(route.fulfilled[0].body, /another site/);
    assert.equal(site.requests.length, 1);
  }
});

test("a repeated POST that later redirects with GET moves on as a page", async (t) => {
  const { site, network } = await fixture(t, {
    "paper.example/form": redirect("/step", 307, { "set-cookie": "a=1" }),
    "paper.example/step": redirect("http://mirror.example/done", 303),
  });
  const route = fakeRoute({
    url: "http://paper.example/form",
    method: "POST",
    body: Buffer.from("form=1"),
  });
  await answerRoute(route, new RedirectChains(), network);
  const [page] = route.fulfilled;
  assert.equal(pageTarget(page.body), "http://mirror.example/done");
  assert.equal(page.headers["set-cookie"], "a=1");
  assert.equal(site.requests.length, 2);
});

test("a navigation to a blocked target gets the failure page and reaches nothing", async (t) => {
  for (const target of BLOCKED_TARGETS) {
    const { loopback, network } = await fixture(t, {
      "paper.example/start": redirect(target),
    });
    const route = fakeRoute({ url: "http://paper.example/start" });
    assert.equal(
      await answerRoute(route, new RedirectChains(), network),
      0,
      target,
    );
    const [failure] = route.fulfilled;
    assert.equal(failure.status, 403, target);
    assert.match(failure.contentType, /^text\/plain/);
    assert.match(failure.body, /^Browser redirect not followed/);
    assert.equal(loopback.requests.length, 0, target);
  }
});

test("a subresource stopped at a redirect is aborted, not answered", async (t) => {
  for (const resourceType of SUBRESOURCES.filter(Boolean)) {
    const { site, loopback, network } = await fixture(t, {
      "paper.example/r": redirect("http://mirror.example/secret"),
      "paper.example/local": redirect("http://127.0.0.1/"),
      "mirror.example/secret": { status: 200, body: "licensed" },
    });
    for (const url of [
      "http://paper.example/r",
      "http://paper.example/local",
    ]) {
      const route = fakeRoute({ url, resourceType, frame: null });
      await answerRoute(route, new RedirectChains(), network);
      assert.deepEqual(route.fulfilled, [], resourceType);
      assert.deepEqual(route.aborted, ["blockedbyclient"], resourceType);
    }
    assert.equal(site.requests.length, 2, resourceType);
    assert.equal(loopback.requests.length, 0, resourceType);
  }
});

test("a route reports the bytes it served", async (t) => {
  const { network } = await fixture(t, {
    "paper.example/page": { status: 200, body: "12345" },
    "paper.example/start": redirect("/page"),
  });
  const chains = new RedirectChains();
  const page = fakeRoute({ url: "http://paper.example/page" });
  assert.equal(await answerRoute(page, chains, network), 5);
  const stub = fakeRoute({ url: "http://paper.example/start" });
  assert.equal(
    await answerRoute(stub, chains, network),
    stub.fulfilled[0].body.length,
  );
});
