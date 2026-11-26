import { useEffect, useRef, useState, type RefObject } from "react";

export interface SpotlightRect {
  left: number;
  top: number;
  width: number;
  height: number;
}
export interface CoachPosition {
  left: number;
  top: number;
}

const intersects = (a: SpotlightRect, b: SpotlightRect) =>
  Math.max(
    0,
    Math.min(a.left + a.width, b.left + b.width) - Math.max(a.left, b.left),
  ) *
  Math.max(
    0,
    Math.min(a.top + a.height, b.top + b.height) - Math.max(a.top, b.top),
  );
export function placeTutorialCoach(
  target: SpotlightRect | null,
  width: number,
  height: number,
  viewportWidth: number,
  viewportHeight: number,
  action?: SpotlightRect | null,
): CoachPosition {
  const gap = 16;
  const right = Math.max(gap, viewportWidth - width - gap);
  const bottom = Math.max(gap, viewportHeight - height - gap);
  const choices = [
    { left: right, top: bottom },
    { left: gap, top: bottom },
    { left: right, top: gap },
    { left: gap, top: gap },
  ];
  if (!target)
    return action
      ? placeTutorialCoach(action, width, height, viewportWidth, viewportHeight)
      : choices[0];
  const padded = {
    left: target.left - gap,
    top: target.top - gap,
    width: target.width + gap * 2,
    height: target.height + gap * 2,
  };
  const placement = choices.reduce(
    (best, position) =>
      intersects({ ...position, width, height }, padded) <
      intersects({ ...best, width, height }, padded)
        ? position
        : best,
    choices[0],
  );
  // Prefer keeping the whole control group clear. Only prioritize one action
  // when the highlighted screen leaves no corner free (for example a preview).
  return action && intersects({ ...placement, width, height }, target) > 0
    ? placeTutorialCoach(action, width, height, viewportWidth, viewportHeight)
    : placement;
}

/** Measures the live controls; no duplicated geometry or lifted app elements. */
export function useTutorialSpotlight(
  root: RefObject<HTMLDivElement | null>,
  coach: RefObject<HTMLElement | null>,
  selector: string | null,
  interactive: boolean,
  step: number,
  {
    actionSelector,
    externalTarget = false,
    onExternalActivate,
  }: {
    actionSelector?: string;
    externalTarget?: boolean;
    onExternalActivate?: () => void;
  } = {},
) {
  const [rect, setRect] = useState<SpotlightRect | null>(null);
  const [position, setPosition] = useState<CoachPosition | null>(null);
  const activateRef = useRef(onExternalActivate);
  activateRef.current = onExternalActivate;
  useEffect(() => {
    const scope = root.current;
    if (!scope) return;
    const previousCoachHeight = scope.style.getPropertyValue(
      "--tutorial-coach-height",
    );
    const findTarget = () =>
      selector
        ? (externalTarget ? document : scope).querySelector<HTMLElement>(
            selector,
          )
        : null;
    let frame = 0;
    let firstTarget: HTMLElement | null = null;
    let resize: ResizeObserver | null = null;
    let inertElements: HTMLElement[] = [];
    const restore = () => {
      for (const item of inertElements) item.removeAttribute("inert");
      inertElements = [];
    };
    const isolate = (branch: HTMLElement, allowed: HTMLElement[]) => {
      if (allowed.includes(branch)) return;
      if (allowed.some((element) => branch.contains(element))) {
        for (const child of branch.children)
          if (child instanceof HTMLElement) isolate(child, allowed);
      } else if (!branch.hasAttribute("inert")) {
        branch.setAttribute("inert", "");
        inertElements.push(branch);
      }
    };
    const measure = () => {
      const target = findTarget();
      restore();
      // Include the actual sidebar control in the tutorial's interaction area.
      // Free exploration unlocks the report, while the live app stays isolated.
      const allowed = selector
        ? [coach.current, interactive ? target : null].filter(
            (element): element is HTMLElement => element !== null,
          )
        : [scope];
      isolate(document.body, allowed);
      if (target && target !== firstTarget) {
        if (firstTarget) resize?.unobserve(firstTarget);
        firstTarget = target;
        resize?.observe(target);
        target.scrollIntoView?.({
          block: "center",
          inline: "nearest",
          behavior: "instant",
        });
      }
      const bounds = target?.getBoundingClientRect();
      const next =
        bounds && bounds.width > 0 && bounds.height > 0
          ? {
              left: Math.max(4, bounds.left - 6),
              top: Math.max(4, bounds.top - 6),
              width: Math.max(
                0,
                Math.min(window.innerWidth - 4, bounds.right + 6) -
                  Math.max(4, bounds.left - 6),
              ),
              height: Math.max(
                0,
                Math.min(window.innerHeight - 4, bounds.bottom + 6) -
                  Math.max(4, bounds.top - 6),
              ),
            }
          : null;
      setRect((old) =>
        JSON.stringify(old) === JSON.stringify(next) ? old : next,
      );
      const size = coach.current?.getBoundingClientRect();
      if (size) {
        scope.style.setProperty("--tutorial-coach-height", `${size.height}px`);
        // A large spotlight (such as the preview dialog) may fill the window.
        // Keep its required action clear even when the coach must overlap it.
        const action = actionSelector
          ? scope.querySelector(actionSelector)?.getBoundingClientRect()
          : null;
        const placement = placeTutorialCoach(
          // Informational panels can fill the screen. Their spotlight is not
          // a click target, so it should not push the coach out of its corner.
          interactive ? next : null,
          size.width,
          size.height,
          window.innerWidth,
          window.innerHeight,
          interactive && action && action.width > 0 ? action : undefined,
        );
        setPosition((old) =>
          old?.left === placement.left && old.top === placement.top
            ? old
            : placement,
        );
      }
    };
    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(measure);
    };
    const mutation = new MutationObserver(schedule);
    // Sidebar layout changes can replace the target with its icon-only version.
    mutation.observe(document.body, { childList: true, subtree: true });
    resize =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(schedule);
    resize?.observe(scope);
    if (coach.current) resize?.observe(coach.current);
    measure();
    window.addEventListener("resize", schedule);
    document.addEventListener("scroll", schedule, true);
    // Keep all non-target app controls inert, including keyboard activation.
    const block = (event: MouseEvent) => {
      if (
        !(event.target instanceof Node) ||
        coach.current?.contains(event.target)
      )
        return;
      const target = selector ? findTarget() : scope;
      if ((selector && !interactive) || !target?.contains(event.target)) {
        event.preventDefault();
        event.stopPropagation();
      } else if (externalTarget) {
        // Use the real control without sending the underlying app's router home.
        event.preventDefault();
        event.stopPropagation();
        activateRef.current?.();
      }
    };
    document.addEventListener("click", block, true);
    return () => {
      if (previousCoachHeight)
        scope.style.setProperty("--tutorial-coach-height", previousCoachHeight);
      else scope.style.removeProperty("--tutorial-coach-height");
      restore();
      cancelAnimationFrame(frame);
      mutation.disconnect();
      resize?.disconnect();
      window.removeEventListener("resize", schedule);
      document.removeEventListener("scroll", schedule, true);
      document.removeEventListener("click", block, true);
    };
  }, [
    root,
    coach,
    selector,
    interactive,
    step,
    actionSelector,
    externalTarget,
  ]);
  return { rect, position };
}
