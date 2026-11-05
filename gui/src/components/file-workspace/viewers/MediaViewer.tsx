// Audio and video, seeking through range requests.
import { useState } from "react";

export default function MediaViewer({
  src,
  title,
  video,
}: {
  src: string;
  title: string;
  video: boolean;
}) {
  const [failed, setFailed] = useState(false);
  if (failed)
    return (
      <p role="alert" className="file-error">
        This media format cannot be played here. Open it externally.
      </p>
    );
  return (
    <div className="file-media">
      {video ? (
        <video
          controls
          preload="metadata"
          src={src}
          aria-label={title}
          onError={() => setFailed(true)}
        />
      ) : (
        <audio
          controls
          preload="metadata"
          src={src}
          aria-label={title}
          onError={() => setFailed(true)}
        />
      )}
    </div>
  );
}
