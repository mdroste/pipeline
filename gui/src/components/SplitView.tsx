import {
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import ResizeHandle from "./ResizeHandle";
import "./SplitView.css";

type Direction = "auto" | "horizontal" | "vertical";
function restore(key: string) {
  try {
    const saved = JSON.parse(localStorage.getItem(key) ?? "null");
    return {
      ratio: Number.isFinite(saved?.ratio)
        ? Math.max(0, Math.min(1, saved.ratio))
        : 0.5,
      direction: (["auto", "horizontal", "vertical"].includes(saved?.direction)
        ? saved.direction
        : "auto") as Direction,
    };
  } catch {
    return { ratio: 0.5, direction: "auto" as Direction };
  }
}

/** Two persistent panes with local fit constraints and a shared splitter. */
export default function SplitView({
  first,
  second,
  firstLabel,
  secondLabel,
  storageKey,
}: {
  first: ReactNode;
  second: ReactNode;
  firstLabel: string;
  secondLabel: string;
  storageKey: string;
}) {
  const [preference, setPreference] = useState(() => restore(storageKey));
  const [focus, setFocus] = useState<"first" | "second" | "both">("both");
  const [lastPane, setLastPane] = useState<"first" | "second">("first");
  const [size, setSize] = useState({ width: 0, height: 0 });
  const root = useRef<HTMLDivElement>(null);
  const id = useId();
  useLayoutEffect(() => {
    const element = root.current;
    if (!element) return;
    const measure = () => {
      const r = element.getBoundingClientRect();
      const css = getComputedStyle(element);
      if (r.width > 0 && r.height > 0)
        setSize({
          width:
            r.width -
            (parseFloat(css.paddingLeft) || 0) -
            (parseFloat(css.paddingRight) || 0),
          height:
            r.height -
            (parseFloat(css.paddingTop) || 0) -
            (parseFloat(css.paddingBottom) || 0),
        });
    };
    measure();
    const observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(measure);
    observer?.observe(element);
    return () => observer?.disconnect();
  }, []);
  const direction =
    preference.direction === "auto"
      ? size.width >= 960
        ? "horizontal"
        : "vertical"
      : preference.direction;
  const vertical = direction === "vertical";
  const available = Math.max(0, (vertical ? size.height : size.width) - 12);
  const minimum = vertical ? 240 : 320;
  const fits = available >= minimum * 2;
  const shown = focus === "both" && !fits ? lastPane : focus;
  const firstSize = Math.max(
    minimum,
    Math.min(available - minimum, available * preference.ratio),
  );
  const update = (next: typeof preference) => {
    setPreference(next);
    try {
      localStorage.setItem(storageKey, JSON.stringify(next));
    } catch {
      /* Layout remains usable without storage. */
    }
  };
  return (
    <div className="split-view">
      <div
        className="split-view-toolbar"
        role="group"
        aria-label={`${firstLabel} and ${secondLabel} layout`}
      >
        <label>
          Layout{" "}
          <select
            aria-label="Pane arrangement"
            value={preference.direction}
            onChange={(e) =>
              update({ ...preference, direction: e.target.value as Direction })
            }
          >
            <option value="auto">Automatic</option>
            <option value="horizontal">Side by side</option>
            <option value="vertical">Stacked</option>
          </select>
        </label>
        <button
          type="button"
          aria-controls={`${id}-first`}
          aria-pressed={shown === "first"}
          title={firstLabel}
          onClick={() => setFocus("first")}
        >
          First pane
        </button>
        <button
          type="button"
          aria-controls={`${id}-second`}
          aria-pressed={shown === "second"}
          title={secondLabel}
          onClick={() => setFocus("second")}
        >
          Second pane
        </button>
        {fits && (
          <button
            type="button"
            aria-pressed={shown === "both"}
            onClick={() => setFocus("both")}
          >
            Both panes
          </button>
        )}
      </div>
      <div ref={root} className={`split-view-body split-view-${direction}`}>
        <section
          id={`${id}-first`}
          aria-label={firstLabel}
          hidden={shown === "second"}
          onFocusCapture={() => setLastPane("first")}
          className="split-view-pane"
          style={
            shown === "both"
              ? { flex: "0 0 auto", [vertical ? "height" : "width"]: firstSize }
              : { flex: 1 }
          }
        >
          <div className="split-view-content">{first}</div>
          {shown === "both" && (
            <ResizeHandle
              label={`Resize ${firstLabel}`}
              controlsId={`${id}-first`}
              edge={vertical ? "bottom" : "right"}
              currentWidth={firstSize}
              defaultWidth={available / 2}
              min={minimum}
              max={available - minimum}
              minRemaining={0}
              onResize={(value) =>
                update({ ...preference, ratio: value / available })
              }
            />
          )}
        </section>
        <section
          id={`${id}-second`}
          aria-label={secondLabel}
          hidden={shown === "first"}
          onFocusCapture={() => setLastPane("second")}
          className="split-view-pane split-view-second"
        >
          <div className="split-view-content">{second}</div>
        </section>
      </div>
    </div>
  );
}
