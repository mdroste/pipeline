import test from "node:test";
import assert from "node:assert/strict";
import { publicAddress, publicUrl, publicRequest } from "./public-request.mjs";

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
