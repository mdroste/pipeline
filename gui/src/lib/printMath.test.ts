import script from "../../src-tauri/src/commands/export/print_math.js?raw";

it("fits only oversized equations after fonts load and when the print width changes", async () => {
  document.body.innerHTML =
    '<p id="prose">Body text <span class="katex" id="short">x</span></p>' +
    '<span class="katex" id="inline">wide inline equation</span>' +
    '<span class="katex-display"><span class="katex" id="display">wide display equation</span></span>';
  const listeners = vi.spyOn(window, "addEventListener");
  const originalFonts = Object.getOwnPropertyDescriptor(document, "fonts");
  let available = 500;
  let fontsLoaded!: () => void;
  Object.defineProperty(window.document, "fonts", {
    configurable: true,
    value: { ready: new Promise<void>((resolve) => (fontsLoaded = resolve)) },
  });
  vi.spyOn(
    window.HTMLElement.prototype,
    "clientWidth",
    "get",
  ).mockImplementation(() => available);
  window.document.querySelectorAll<HTMLElement>(".katex").forEach((node) => {
    const naturalWidth = node.id === "short" ? 20 : 1000;
    node.getBoundingClientRect = () =>
      ({
        width: naturalWidth * (parseFloat(node.style.fontSize || "16") / 16),
      }) as DOMRect;
  });
  vi.spyOn(window, "getComputedStyle").mockImplementation(
    (node) =>
      ({
        fontSize: (node as HTMLElement).style.fontSize || "16px",
      }) as CSSStyleDeclaration,
  );
  vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
    callback(0);
    return 0;
  });
  vi.spyOn(window, "print").mockImplementation(() => {});

  try {
    window.eval(script);
    expect(window.print).not.toHaveBeenCalled();
    fontsLoaded();
    await Promise.resolve();
    expect(window.print).toHaveBeenCalledOnce();
    expect(window.document.querySelectorAll(".pipeline-math")).toHaveLength(3);
    expect(
      window.document.querySelectorAll(".pipeline-math-display"),
    ).toHaveLength(1);

    const inline = window.document.getElementById("inline")!;
    const display = window.document.getElementById("display")!;
    expect(inline.getBoundingClientRect().width).toBeLessThanOrEqual(500);
    expect(display.getBoundingClientRect().width).toBeLessThanOrEqual(500);
    const screenSize = inline.style.fontSize;

    available = 300;
    window.dispatchEvent(new window.Event("beforeprint"));
    expect(inline.getBoundingClientRect().width).toBeLessThanOrEqual(300);
    expect(display.getBoundingClientRect().width).toBeLessThanOrEqual(300);
    const printSize = inline.style.fontSize;
    window.dispatchEvent(new window.Event("beforeprint"));
    expect(inline.style.fontSize).toBe(printSize);

    available = 500;
    window.dispatchEvent(new window.Event("afterprint"));
    expect(inline.style.fontSize).toBe(screenSize);
    available = 1200;
    window.dispatchEvent(new window.Event("resize"));
    expect(inline.style.fontSize).toBe("");
    expect(window.document.getElementById("short")!.style.fontSize).toBe("");
    expect(window.document.getElementById("prose")!.style.fontSize).toBe("");
  } finally {
    for (const [event, listener, options] of listeners.mock.calls) {
      window.removeEventListener(event, listener, options);
    }
    if (originalFonts) Object.defineProperty(document, "fonts", originalFonts);
    else Reflect.deleteProperty(document, "fonts");
    document.body.innerHTML = "";
    vi.restoreAllMocks();
  }
});
