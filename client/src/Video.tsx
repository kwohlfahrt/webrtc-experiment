import { useState, useEffect, useRef } from "react";
import { Pos } from "./util";

interface Props {
  pos: Pos;
  distance?: number;
  media: MediaStream | null;
}

export const useMedia = () => {
  const [media, setMedia] = useState<null | MediaStream>(null);

  useEffect(() => {
    const p = navigator.mediaDevices.getUserMedia({
      audio: true,
      video: { width: 320, height: 320 },
    });
    p.then(setMedia);

    return () => {
      setMedia(null);
      p.then((media) => media.getTracks().forEach((t) => t.stop()));
    };
  }, []);

  return media;
};

export default ({ pos, distance, media }: Props) => {
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    if (videoRef.current != null) videoRef.current.srcObject = media;
  }, [videoRef.current]);

  const containerStyle = { left: `${pos.x}px`, top: `${pos.y}px` };
  const videoStyle = {
    left: "-25px",
    top: "-25px",
    opacity: 1,
  };
  const classes = ["videoContainer"];
  if (distance == null) {
    classes.push("self");
  } else {
    videoStyle.opacity = Math.min(1, Math.max(0, 1 - (distance - 200) / 200));
  }

  return (
    <div style={containerStyle} className={classes.join(" ")}>
      {distance ?? 0 < 400 ? (
        <video style={videoStyle} autoPlay ref={videoRef} />
      ) : (
        <div>Profile Pic</div>
      )}
    </div>
  );
};
