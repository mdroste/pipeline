// Every desktop webview (WebView2, WKWebView, WebKitGTK) opens its own browser
// menu on right-click: Back, Reload, Save as, Print, Open link in new window,
// Copy image link. In Pipeline those act on the app's internal page, so the
// app keeps the native menu only where it helps: text fields and editors
// (cut, copy, paste, spelling) and over selected text (copy). Components that
// draw their own menu call preventDefault first; an element can opt back in
// with data-native-menu.

const TEXT_INPUT_TYPES = new Set([
  "",
  "text",
  "search",
  "url",
  "email",
  "tel",
  "password",
  "number",
]);

function elementOf(target: EventTarget | null): Element | null {
  if (!target || typeof (target as Node).nodeType !== "number") return null;
  const node = target as Node;
  return node.nodeType === Node.ELEMENT_NODE
    ? (node as Element)
    : node.parentElement;
}

function isEditable(element: Element): boolean {
  if (element.closest("textarea")) return true;
  const input = element.closest("input");
  if (input) return TEXT_INPUT_TYPES.has(input.type.toLowerCase());
  const editable = element.closest("[contenteditable]");
  return Boolean(
    editable && editable.getAttribute("contenteditable") !== "false",
  );
}

function isInsideSelection(element: Element, selection: Selection | null) {
  if (!selection || selection.isCollapsed || !selection.toString().trim())
    return false;
  for (let index = 0; index < selection.rangeCount; index++)
    if (selection.getRangeAt(index).intersectsNode(element)) return true;
  return false;
}

/** Whether a right-click on `target` should open the webview's own menu. */
export function keepsNativeMenu(
  target: EventTarget | null,
  selection: Selection | null,
): boolean {
  const element = elementOf(target);
  if (!element) return false;
  if (element.closest("[data-native-menu]")) return true;
  return isEditable(element) || isInsideSelection(element, selection);
}

/**
 * Suppresses the browser menu everywhere else. In development builds,
 * Shift+right-click still opens it so Inspect stays one click away.
 */
export function installContextMenuFilter(
  doc: Document = document,
  allowShiftBypass: boolean = import.meta.env.DEV,
): () => void {
  const onContextMenu = (event: MouseEvent) => {
    if (event.defaultPrevented) return;
    if (allowShiftBypass && event.shiftKey) return;
    if (keepsNativeMenu(event.target, doc.getSelection())) return;
    event.preventDefault();
  };
  doc.addEventListener("contextmenu", onContextMenu);
  return () => doc.removeEventListener("contextmenu", onContextMenu);
}
