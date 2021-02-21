use serde::{Deserialize, Serialize};

// TODO: use RawValue for efficiency on pass-through data
#[derive(Debug, Serialize, Deserialize)]
pub struct PeerMessage {
    pub peer: usize,
    #[serde(flatten)]
    pub data: PeerMessageData,
}

impl PeerMessage {
    pub fn forward(self, source: usize) -> ServerMessage {
        ServerMessage::PeerMessage {
            message: PeerMessage {
                peer: source,
                ..self
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PeerMessageData {
    ICECandidate { data: serde_json::Value },
    SDP { data: serde_json::Value },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    Hello { state: Peer, peers: Vec<Peer> },
    AddPeer { peer: Peer },
    RemovePeer { peer: usize },
    MovePeer { peer: usize, pos: Pos },
    PeerMessage { message: PeerMessage },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Peer { message: PeerMessage },
    Move { pos: Pos },
}

#[derive(Debug, Serialize, Deserialize, Default, Copy, Clone)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Peer {
    pub id: usize,
    pub pos: Pos,
}
