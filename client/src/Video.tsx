import { useState, useEffect, useRef } from "react";
import { Pos } from "./util";

interface Props {
  pos: Pos;
  factor?: number;
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

export default ({ pos, factor, media }: Props) => {
  const videoRef = useRef<HTMLVideoElement>(null);
  const volume = factor ?? 0;

  useEffect(() => {
    if (videoRef.current != null) {
      videoRef.current.srcObject = media;
      videoRef.current.play();
    }
  }, [videoRef.current]);

  useEffect(() => {
    if (videoRef.current != null) videoRef.current.volume = volume;
  }, [videoRef.current, volume]);

  const containerStyle = { left: `${pos.x - 80}px`, top: `${pos.y - 80}px` };
  const videoStyle = {
    left: "-25px",
    top: "-25px",
    opacity: 1,
  };
  const classes = ["videoContainer"];
  if (factor == null) {
    classes.push("self");
  } else {
    videoStyle.opacity = factor;
  }


  return (
    <div style={containerStyle} className={classes.join(" ")}>
      {(volume > 0 || factor == null) ? (
        <video style={videoStyle} ref={videoRef} />
      ) : (
        <div>Profile Pic</div>
      )}
    </div>
  );
};
