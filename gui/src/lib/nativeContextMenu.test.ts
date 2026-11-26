import { afterEach, beforeEach, expect, test } from "vitest";
import { installContextMenuFilter, keepsNativeMenu } from "./nativeContextMenu";

let uninstall = () => {};

beforeEach(() => {
  document.body.innerHTML = `
    <main id="page">
      <p id="prose">Findings about <b id="bold">the estimate</b></p>
      <a id="link" href="https://example.org">source</a>
      <img id="image" alt="" src="data:image/gif;base64,R0lGODlhAQABAAAAACw=" />
      <input id="text" type="text" />
      <input id="check" type="checkbox" />
      <textarea id="area"></textarea>
      <div id="editor" contenteditable="true"><span id="line">x = 1</span></div>
      <div id="readonly" contenteditable="false">y = 2</div>
      <div data-native-menu><span id="optin">keep</span></div>
    </main>`;
  uninstall = installContextMenuFilter(document, false);
});

afterEach(() => {
  uninstall();
  document.getSelection()?.removeAllRanges();
});

const rightClick = (id: string, init: MouseEventInit = {}) => {
  const event = new MouseEvent("contextmenu", {
    bubbles: true,
    cancelable: true,
    ...init,
  });
  document.getElementById(id)!.dispatchEvent(event);
  return event.defaultPrevented;
};

test("suppresses the browser page, link, and image menus", () => {
  expect(rightClick("page")).toBe(true);
  expect(rightClick("prose")).toBe(true);
  expect(rightClick("link")).toBe(true);
  expect(rightClick("image")).toBe(true);
  expect(rightClick("check")).toBe(true);
  expect(rightClick("readonly")).toBe(true);
});

test("keeps the native menu in text fields and editors", () => {
  expect(rightClick("text")).toBe(false);
  expect(rightClick("area")).toBe(false);
  expect(rightClick("editor")).toBe(false);
  expect(rightClick("line")).toBe(false);
  expect(rightClick("optin")).toBe(false);
});

test("keeps the native menu over selected text only", () => {
  const range = document.createRange();
  range.selectNodeContents(document.getElementById("bold")!);
  document.getSelection()!.addRange(range);
  expect(rightClick("bold")).toBe(false);
  expect(rightClick("link")).toBe(true);
});

test("leaves menus that a component already handled alone", () => {
  const link = document.getElementById("link")!;
  link.addEventListener("contextmenu", (event) => event.preventDefault());
  expect(rightClick("link")).toBe(true);
  expect(keepsNativeMenu(null, null)).toBe(false);
});

test("development builds keep Shift+right-click for Inspect", () => {
  uninstall();
  uninstall = installContextMenuFilter(document, true);
  expect(rightClick("page", { shiftKey: true })).toBe(false);
  expect(rightClick("page")).toBe(true);
});
