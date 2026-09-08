import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import useModalDialog from "../hooks/useModalDialog";

interface Props {
  onResize: (width: number) => void;
  min: number;
  max: number;
  currentWidth?: number;
  defaultWidth?: number;
  label?: string;
  edge?: "left" | "right" | "top" | "bottom";
  controlsId?: string;
  /** Space reserved for the flexible sibling, in addition to fixed siblings. */
  minRemaining?: number;
}
const bound = (value: number, min: number, max: number) => Math.max(min, Math.min(max, Number.isFinite(value) ? value : min));

function SizeDialog({ label, value, min, max, defaultValue, vertical, onApply, onClose }: {
  label: string; value: number; min: number; max: number; defaultValue?: number;
  vertical: boolean; onApply: (value: number) => void; onClose: () => void;
}) {
  const title = useId();
  const [draft, setDraft] = useState(String(Math.round(value)));
  const number = Number(draft);
  const valid = draft.trim() !== "" && Number.isFinite(number) && number >= min && number <= max;
  const ref = useModalDialog<HTMLDivElement>(onClose, true, true);
  return createPortal(<div className="fixed inset-0 z-[90] flex items-center justify-center bg-black/30 p-4" onPointerDown={event => { if (event.target === event.currentTarget) onClose(); }}>
    <div ref={ref} role="dialog" aria-modal="true" aria-labelledby={title} tabIndex={-1} className="w-full max-w-sm rounded-xl border border-gray-300 bg-white p-5 text-gray-900 shadow-xl dark:border-gray-700 dark:bg-neutral-900 dark:text-gray-100">
      <h2 id={title} className="text-base font-semibold">{label}</h2>
      <form onSubmit={event => { event.preventDefault(); if (valid) { onApply(number); onClose(); } }}>
        <label className="mt-4 block text-sm">{vertical ? "Height" : "Width"} in pixels
          <input data-autofocus type="number" min={min} max={max} step="any" value={draft} aria-invalid={!valid} aria-describedby={`${title}-range`} onChange={event => setDraft(event.target.value)} className="mt-1 w-full rounded border border-gray-300 bg-transparent px-3 py-2 dark:border-gray-600"/>
        </label>
        <p id={`${title}-range`} className="mt-1 text-xs text-gray-600 dark:text-gray-300">Available range: {Math.ceil(min)}–{Math.floor(max)} pixels.</p>
        <div className="mt-3 flex gap-2">
          <button type="button" className="rounded border px-3 py-2 text-sm" onClick={() => setDraft(String(bound(number - 8, min, max)))}>Decrease</button>
          <button type="button" className="rounded border px-3 py-2 text-sm" onClick={() => setDraft(String(bound(number + 8, min, max)))}>Increase</button>
          {defaultValue !== undefined && <button type="button" className="rounded border px-3 py-2 text-sm" onClick={() => setDraft(String(bound(defaultValue, min, max)))}>Reset</button>}
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <button type="button" className="rounded border px-3 py-2 text-sm" onClick={onClose}>Cancel</button>
          <button type="submit" disabled={!valid} className="rounded bg-gray-900 px-3 py-2 text-sm text-white disabled:opacity-40 dark:bg-gray-100 dark:text-gray-900">Apply size</button>
        </div>
      </form>
    </div>
  </div>, document.body);
}

export default function ResizeHandle({ onResize, min, max, currentWidth, defaultWidth,
  label = "Resize panel", edge = "right", controlsId, minRemaining = 240,
}: Props) {
  const handleRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<(() => void) | null>(null);
  const generatedId = useId();
  const [targetId, setTargetId] = useState(controlsId ?? generatedId);
  const [limits, setLimits] = useState({ min, max });
  const [options, setOptions] = useState(false);
  const vertical = edge === "top" || edge === "bottom";
  const dimension = vertical ? "height" : "width";
  const direction = edge === "left" || edge === "top" ? -1 : 1;
  const clamp = useCallback((value: number) => bound(value, limits.min, limits.max), [limits]);
  const value = clamp(currentWidth ?? min);

  useLayoutEffect(() => {
    const pane = handleRef.current?.parentElement;
    const container = pane?.parentElement;
    if (!pane || !container) return;
    const previousId = pane.id;
    if (!pane.id) pane.id = controlsId ?? generatedId;
    setTargetId(controlsId ?? pane.id);
    const measure = () => {
      const available = container.getBoundingClientRect()[dimension];
      let maximum = max;
      if (available > 0) {
        const siblings = Array.from(container.children).filter((element): element is HTMLElement => element !== pane && element instanceof HTMLElement);
        const fixed = siblings.reduce((sum, element) => {
          const style = getComputedStyle(element);
          if (style.display === "none" || style.position === "absolute" || style.position === "fixed" || Number(style.flexGrow) > 0) return sum;
          return sum + element.getBoundingClientRect()[dimension];
        }, 0);
        const style = getComputedStyle(container);
        const padding = vertical ? parseFloat(style.paddingTop) + parseFloat(style.paddingBottom) : parseFloat(style.paddingLeft) + parseFloat(style.paddingRight);
        const gap = parseFloat(vertical ? style.rowGap : style.columnGap) || 0;
        maximum = Math.max(0, Math.min(max, available - (padding || 0) - gap * siblings.filter(el => getComputedStyle(el).display !== "none").length - fixed - minRemaining));
      }
      const next = { min: Math.min(min, maximum), max: maximum };
      setLimits(old => old.min === next.min && old.max === next.max ? old : next);
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(container);
    Array.from(container.children).forEach(element => { if (element !== pane) observer?.observe(element); });
    return () => { observer?.disconnect(); if (!previousId && pane.id === (controlsId ?? generatedId)) pane.removeAttribute("id"); };
  }, [controlsId, generatedId, dimension, min, max, minRemaining, vertical]);

  useLayoutEffect(() => {
    cancelRef.current?.();
    const pane = handleRef.current?.parentElement;
    if (pane && currentWidth !== undefined) pane.style[dimension] = `${value}px`;
  }, [value, currentWidth, dimension, limits]);
  useEffect(() => () => cancelRef.current?.(), []);

  const begin = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || event.isPrimary === false) return;
    cancelRef.current?.();
    const handle = event.currentTarget;
    const pane = handle.parentElement;
    if (!pane) return;
    event.preventDefault(); handle.focus();
    const start = vertical ? event.clientY : event.clientX;
    const startSize = clamp(currentWidth ?? pane.getBoundingClientRect()[dimension]);
    const previousStyle = pane.style[dimension];
    const previousCursor = document.body.style.cursor;
    const previousSelect = document.body.style.userSelect;
    let pending = startSize;
    let moved = false;
    let frame: number | null = null;
    let finished = false;
    const preview = () => {
      pane.style[dimension] = `${pending}px`;
      handle.setAttribute("aria-valuenow", String(pending));
      handle.setAttribute("aria-valuetext", `${Math.round(pending)} pixels`);
      frame = null;
    };
    const move = (e: PointerEvent) => {
      if (e.pointerId !== event.pointerId) return;
      pending = clamp(startSize + ((vertical ? e.clientY : e.clientX) - start) * direction);
      moved ||= pending !== startSize;
      if (frame === null) frame = requestAnimationFrame(preview);
    };
    const finish = (commit: boolean) => {
      if (finished) return;
      finished = true;
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", up);
      document.removeEventListener("pointercancel", cancel);
      document.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", cancel);
      handle.removeEventListener("lostpointercapture", cancel);
      if (frame !== null) cancelAnimationFrame(frame);
      if (handle.hasPointerCapture?.(event.pointerId)) handle.releasePointerCapture(event.pointerId);
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousSelect;
      cancelRef.current = null;
      if (commit && moved) { preview(); onResize(pending); }
      else { pane.style[dimension] = previousStyle; handle.setAttribute("aria-valuenow", String(startSize)); handle.setAttribute("aria-valuetext", `${Math.round(startSize)} pixels`); }
    };
    const up = (e: PointerEvent) => { if (e.pointerId === event.pointerId) finish(true); };
    const cancel = (e?: Event) => { if (e && "pointerId" in e && e.pointerId !== event.pointerId) return; finish(false); };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); cancel(); } };
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", up);
    document.addEventListener("pointercancel", cancel);
    document.addEventListener("keydown", key, true);
    window.addEventListener("blur", cancel);
    handle.addEventListener("lostpointercapture", cancel);
    try { handle.setPointerCapture?.(event.pointerId); } catch { /* A released pointer is still handled by the document listeners. */ }
    document.body.style.cursor = vertical ? "row-resize" : "col-resize";
    document.body.style.userSelect = "none";
    cancelRef.current = cancel;
  };
  const keyDown = (event: React.KeyboardEvent) => {
    const step = event.shiftKey ? 32 : 8;
    let next: number | null = null;
    if (event.key === (vertical ? "ArrowUp" : "ArrowLeft")) next = value - step * direction;
    if (event.key === (vertical ? "ArrowDown" : "ArrowRight")) next = value + step * direction;
    if (event.key === "Home") next = limits.min;
    if (event.key === "End") next = limits.max;
    if (event.key === "Enter") { event.preventDefault(); setOptions(true); return; }
    if (next !== null) { event.preventDefault(); onResize(clamp(next)); }
  };
  const position = vertical ? `${edge === "bottom" ? "-bottom-1" : "-top-1"} left-0 w-full h-2 cursor-row-resize` : `${edge === "right" ? "-right-1" : "-left-1"} top-0 h-full w-2 cursor-col-resize`;
  return <>
    <div ref={handleRef} role="separator" aria-label={label} aria-orientation={vertical ? "horizontal" : "vertical"} aria-controls={targetId}
      aria-valuemin={limits.min} aria-valuemax={limits.max} aria-valuenow={value} aria-valuetext={`${Math.round(value)} pixels`}
      tabIndex={0} onPointerDown={begin} onDoubleClick={() => defaultWidth !== undefined && onResize(clamp(defaultWidth))} onKeyDown={keyDown}
      title="Drag or use arrow keys to resize · Enter for size options · Double-click to reset"
      className={`group absolute z-20 touch-none focus-visible:outline-none ${position}`}>
      <span aria-hidden="true" className={`absolute bg-gray-400 opacity-40 group-hover:opacity-100 group-focus-visible:opacity-100 dark:bg-gray-500 ${vertical ? "inset-x-0 top-1/2 h-px" : "inset-y-0 left-1/2 w-px"}`}/>
    </div>
    {options && <SizeDialog label={label} value={value} min={limits.min} max={limits.max} defaultValue={defaultWidth} vertical={vertical} onApply={onResize} onClose={() => setOptions(false)}/>}
  </>;
}
