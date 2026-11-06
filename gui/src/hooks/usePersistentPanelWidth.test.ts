import { act, renderHook } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import usePersistentPanelWidth from "./usePersistentPanelWidth";

it("keeps the layout usable when storage reads and writes fail", () => {
  const read = vi.spyOn(localStorage, "getItem").mockImplementation(() => {
    throw new DOMException("Blocked", "SecurityError");
  });
  const write = vi.spyOn(localStorage, "setItem").mockImplementation(() => {
    throw new DOMException("Full", "QuotaExceededError");
  });
  try {
    const { result } = renderHook(() =>
      usePersistentPanelWidth("rail", 240, 200, 500),
    );
    expect(result.current[0]).toBe(240);
    act(() => result.current[1](400));
    expect(result.current[0]).toBe(400);
    act(() => result.current[1](800));
    expect(result.current[0]).toBe(500);
  } finally {
    read.mockRestore();
    write.mockRestore();
  }
});
