import gi
gi.require_version('Gst', '1.0')
from gi.repository import Gst

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
        self.payload_filter.set_property(
            "caps", Gst.Caps(Gst.Structure("application/x-rtp", **payload_caps))
        )

        elems = self.src, self.encoder, self.payload, self.payload_filter, self.tee, self.sink
        Gst.Element.link_many(*elems)
        pipeline.add(*elems)

class AudioSrc:
    def __init__(self, pipeline):
        self.src = Gst.ElementFactory.make("audiotestsrc")
        self.encoder = Gst.ElementFactory.make("opusenc")
        self.payload = Gst.ElementFactory.make("rtpopuspay")
        self.payload_filter = Gst.ElementFactory.make("capsfilter")
        self.tee = Gst.ElementFactory.make("tee")
        self.sink = Gst.ElementFactory.make("fakesink")

        payload_caps = {
            "payload": 97,
            "media": "audio",
            "encoding-name": "OPUS",
        }

        self.src.set_property("wave", "ticks")
        self.src.set_property("is-live", True)
        self.payload_filter.set_property(
            "caps", Gst.Caps(Gst.Structure("application/x-rtp", **payload_caps))
        )

        elems = self.src, self.encoder, self.payload, self.payload_filter, self.tee, self.sink
        Gst.Element.link_many(*elems)
        pipeline.add(*elems)
