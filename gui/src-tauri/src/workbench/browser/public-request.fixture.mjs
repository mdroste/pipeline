// Test fixtures for the browser's public-network transport.
import assert from "node:assert/strict";
import { lookup } from "node:dns/promises";
import http from "node:http";
import { connect } from "node:net";
import vm from "node:vm";

// The real transport against local servers. Only name resolution and the final
// socket are replaced: hosts below resolve as stated, and each connection lands
// on the local server standing in for its host, so ports 80 and 443 stay in
// force and the address checks see the same answers they would in production.
const PUBLIC = [{ address: "93.184.216.34", family: 4 }];
export const names = {
  "paper.example": PUBLIC,
  "mirror.example": PUBLIC,
  "rebind.example": [{ address: "127.0.0.1", family: 4 }],
  "split.example": [...PUBLIC, { address: "10.0.0.8", family: 4 }],
};

export class FixtureAgent extends http.Agent {
  constructor(ports) {
    super();
    this.ports = ports;
  }
  createConnection(options, callback) {
    const port = this.ports[options.host] ?? this.ports.loopback;
    return connect(port, "127.0.0.1", callback);
  }
}

export async function listen(handler) {
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
export async function fixture(t, routes) {
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

export const redirect = (location, status = 302, headers = {}) => ({
  status,
  headers: { location, ...headers },
});

// Stands in for a Playwright route. It refuses the one thing the route handler
// must never do, however a test gets there.
export function fakeRoute({
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

export function pageTarget(body) {
  const script = /<script>(.*)<\/script>/s.exec(body.toString());
  if (!script) return undefined;
  const moves = [];
  vm.runInNewContext(script[1], {
    location: { replace: (to) => moves.push(to) },
  });
  assert.equal(moves.length, 1);
  return moves[0];
}
