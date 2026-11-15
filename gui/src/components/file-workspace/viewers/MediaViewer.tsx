// Audio and video, seeking through range requests.
import { useState } from "react";

interface Props {
  src: string;
  title: string;
  video: boolean;
}

export default function MediaViewer(props: Props) {
  return <MediaPlayer key={`${props.video}:${props.src}`} {...props} />;
}

function MediaPlayer({ src, title, video }: Props) {
  const [failed, setFailed] = useState(false);
  if (failed)
    return (
      <p role="alert" className="file-error">
        This media format cannot be played here. Open it externally or retry.
        <button className="file-action" onClick={() => setFailed(false)}>
          Retry playback
        </button>
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
