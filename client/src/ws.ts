import { useState, useEffect, useRef, useCallback } from "react";
import { useMap, Pos } from "./util";

type PeerMessage = {
  peer: number;
} & (
  | {
      type: "ICECandidate";
      data: RTCIceCandidateInit;
    }
  | {
      type: "SDP";
      data: RTCSessionDescriptionInit;
    }
);

type ServerMessage =
  | {
      type: "Hello";
      state: { id: number } & PeerState;
      peers: ({ id: number } & PeerState)[];
    }
  | {
      type: "AddPeer";
      peer: { id: number } & PeerState;
    }
  | {
      type: "RemovePeer";
      peer: number;
    }
  | {
      type: "PeerMessage";
      message: PeerMessage;
    };

interface PeerState {
  pos: Pos;
}

interface PeerConnection extends PeerState {
  connection: RTCPeerConnection;
  streams: readonly MediaStream[];
}

export interface Peer extends PeerState {
  stream: MediaStream;
}

const call = (
  media: MediaStream,
  selfCb: (id: number, state: Peer) => void,
  peerCb: (id: number, state: Peer | null) => void,
) => {
  const ws = new WebSocket(`ws://${HOST}/signalling`);
  const connections = new Map<number, PeerConnection>();
  const send = (msg: PeerMessage) => ws.send(JSON.stringify(msg));

  let self: number | null = null;

  const addPeer = (
    { id, ...state }: { id: number } & PeerState,
    polite: boolean,
  ) => {
    const connection = new RTCPeerConnection();

    connection.addEventListener("icecandidate", ({ candidate }) => {
      if (candidate) {
        send({ type: "ICECandidate", peer: id, data: candidate });
      }
    });
    connection.addEventListener("track", ({ streams }) => {
      connections.set(id, { ...state, connection, streams });
      peerCb(id, { ...state, stream: streams[0] });
    });
    connection.addEventListener("negotiationneeded", async () => {
      await connection.setLocalDescription(await connection.createOffer());
      send({
        type: "SDP",
        peer: id,
        data: connection.localDescription!,
      });
    });

    if (!polite && media != null) {
      media.getTracks().forEach((track) => connection.addTrack(track, media));
    }

    connections.set(id, { ...state, connection, streams: [] });
  };

  const removePeer = (id: number) => {
    const { connection } = connections.get(id)!;
    connection.close();
    connections.delete(id);
    peerCb(id, null);
  };

  const handler = async (msg: ServerMessage) => {
    if (msg.type == "Hello") {
      const {
        state: { id, ...state },
        peers,
      } = msg;
      self = id;
      selfCb(id, { ...state, stream: media });
      peers.forEach((p) => addPeer(p, true));
    } else if (msg.type == "AddPeer") {
      const { peer } = msg;
      addPeer(peer, false);
    } else if (msg.type == "RemovePeer") {
      const { peer } = msg;
      removePeer(peer);
    } else if (msg.type == "PeerMessage") {
      const { peer } = msg.message;
      const { connection } = connections.get(peer)!;
      if (msg.message.type == "ICECandidate") {
        await connection.addIceCandidate(msg.message.data);
      } else if (msg.message.type == "SDP") {
        const sdp = msg.message.data;
        if (sdp.type == "answer") {
          await connection.setRemoteDescription(sdp);
        } else if (sdp.type == "offer") {
          await connection.setRemoteDescription(sdp);
          // Never null, because listener is not added in that condition
          media!
            .getTracks()
            .forEach((track) => connection.addTrack(track, media!));
          await connection.setLocalDescription(await connection.createAnswer());
          send({
            type: "SDP",
            peer,
            data: connection.localDescription!,
          });
        }
      }
    }
  };

  ws.addEventListener("message", ({ data }) =>
    handler(JSON.parse(data) as ServerMessage),
  );

  return ws;
};

export const useCall = (
  media: MediaStream | null,
): [Peer | null, (Peer & { id: number })[]] => {
  const [peers, updatePeers] = useMap<number, Peer>();
  const [self, setSelf] = useState<Peer | null>(null);

  const selfCb = useCallback((id: number, state: Peer) => setSelf(state), [
    setSelf,
  ]);
  const peerCb = useCallback(
    (id: number, state: Peer | null) => {
      if (state == null) {
        updatePeers.remove(id);
      } else {
        updatePeers.insert(id, state);
      }
    },
    [updatePeers.remove, updatePeers.insert],
  );

  useEffect(() => {
    if (media == null) return;
    const ws = call(media, selfCb, peerCb);
    return () => ws.close();
  }, [media, peerCb]);

  return [self, Array.from(peers.entries(), ([id, peer]) => ({ id, ...peer }))];
};
