// Public-network transport for every browser request. Resolve and pin each
// destination, including redirect hops; do not inherit proxy variables.
//
// Playwright routes only the first URL of a chain, so a 3xx handed to Chromium
// would be followed outside these checks. None ever is. A navigation is answered
// with a small page that moves on to the checked target, so the next hop is a
// real navigation that comes back through this file, with that site's own URL,
// origin, and cookies. Anything else is followed here, and only within its first
// origin: the answer lands on the first URL, so a page would read another
// origin's response as its own. That includes a POST that 307 or 308 repeats,
// which a page cannot do for itself. A redirect to another origin is refused.
import { lookup } from "node:dns/promises";
import { BlockList, isIP } from "node:net";
import http from "node:http";
import https from "node:https";
import { gunzipSync, inflateSync, brotliDecompressSync } from "node:zlib";

const denied = new BlockList();
for (const [ip, bits] of [
  ["0.0.0.0", 8],
  ["10.0.0.0", 8],
  ["100.64.0.0", 10],
  ["127.0.0.0", 8],
  ["169.254.0.0", 16],
  ["172.16.0.0", 12],
  ["192.0.0.0", 24],
  ["192.0.2.0", 24],
  ["192.88.99.0", 24],
  ["192.168.0.0", 16],
  ["198.18.0.0", 15],
  ["198.51.100.0", 24],
  ["203.0.113.0", 24],
  ["224.0.0.0", 3],
])
  denied.addSubnet(ip, bits, "ipv4");
const global6 = new BlockList();
global6.addSubnet("2000::", 3, "ipv6");
const denied6 = new BlockList();
for (const [ip, bits] of [
  ["2001::", 23],
  ["2001:db8::", 32],
  ["2002::", 16],
])
  denied6.addSubnet(ip, bits, "ipv6");
export function publicAddress(address) {
  const family = isIP(address);
  return family === 4
    ? !denied.check(address, "ipv4")
    : family === 6 &&
        global6.check(address, "ipv6") &&
        !denied6.check(address, "ipv6");
}
export function publicUrl(value) {
  const url = new URL(value);
  if (
    !["https:", "http:"].includes(url.protocol) ||
    url.username ||
    url.password ||
    (url.port && !["80", "443"].includes(url.port)) ||
    value.length > 8192
  )
    throw new Error(
      "Only public HTTP/HTTPS URLs on ports 80 and 443 are supported",
    );
  return url;
}
const MAX_REDIRECTS = 5;
const REDIRECT_STATUSES = new Set([301, 302, 303, 307, 308]);
const BODY_HEADERS = [
  "content-encoding",
  "content-language",
  "content-length",
  "content-location",
  "content-type",
];

async function pinnedAddress(url, signal, resolve) {
  const host = url.hostname.replace(/^\[|\]$/g, "");
  const addresses = await Promise.race([
    resolve(host, { all: true }),
    new Promise((_, reject) =>
      signal.addEventListener(
        "abort",
        () => reject(new Error("Browser request timed out")),
        { once: true },
      ),
    ),
  ]);
  if (!addresses.length || addresses.some((a) => !publicAddress(a.address)))
    throw new Error("Private and reserved network addresses are blocked");
  return addresses.find((value) => value.family === 4) ?? addresses[0];
}

function redirectFailure(reason) {
  return Object.assign(new Error(`Browser redirect not followed: ${reason}`), {
    blockedRedirect: true,
  });
}

// Node reads a header one byte per character. Escaping each byte, as Chromium
// does, keeps a raw UTF-8 Location intact and leaves other encodings' bytes alone.
function escapedLocation(location) {
  return location.replace(
    /[\x80-\xff]/g,
    (byte) => `%${byte.charCodeAt(0).toString(16).toUpperCase()}`,
  );
}

// `location.replace` to this page's own address plus a fragment only scrolls, so
// the redirect page would stay on screen. Without the fragment it is a real
// navigation: a site that bounces to itself works, and a loop meets the limit.
function pageTarget(target, current) {
  const [url, here] = [new URL(target), new URL(current)];
  url.hash = "";
  here.hash = "";
  return url.href === here.href ? url.href : target;
}

function withCookies(header = "", cookies) {
  const pairs = new Map(
    header
      .split("; ")
      .filter(Boolean)
      .map((pair) => [pair.split("=")[0], pair]),
  );
  for (const cookie of cookies) {
    const pair = cookie.split(";")[0].trim();
    pairs.set(pair.split("=")[0], pair);
  }
  return [...pairs.values()].join("; ");
}

// Method and body change as they do in browsers: 303 turns any unsafe method
// into GET, 301 and 302 turn POST into GET, and 307 and 308 repeat the request.
function nextHop(hop, { status, location, cookies }) {
  const url = new URL(location, hop.url);
  url.hash ||= new URL(hop.url).hash;
  const toGet =
    (status === 303 && !["GET", "HEAD"].includes(hop.method)) ||
    ([301, 302].includes(status) && hop.method === "POST");
  const headers = {};
  for (const [key, value] of Object.entries(hop.headers)) {
    const name = key.toLowerCase();
    if (!toGet || !BODY_HEADERS.includes(name)) headers[name] = value;
  }
  if (cookies.length) headers.cookie = withCookies(headers.cookie, cookies);
  return {
    url: url.href,
    method: toGet ? "GET" : hop.method,
    headers,
    body: toGet ? undefined : hop.body,
  };
}

// Every hop here shares one origin, so Chromium files these cookies correctly.
function withHopCookies(response, cookies) {
  const kept = [...cookies, response.headers["set-cookie"]].filter(Boolean);
  if (!kept.length) return response;
  return {
    ...response,
    headers: { ...response.headers, "set-cookie": kept.join("\n") },
  };
}

// `resourceType` is Playwright's. Left out, a request gets the strictest rule.
export async function publicRequest(
  value,
  { method = "GET", headers = {}, body, resourceType } = {},
  { lookup: resolve = lookup, agent = false } = {},
) {
  if (body?.length > 2 * 1024 * 1024)
    throw new Error("Browser upload exceeds 2 MiB");
  const signal = AbortSignal.timeout(25000);
  let url = publicUrl(value);
  let address = await pinnedAddress(url, signal, resolve);
  const firstOrigin = url.origin;
  const cookies = [];
  let hop = { url: value, method, headers, body };
  for (let redirects = 0; ; redirects++) {
    const response = await send(url, address, hop, signal, agent);
    if (response.location === undefined)
      return withHopCookies(response, cookies);
    if (redirects === MAX_REDIRECTS)
      throw redirectFailure(`more than ${MAX_REDIRECTS} redirects`);
    cookies.push(...response.cookies);
    try {
      hop = nextHop(hop, response);
      url = publicUrl(hop.url);
      address = await pinnedAddress(url, signal, resolve);
    } catch (error) {
      throw redirectFailure(error.message);
    }
    if (resourceType === "document" && hop.method === "GET")
      return { redirect: pageTarget(hop.url, value), cookies };
    if (url.origin !== firstOrigin)
      throw redirectFailure("it leads to another site");
  }
}

// Carries a navigation to its next hop. The target comes from a hostile server,
// so it is checked here as well as by the caller, and encoded for the script and
// the attribute it appears in. The fallback sits in noscript so that one redirect
// is not requested twice.
export function redirectPage(target, cookies = []) {
  publicUrl(target);
  const literal = JSON.stringify(target).replace(/</g, "\\u003c");
  const attribute = target.replace(
    /[&<>"']/g,
    (character) => `&#${character.charCodeAt(0)};`,
  );
  const headers = {
    "content-type": "text/html; charset=utf-8",
    "cache-control": "no-store",
  };
  if (cookies.length) headers["set-cookie"] = cookies.join("\n");
  return {
    status: 200,
    headers,
    body: Buffer.from(
      `<!doctype html><meta charset="utf-8"><title>Redirecting</title><noscript><meta http-equiv="refresh" content="0;url=${attribute}"></noscript><script>location.replace(${literal})</script>`,
    ),
  };
}

// Redirect pages served in a row, so a loop ends in a failure page. Counted per
// frame: a page keeps one main frame across navigations, which makes this a
// per-page limit there, and embedded frames cannot spend the page's allowance.
export class RedirectChains {
  #frames = new WeakMap();

  // Counts one more redirect page at `url`. False once the limit is spent. The
  // first request of a new window has no frame yet, and is not counted.
  advance(frame, url) {
    if (!frame) return true;
    const chain = this.#frames.get(frame);
    if (!chain?.hops)
      this.#frames.set(frame, { hops: 1, pages: new Set([url]) });
    else if (chain.hops === MAX_REDIRECTS) return false;
    else {
      chain.hops++;
      chain.pages.add(url);
    }
    return true;
  }

  // A real answer at `url` ends the chain. Earlier pages stay marked, because
  // the frame still shows one of them until that answer commits.
  finish(frame, url) {
    const chain = frame && this.#frames.get(frame);
    if (!chain) return;
    chain.hops = 0;
    chain.pages.delete(url);
  }

  // The chain failed with nothing left to commit.
  abandon(frame) {
    if (frame) this.#frames.delete(frame);
  }

  redirecting(frame, url) {
    return this.#frames.get(frame)?.pages.has(url) ?? false;
  }
}

function frameOf(request) {
  try {
    return request.frame();
  } catch {
    return null;
  }
}

// Answers one routed request and returns the bytes it transferred.
export async function answerRoute(route, chains, network) {
  const request = route.request();
  const url = request.url();
  const resourceType = request.resourceType();
  const navigation = resourceType === "document";
  const frame = navigation ? frameOf(request) : null;
  try {
    const response = await publicRequest(
      url,
      {
        method: request.method(),
        headers: await request.allHeaders(),
        body: request.postDataBuffer(),
        resourceType,
      },
      network,
    );
    if (response.redirect === undefined) {
      if (navigation) chains.finish(frame, url);
      await route.fulfill(response);
      return response.body.length;
    }
    if (!chains.advance(frame, url))
      throw redirectFailure(`more than ${MAX_REDIRECTS} redirects`);
    const page = redirectPage(response.redirect, response.cookies);
    await route.fulfill(page);
    return page.body.length;
  } catch (error) {
    if (navigation && error.blockedRedirect) {
      // A page stopped at a redirect says why instead of a network error code.
      chains.finish(frame, url);
      await route
        .fulfill({
          status: 403,
          contentType: "text/plain; charset=utf-8",
          body: error.message,
        })
        .catch(() => {});
    } else {
      if (navigation) chains.abandon(frame);
      await route.abort("blockedbyclient").catch(() => {});
    }
    return 0;
  }
}

function send(url, address, { method, headers, body }, signal, agent) {
  const safeHeaders = { ...headers, "accept-encoding": "identity" };
  for (const key of [
    "host",
    "connection",
    "proxy-authorization",
    "proxy-connection",
  ])
    delete safeHeaders[key];
  return new Promise((resolve, reject) => {
    const request = (url.protocol === "https:" ? https : http).request(
      url,
      {
        method,
        headers: safeHeaders,
        agent,
        signal,
        lookup: (_host, options, callback) =>
          options.all
            ? callback(null, [address])
            : callback(null, address.address, address.family),
      },
      (response) => {
        // Nothing shows a redirect's own body, so it is not downloaded.
        if (
          REDIRECT_STATUSES.has(response.statusCode) &&
          response.headers.location !== undefined
        ) {
          resolve({
            status: response.statusCode,
            location: escapedLocation(response.headers.location),
            cookies: response.headers["set-cookie"] ?? [],
          });
          response.destroy();
          return;
        }
        const chunks = [];
        let size = 0;
        response.on("data", (chunk) => {
          size += chunk.length;
          if (size > 16 * 1024 * 1024) {
            response.destroy(new Error("Browser response exceeds 16 MiB"));
            return;
          }
          chunks.push(chunk);
        });
        response.on("error", reject);
        response.on("end", () => {
          try {
            let data = Buffer.concat(chunks);
            const encoding = response.headers["content-encoding"];
            const options = { maxOutputLength: 16 * 1024 * 1024 };
            if (encoding === "gzip") data = gunzipSync(data, options);
            else if (encoding === "deflate") data = inflateSync(data, options);
            else if (encoding === "br")
              data = brotliDecompressSync(data, options);
            else if (encoding && encoding !== "identity")
              throw new Error("Unsupported response compression");
            const returned = {};
            for (const [key, value] of Object.entries(response.headers))
              if (
                value !== undefined &&
                ![
                  "content-encoding",
                  "content-length",
                  "transfer-encoding",
                  "connection",
                ].includes(key)
              )
                returned[key] = Array.isArray(value) ? value.join("\n") : value;
            resolve({
              status: response.statusCode,
              headers: returned,
              body: data,
            });
          } catch (error) {
            reject(error);
          }
        });
      },
    );
    request.setTimeout(20000, () =>
      request.destroy(new Error("Browser request timed out")),
    );
    request.on("error", reject);
    request.end(body);
  });
}
