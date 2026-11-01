// Zoom, pan and rotate for one image. Rotation changes this view only.
import { useEffect, useRef, useState } from "react";
import type {
  FileAdapter,
  FilePreview,
  ImagePreview,
} from "../../../lib/fileWorkspaceClient";
import { formatFileSize } from "../../../lib/fileKinds";
import { workbenchErrorMessage } from "../../../lib/workbenchError";

const MIN_ZOOM = 0.05;
const MAX_ZOOM = 16;
const clamp = (value: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, value));

interface ImageProps {
  src: string;
  alt: string;
  bytes: number;
  note?: string;
  onFailed?: () => void;
}
/** Each image starts fresh: a reset effect could run after `load` has fired. */
export default function ImageViewer(props: ImageProps) {
  return <ImageFrame key={props.src} {...props} />;
}
function ImageFrame({ src, alt, bytes, note, onFailed }: ImageProps) {
  const frame = useRef<HTMLDivElement>(null);
  const [zoom, setZoom] = useState<number | null>(null);
  const [rotation, setRotation] = useState(0);
  const [natural, setNatural] = useState<{ w: number; h: number } | null>(null);
  const [failed, setFailed] = useState(false);
  const drag = useRef<{
    x: number;
    y: number;
    left: number;
    top: number;
  } | null>(null);
  // Fit depends on the frame, so a resized pane must refit.
  const [, setFrameSize] = useState(0);
  useEffect(() => {
    const box = frame.current;
    if (!box || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() =>
      setFrameSize(box.clientWidth * 100000 + box.clientHeight),
    );
    observer.observe(box);
    return () => observer.disconnect();
  }, []);
  const turned = rotation % 180 !== 0;
  const fitted = () => {
    const box = frame.current;
    if (!box || !natural) return 1;
    const [w, h] = turned ? [natural.h, natural.w] : [natural.w, natural.h];
    return Math.min(box.clientWidth / w, box.clientHeight / h, 1) || 1;
  };
  const scale = zoom ?? fitted();
  const nudge = (factor: number) => setZoom(clamp(scale * factor));
  if (failed)
    return (
      <p role="alert" className="file-error">
        This image could not be displayed. Reload it, or open it externally.
      </p>
    );
  const width = natural ? natural.w * scale : undefined;
  const height = natural ? natural.h * scale : undefined;
  return (
    <div className="file-image">
      <div className="file-toolbar" role="toolbar" aria-label="Image view">
        <button aria-label="Zoom out" onClick={() => nudge(1 / 1.25)}>
          −
        </button>
        <span aria-live="polite">{Math.round(scale * 100)}%</span>
        <button aria-label="Zoom in" onClick={() => nudge(1.25)}>
          +
        </button>
        <button aria-pressed={zoom === null} onClick={() => setZoom(null)}>
          Fit
        </button>
        <button aria-pressed={zoom === 1} onClick={() => setZoom(1)}>
          Actual size
        </button>
        <button onClick={() => setRotation((old) => (old + 90) % 360)}>
          Rotate
        </button>
        <span className="file-image-facts">
          {natural ? `${natural.w} × ${natural.h} · ` : ""}
          {formatFileSize(bytes)}
          {note ? ` · ${note}` : ""}
        </span>
      </div>
      <div
        ref={frame}
        className="file-image-frame"
        tabIndex={0}
        role="group"
        aria-label={`Image: ${alt}`}
        onWheel={(event) => {
          if (!event.ctrlKey && !event.metaKey) return;
          event.preventDefault();
          nudge(event.deltaY < 0 ? 1.1 : 1 / 1.1);
        }}
        onKeyDown={(event) => {
          if (event.key === "+" || event.key === "=") nudge(1.25);
          else if (event.key === "-") nudge(1 / 1.25);
          else if (event.key === "0") setZoom(null);
          else if (event.key.toLowerCase() === "r")
            setRotation((old) => (old + 90) % 360);
        }}
        onPointerDown={(event) => {
          const box = frame.current;
          if (!box) return;
          drag.current = {
            x: event.clientX,
            y: event.clientY,
            left: box.scrollLeft,
            top: box.scrollTop,
          };
          box.setPointerCapture?.(event.pointerId);
        }}
        onPointerMove={(event) => {
          const box = frame.current;
          if (!box || !drag.current) return;
          box.scrollLeft = drag.current.left - (event.clientX - drag.current.x);
          box.scrollTop = drag.current.top - (event.clientY - drag.current.y);
        }}
        onPointerUp={() => {
          drag.current = null;
        }}
      >
        <div
          className="file-image-stage"
          style={
            natural
              ? {
                  width: turned ? height : width,
                  height: turned ? width : height,
                }
              : undefined
          }
        >
          <img
            alt={alt}
            src={src}
            draggable={false}
            style={{
              width,
              height,
              transform: rotation ? `rotate(${rotation}deg)` : undefined,
            }}
            onLoad={(event) =>
              setNatural({
                w: event.currentTarget.naturalWidth,
                h: event.currentTarget.naturalHeight,
              })
            }
            onError={() => {
              setFailed(true);
              onFailed?.();
            }}
          />
        </div>
      </div>
    </div>
  );
}

/** A format the webview cannot decode, converted to PNG by the backend. */
export function ConvertedImage({
  adapter,
  file,
}: {
  adapter: FileAdapter;
  file: FilePreview;
}) {
  const [preview, setPreview] = useState<ImagePreview | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    setPreview(null);
    setError("");
    if (!adapter.imagePreview) {
      setError("This image type cannot be shown here. Open it externally.");
      return;
    }
    adapter
      .imagePreview(file.path)
      .then((value) => live && setPreview(value))
      .catch((e) => live && setError(workbenchErrorMessage(e)));
    return () => {
      live = false;
    };
  }, [adapter, file.path, file.hash]);
  if (error)
    return (
      <p role="alert" className="file-error">
        {error}
      </p>
    );
  if (!preview) return <p className="file-status">Converting image…</p>;
  const reduced =
    preview.width < preview.sourceWidth
      ? `Reduced from ${preview.sourceWidth} × ${preview.sourceHeight}`
      : "";
  const pages = preview.pages > 1 ? `Page 1 of ${preview.pages}` : "";
  return (
    <ImageViewer
      alt={file.path}
      bytes={file.bytes}
      src={`data:${preview.mime};base64,${preview.base64}`}
      note={[pages, reduced].filter(Boolean).join(" · ")}
    />
  );
}
