import asyncio
from sys import argv
import json

import websockets

import gi
gi.require_version('Gst', '1.0')
from gi.repository import Gst

from .src import AudioSrc, VideoSrc
from .peer import Peer

class Stream:
    def __init__(self, server):
        self.server = server
        self.pipeline = Gst.Pipeline()
        self.video_src = VideoSrc(self.pipeline)
        self.audio_src = AudioSrc(self.pipeline)
        self.peers = {}
        self.conn = None
        self.pipeline.set_state(Gst.State.PLAYING)

    def add_peer(self, peer_id, polite):
        template = self.video_src.tee.get_pad_template("src_%u")
        src_pad = self.video_src.tee.request_pad(template)
        webrtc = Gst.ElementFactory.make("webrtcbin")
        self.pipeline.add(webrtc)
        self.peers[peer_id] = Peer(peer_id, webrtc, self.conn)

        self.audio_src.tee.link(webrtc)
        webrtc.sync_state_with_parent()

    async def run(self):
        self.conn = await websockets.connect(self.server)
        async for msg in self.conn:
            msg = json.loads(msg)
            if msg['type'] == "Hello":
                self.id = msg['state']['id']
                for peer in msg['peers']:
                    self.add_peer(peer['id'], True)
            elif msg['type'] == "AddPeer":
                self.add_peer(msg['peer']['id'], False)
            elif msg['type'] == "PeerMessage":
                msg = msg['message']
                peer = self.peers[msg['peer']]
                if msg['type'] == "ICECandidate":
                    peer.add_ice_candidate(msg['data'])
                elif msg['type'] == "SDP":
                    peer.apply_sdp(msg['data'])

def main():
    server = argv[1]

    Gst.init(None)
    stream = Stream(server)
    asyncio.run(stream.run())
