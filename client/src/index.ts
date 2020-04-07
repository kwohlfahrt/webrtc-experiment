async function main() {
  const localVideo = document.getElementById("localVideo")! as any as HTMLVideoElement;
  const remoteVideo = document.getElementById("remoteVideo")! as any as HTMLVideoElement;

  const media = await navigator.mediaDevices.getUserMedia({
    audio: true,
    video: true,
  });
  localVideo.srcObject = media;

  const connections = [new RTCPeerConnection(), new RTCPeerConnection()];
  media.getTracks().forEach(track => connections[0].addTrack(track, media));

  connections[0].addEventListener("icecandidate", e =>
    e.candidate && connections[1].addIceCandidate(e.candidate),
  );
  connections[1].addEventListener("icecandidate", e =>
    e.candidate && connections[0].addIceCandidate(e.candidate),
  );
  connections[1].addEventListener("track", e => {
    e.streams.forEach(stream => (remoteVideo.srcObject = stream));
  });

  const offer = await connections[0].createOffer();
  await connections[0].setLocalDescription(offer);
  await connections[1].setRemoteDescription(offer);

  const answer = await connections[1].createAnswer();
  await connections[1].setLocalDescription(answer);
  await connections[0].setRemoteDescription(answer);

  const ws = new WebSocket("ws://localhost:4000");
  ws.addEventListener("message", (e) => {
    console.log("Server says:", e.data)
  })
  ws.addEventListener("open", () => {
    ws.send("Hello, server!");
  })
}

main();
