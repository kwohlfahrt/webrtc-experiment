import { useState, useEffect, useRef } from "react";
import Video from "./Video";
import { useMedia } from "./Video";
import { useCall, Peer } from "./ws";
import { Pos, distance, useMap } from "./util";

const positions = [
  { x: 10, y: 10 },
  { x: 500, y: 500 },
  { x: 350, y: 30 },
];

const selfPos = { x: 250, y: 170 };

const size = {
  width: 800,
  height: 600,
};

const Room = () => {
  const media = useMedia();
  const [peers, addPeer, removePeer] = useMap<number, Peer>();
  useCall(addPeer, removePeer, media);

  const videos = positions.map((pos, idx) => (
    <Video
      key={idx}
      pos={pos}
      distance={distance(selfPos, pos)}
      media={media}
    />
  ));

  const style = {
    width: `${size.width}px`,
    height: `${size.height}px`,
  };

  const lines = positions.map((pos, idx) => (
    <line
      opacity={distance(pos, selfPos) < 400 ? 1 : 0}
      key={idx}
      x1={selfPos.x}
      y1={selfPos.y}
      x2={pos.x}
      y2={pos.y}
    />
  ));

  return (
    <div>
      <div style={style} className="room">
        {videos}
        <Video pos={selfPos} media={media} />
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
