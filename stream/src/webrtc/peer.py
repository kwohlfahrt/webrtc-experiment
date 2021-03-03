import json
import asyncio

import gi
gi.require_version('Gst', '1.0')
gi.require_version('GstWebRTC', '1.0')
gi.require_version('GstSdp', '1.0')
from gi.repository import Gst, GstWebRTC, GstSdp

class Peer:
    def __init__(self, peer_id, webrtc, conn):
        self.peer_id = peer_id
        self.loop = asyncio.get_event_loop()
        self.conn = conn
        self.webrtc = webrtc
        self.webrtc.connect("on-negotiation-needed", self.handle_negotiation_needed)
        self.webrtc.connect("on-ice-candidate", self.handle_ice_candidate)

    def send(self, msg):
        msg = msg.copy()
        msg.update({"peer": self.peer_id})
        msg = json.dumps({"type": "Peer", "message": msg})
        asyncio.run_coroutine_threadsafe(self.conn.send(msg), self.loop).result()

    def handle_offer_created(self, promise, *_):
        reply = promise.get_reply() # This object must be kept alive
        offer = reply['offer']
        answer = reply['answer']
        sdp = {"type": "offer", "sdp": offer.sdp.as_text()}
        self.webrtc.emit('set-local-description', offer, None)
        self.send({"type": "SDP", "data": sdp})

    def handle_answer_created(self, promise, *_):
        reply = promise.get_reply() # This object must be kept alive
        answer = reply['answer']
        sdp = {"type": "answer", "sdp": answer.sdp.as_text()}
        self.webrtc.emit('set-local-description', answer, None)
        self.webrtc.sync_state_with_parent()
        self.send({"type": "SDP", "data": sdp})

    def handle_negotiation_needed(self, element):
        promise = Gst.Promise.new_with_change_func(self.handle_offer_created, element, None)
        element.emit('create-offer', None, promise)

    def handle_ice_candidate(self, _, m_line_index, candidate):
        ice = {"candidate": candidate, "sdpMLineIndex": m_line_index}
        self.send({"type": "ICECandidate", "data": ice})

    # Add an ICE candidate from a peer
    def add_ice_candidate(self, ice):
        m_line_index = ice['sdpMLineIndex']
        candidate = ice['candidate']
        if candidate:
            self.webrtc.emit("add-ice-candidate", m_line_index, candidate)

    # Add an SDP offer from a peer
    def apply_sdp(self, sdp):
        if sdp['type'] == "offer":
            description_type = GstWebRTC.WebRTCSDPType.OFFER
        elif sdp['type'] == "answer":
            description_type = GstWebRTC.WebRTCSDPType.ANSWER
        _, sdpmsg = GstSdp.SDPMessage.new_from_text(sdp["sdp"])
        description = GstWebRTC.WebRTCSessionDescription.new(description_type, sdpmsg)
        self.webrtc.emit("set-remote-description", description, None)

        if sdp["type"] == "offer":
            element = self.webrtc
            promise = Gst.Promise.new_with_change_func(self.handle_answer_created, element, None)
            element.emit('create-answer', None, promise)
        elif sdp["type"] == "answer":
            self.webrtc.sync_state_with_parent()
