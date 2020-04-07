extern crate serde;
extern crate serde_json;
extern crate tokio;
extern crate tokio_tungstenite;
extern crate tungstenite;

use std::collections::HashMap;
use std::marker::Unpin;
use std::sync::{Arc, Mutex};

use futures::future;
use futures::{Sink, SinkExt, Stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio::runtime;

#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    WebSocket(tungstenite::error::Error),
    JSON(serde_json::error::Error),
    Poison,
}

// TODO: use RawValue for efficiency on pass-through data

#[derive(Debug, Serialize, Deserialize)]
struct ClientMessage {
    peer: usize,
    #[serde(flatten)]
    data: ClientMessageData,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ClientMessageData {
    ICECandidate { data: serde_json::Value },
    SDPOffer { data: serde_json::Value },
    SDPAnswer { data: serde_json::Value },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ServerMessage {
    Hello { peers: Vec<usize> },
    AddPeer { peer: usize },
    RemovePeer { peer: usize },
    PeerMessage { message: ClientMessage },
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IO(e)
    }
}

impl From<tungstenite::error::Error> for Error {
    fn from(e: tungstenite::error::Error) -> Self {
        Error::WebSocket(e)
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Error::Poison
    }
}

impl From<serde_json::error::Error> for Error {
    fn from(e: serde_json::error::Error) -> Self {
        Error::JSON(e)
    }
}

async fn handle_client<U, S>(
    mut s: S,
    id: usize,
    peers: &Arc<Mutex<HashMap<usize, U>>>,
) -> Result<(), Error>
where
    U: Sink<tungstenite::Message> + Unpin,
    S: Stream<Item = Result<tungstenite::Message, tungstenite::Error>> + Unpin,
{
    let msg = tungstenite::Message::Text(serde_json::to_string(&ServerMessage::Hello {
        peers: peers
            .lock()?
            .keys()
            .copied()
            .filter(|&peer_id| peer_id != id)
            .collect(),
    })?);
    if let Some(sink) = peers.lock()?.get_mut(&id) {
        sink.send(msg).await;
    }

    let msg =
        tungstenite::Message::Text(serde_json::to_string(&ServerMessage::AddPeer { peer: id })?);
    future::join_all(
        peers
            .lock()?
            .iter_mut()
            .filter(|(&peer_id, _)| peer_id != id)
            .map(|(_, peer)| peer.send(msg.clone())),
    )
    .await;

    while let Some(Ok(msg)) = s.next().await {
        match msg {
            tungstenite::Message::Text(content) => {
                match serde_json::from_str(&content)? {
                    ClientMessage { peer, data } => {
                        if let Some(sink) = peers.lock()?.get_mut(&peer) {
                            let msg = tungstenite::Message::Text(serde_json::to_string(
                                &ServerMessage::PeerMessage {
                                    message: ClientMessage { peer: id, data },
                                },
                            )?);
                            sink.send(msg).await;
                        }
                    }
                };
            }
            _ => (),
        }
    }

    peers.lock().unwrap().remove(&id);
    let msg = tungstenite::Message::Text(serde_json::to_string(&ServerMessage::RemovePeer {
        peer: id,
    })?);
    future::join_all(
        peers
            .lock()?
            .values_mut()
            .map(|peer| peer.send(msg.clone())),
    )
    .await;
    Ok(())
}

pub fn server() -> Result<(), Error> {
    let mut rt = runtime::Builder::new().enable_all().build()?;
    let clients = Arc::new(Mutex::new(HashMap::new()));

    let ws_listener = std::net::TcpListener::bind(("::", 4000))?;
    let ws_listener = rt
        .enter(|| tokio::net::TcpListener::from_std(ws_listener))?
        .err_into()
        .and_then(|s| tokio_tungstenite::accept_async(s))
        .err_into()
        .enumerate()
        .map(|(id, s)| {
            s.map(|s| {
                let (sink, source) = s.split();
                clients.lock().unwrap().insert(id, sink);
                (id, source)
            })
        });

    let result = ws_listener.try_for_each_concurrent(None, |(i, c)| handle_client(c, i, &clients));

    rt.block_on(result)
}
