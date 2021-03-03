import asyncio
from sys import argv
import json

import websockets

import gi
gi.require_version('Gst', '1.0')
gi.require_version('GstWebRTC', '1.0')
gi.require_version('GstSdp', '1.0')
from gi.repository import Gst, GstWebRTC, GstSdp

class VideoSrc:
    def __init__(self, pipeline):
        self.src = Gst.ElementFactory.make("videotestsrc")
        self.encoder = Gst.ElementFactory.make("vp8enc")
        self.payload = Gst.ElementFactory.make("rtpvp8pay")
        self.payload_filter = Gst.ElementFactory.make("capsfilter")
        self.tee = Gst.ElementFactory.make("tee")
        self.sink = Gst.ElementFactory.make("fakesink")

        payload_caps = {
            "payload": 96,
            "media": "video",
            "encoding-name": "VP8",
        }

        self.src.set_property("pattern", "smpte")
        self.src.set_property("is-live", True)
        self.payload_filter.set_property("caps", Gst.Caps(Gst.Structure("application/x-rtp", **payload_caps)))

        elems = self.src, self.encoder, self.payload, self.payload_filter, self.tee, self.sink
        Gst.Element.link_many(*elems)
        pipeline.add(*elems)

class Peer:
    def __init__(self, src_pad, conn, polite):
        self.conn = conn
        self.polite = polite
        self.webrtc = Gst.ElementFactory.make("webrtcbin")
        self.webrtc.connect("on-negotiation-needed", self.handle_negotiation_needed)
        self.webrtc.connect("on-ice-candidate", self.handle_ice_candidate)

        if polite:
            sink_template = self.webrtc.get_pad_template("sink_%u")
            sink_pad = self.webrtc.request_pad(sink_template)

    def handle_offer_created(self, promise, *_):
        offer = promise.get_reply()['offer']
        self.webrtc.emit('set-local-description', offer, None)
        sdp = {"type": "offer", "sdp": offer.sdp.as_text()}
        msg = json.dumps({"type": "Peer", "message": {"type": "SDP", "data": sdp}})
        self.conn.send(msg)

    def handle_offer_received(self, offer):
        promise = Gst.Promise.new_with_change_func(self.handle_answer_created, element, None)
        element.emit('create-answer', None, promise)

    def handle_answer_created(self, promise, *_):
        answer = promise.get_reply()['answer']
        sdp = {"type": "answer", "sdp": answer.sdp.as_text()}
        msg = json.dumps({"type": "Peer", "message": {"type": "SDP", "data": sdp}})
        self.conn.send(msg)

    def handle_negotiation_needed(self, element):
        promise = Gst.Promise.new_with_change_func(self.handle_offer_created, element, None)
        element.emit('create-offer', None, promise)

    def handle_ice_candidate(self, _, mlineindex, candidate):
        msg = json.dumps({"type": "Peer", "message": {"type": "ICECandidate", "data": candidate}})
        self.conn.send(msg)

class Stream:
    def __init__(self, server):
        self.server = server
        self.pipeline = Gst.Pipeline()
        self.video_src = VideoSrc(self.pipeline)
        self.peers = {}
        self.conn = None

    def add_peer(self, peer_id, polite):
        template = self.video_src.tee.get_pad_template("src_%u")
        src_pad = self.video_src.tee.request_pad(template)

        peer = Peer(src_pad, self.conn, polite)
        self.pipeline.add(peer.webrtc)
        self.peers[peer_id] = peer

    async def run(self):
        self.conn = await websockets.connect(self.server)
        async for msg in self.conn:
            msg = json.loads(msg)
            print(msg)
            if msg['type'] == "Hello":
                self.id = msg['state']['id']
                for peer in msg['peers']:
                    self.add_peer(peer['id'], True)
            elif msg['type'] == "AddPeer":
                self.add_peer(msg['peer']['id'], False)
            elif msg['type'] == "PeerMessage":
                msg = msg['message']
                peer = msg['peer']
                if msg['type'] == "ICECandidate":
                    ice = msg['data']
                elif msg['type'] == "SDP":
                    sdp = msg['data']

def main():
    server = argv[1]

    Gst.init(None)
    stream = Stream(server)
    asyncio.run(stream.run())
