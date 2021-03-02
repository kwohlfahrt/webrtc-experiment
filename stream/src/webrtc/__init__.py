import asyncio
from sys import argv
import json

import websockets
import gi

gi.require_version('Gst', '1.0')
from gi.repository import Gst
gi.require_version('GstWebRTC', '1.0')
from gi.repository import GstWebRTC
gi.require_version('GstSdp', '1.0')
from gi.repository import GstSdp

class Stream:
    def __init__(self, server):
        self.server = server

    async def run(self):
        conn = await websockets.connect(self.server)
        async for msg in conn:
            if msg['type'] == "Hello":
                self.id = msg['state']['id']
                self.peers = msg['state']['peers']
                for peer in self.peers:
                    pass

def main():
    server = argv[1]

    Gst.init(None)
    asyncio.run(Stream(server).run())
