// Public-network transport for every browser request. Resolve and pin each
// destination, including redirect hops; do not inherit proxy variables.
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
export async function publicRequest(
  value,
  { method = "GET", headers = {}, body } = {},
) {
  const url = publicUrl(value);
  const host = url.hostname.replace(/^\[|\]$/g, "");
  const signal = AbortSignal.timeout(25000);
  const addresses = await Promise.race([
    lookup(host, { all: true }),
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
  const address = addresses.find((value) => value.family === 4) ?? addresses[0];
  const safeHeaders = { ...headers, "accept-encoding": "identity" };
  for (const key of [
    "host",
    "connection",
    "proxy-authorization",
    "proxy-connection",
  ])
    delete safeHeaders[key];
  if (body?.length > 2 * 1024 * 1024)
    throw new Error("Browser upload exceeds 2 MiB");
  return new Promise((resolve, reject) => {
    const request = (url.protocol === "https:" ? https : http).request(
      url,
      {
        method,
        headers: safeHeaders,
        agent: false,
        signal,
        lookup: (_host, options, callback) =>
          options.all
            ? callback(null, [address])
            : callback(null, address.address, address.family),
      },
      (response) => {
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
