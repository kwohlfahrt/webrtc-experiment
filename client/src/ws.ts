import { useState, useEffect, useRef } from "react";

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
      id: number;
      peers: number[];
    }
  | {
      type: "AddPeer";
      peer: number;
    }
  | {
      type: "RemovePeer";
      peer: number;
    }
  | {
      type: "PeerMessage";
      message: PeerMessage;
    };

export interface Peer {
  connection: RTCPeerConnection;
}

const useWebSocket = (
  server: string,
  handler: (msg: ServerMessage) => void,
) => {
  const ws = new WebSocket(`ws://${server}`);
  ws.addEventListener("message", ({ data }) => {
    handler(JSON.parse(data) as ServerMessage);
  });

  useEffect(() => () => ws.close());

  return {
    send(msg: PeerMessage) {
      ws.send(JSON.stringify(msg));
    },
  };
};

export const useCall = (
  addPeerCb: (id: number, peer: Peer) => void,
  removePeerCb: (id: number) => void,
  media: MediaStream | null,
) => {
  const addPeer = (id: number, polite: boolean) => {
    const connection = new RTCPeerConnection();

    connection.addEventListener("icecandidate", ({ candidate }) => {
      if (candidate) {
        ws.send({ type: "ICECandidate", peer: id, data: candidate });
      }
    });
    connection.addEventListener("track", ({ streams }) => {
      //streams.forEach((stream) => (video.srcObject = stream));
    });
    connection.addEventListener("negotiationneeded", async () => {
      await connection.setLocalDescription(await connection.createOffer());
      ws.send({
        type: "SDP",
        peer: id,
        data: connection.localDescription!,
      });
    });

    if (!polite && media != null) {
      media.getTracks().forEach((track) => connection.addTrack(track, media));
    }

    addPeerCb(id, { connection });
  };


  const handleMessage = async (msg: ServerMessage) => {
  /*
    if (msg.type == "Hello") {
      const { id, peers } = data;
      peers.forEach((p) => addPeer(p, true));
    } else if (msg.type == "AddPeer") {
      const { peer } = data;
      addPeer(peer, false);
    } else if (msg.type == "RemovePeer") {
      const { peer } = data;
      peer.connection.close();
      removePeer(peer);
    } else if (msg.type == "PeerMessage") {
      const { peer } = msg.message;
      const { connection } = $$$;
      if (msg.message.type == "ICECandidate") {
        await connection.addIceCandidate(msg.message.data);
      } else if (msg.message.type == "SDP") {
        const sdp = msg.message.data;
        if (sdp.type == "answer") {
          await connection.setRemoteDescription(sdp);
        } else if (sdp.type == "offer") {
          await connection.setRemoteDescription(sdp);
          media
            .getTracks()
            .forEach((track) => connection.addTrack(track, media));
          await connection.setLocalDescription(await connection.createAnswer());
          sendMessage({
            type: "SDP",
            peer,
            data: connection.localDescription!,
          });
        }
      }
    }
  */
  };

  const ws = useWebSocket("localhost:4000", handleMessage);
};
