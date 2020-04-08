extern crate serde;
extern crate serde_json;

use serde::{Deserialize, Serialize};

// TODO: use RawValue for efficiency on pass-through data
#[derive(Debug, Serialize, Deserialize)]
pub struct ClientMessage {
    peer: usize,
    #[serde(flatten)]
    data: ClientMessageData,
}

impl ClientMessage {
    pub fn peer(&self) -> usize {
        self.peer
    }

    pub fn forward(self, source: usize) -> ServerMessage {
        ServerMessage::PeerMessage {
            message: ClientMessage {
                peer: source,
                ..self
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessageData {
    ICECandidate { data: serde_json::Value },
    SDPOffer { data: serde_json::Value },
    SDPAnswer { data: serde_json::Value },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    Hello { peers: Vec<usize> },
    AddPeer { peer: usize },
    RemovePeer { peer: usize },
    PeerMessage { message: ClientMessage },
}
