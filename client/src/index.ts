type ServerMessage =
  | {
      type: "Hello";
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
      message: ClientMessage;
    };

type ClientMessage = {
  peer: number;
} & (
  | {
      type: "ICECandidate";
      data: RTCIceCandidateInit;
    }
  | {
      type: "SDP";
      data: RTCSessionDescriptionInit;
    });

interface Peer {
  element: HTMLElement;
  connection: RTCPeerConnection;
}

async function call(
  server: string,
  container: HTMLElement,
  media: MediaStream,
) {
  const ws = new WebSocket(`ws://${server}`);
  function sendMessage(msg: ClientMessage) {
    return ws.send(JSON.stringify(msg));
  }
  const knownPeers = new Map<number, Peer>();

  async function add_peer(id: number, polite: boolean) {
    const connection = new RTCPeerConnection();
    const video = document.createElement("video");
    video.autoplay = true;

    connection.addEventListener("icecandidate", e => {
      if (e.candidate) {
        sendMessage({ type: "ICECandidate", peer: id, data: e.candidate });
      }
    });
    connection.addEventListener("track", e => {
      e.streams.forEach(stream => (video.srcObject = stream));
    });
    connection.addEventListener("negotiationneeded", async () => {
      await connection.setLocalDescription(await connection.createOffer());
      sendMessage({
        type: "SDP",
        peer: id,
        data: connection.localDescription!,
      });
    });

    if (!polite) {
      media.getTracks().forEach(track => connection.addTrack(track, media));
    }

    const child = document.createElement("div");
    child.appendChild(video);
    const caption = document.createElement("div");
    caption.innerHTML = `Peer ${id}`;
    child.appendChild(caption);
    container.appendChild(child);
    knownPeers.set(id, {
      element: child,
      connection,
    });
  }

  function remove_peer(id: number) {
    const peer = knownPeers.get(id);
    if (peer != null) {
      knownPeers.delete(id);
      container.removeChild(peer.element);
      peer.connection.close();
    }
  }

  ws.addEventListener("message", async e => {
    const data = JSON.parse(e.data) as ServerMessage;
    if (data.type == "Hello") {
      const { peers } = data;
      await Promise.all(peers.map(p => add_peer(p, true)));
    } else if (data.type == "AddPeer") {
      const { peer } = data;
      await add_peer(peer, false);
    } else if (data.type == "RemovePeer") {
      const { peer } = data;
      remove_peer(peer);
    } else if (data.type == "PeerMessage") {
      const { peer } = data.message;
      const { connection } = knownPeers.get(peer)!;
      if (data.message.type == "ICECandidate") {
        await connection.addIceCandidate(data.message.data);
      } else if (data.message.type == "SDP") {
        const sdp = data.message.data;
        if (sdp.type == "answer") {
          await connection.setRemoteDescription(sdp);
        } else if (sdp.type == "offer") {
          await connection.setRemoteDescription(sdp);
          media.getTracks().forEach(track => connection.addTrack(track, media));
          await connection.setLocalDescription(await connection.createAnswer());
          sendMessage({
            type: "SDP",
            peer,
            data: connection.localDescription!,
          });
        }
      }
    }
  });
}

async function main() {
  const monitorVideo = (document.getElementById(
    "monitor",
  )! as any) as HTMLVideoElement;
  const media = await navigator.mediaDevices.getUserMedia({
    audio: true,
    video: true,
  });
  monitorVideo.srcObject = media;

  const remoteVideos = document.getElementById("remotes")!;
  await call("prodo-laptop.home:4000", remoteVideos, media);
}

document.addEventListener("DOMContentLoaded", () => main());
