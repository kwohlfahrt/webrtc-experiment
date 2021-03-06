import { useState, useEffect, useRef } from "react";
import Video from "./Video";
import { useMedia } from "./Video";
import { useCall, Peer } from "./ws";
import { Pos, factor, useMap } from "./util";

const size = {
  width: 800,
  height: 600,
};

const Room = () => {
  const media = useMedia();
  const [selfs, peers] = useCall(media);

  if (selfs == null) {
    return <div>Loading</div>;
  }

  const [self, setPos] = selfs;

  const videos = peers.map(({ id, pos, stream }) => (
    <Video
      key={id}
      pos={pos}
      factor={factor(self.pos, pos)}
      media={stream}
    />
  ));

  const style = {
    width: `${size.width}px`,
    height: `${size.height}px`,
  };

  const lines = peers.map(({ id, pos }) => (
    <line
      opacity={factor(self.pos, pos)}
      key={id}
      x1={self.pos.x - 80}
      y1={self.pos.y - 80}
      x2={pos.x - 80}
      y2={pos.y - 80}
    />
  ));

  const handleClick = (e: React.MouseEvent<HTMLElement>) => {
    setPos({
      x: e.clientX - e.currentTarget.clientLeft,
      y: e.clientY - e.currentTarget.clientTop,
    });
  };

  return (
    <div>
      <div style={style} className="room" onClick={handleClick}>
        {videos}
        <Video pos={self.pos} media={media} />
      </div>
      <svg style={style} viewBox={`0 0 ${size.width} ${size.height}`}>
        {lines}
      </svg>
    </div>
  );
};

export default () => {
  const [inCall, setInCall] = useState(false);

  return (
    <div>
      {inCall ? <Room /> : undefined}
      <button onClick={() => setInCall(!inCall)}>
        {inCall ? "Leave" : "Join"}
      </button>
    </div>
  );
};
