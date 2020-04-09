extern crate tokio;
extern crate tokio_tungstenite;
extern crate tungstenite;

mod error;
pub mod message;

use std::collections::HashMap;
use std::marker::Unpin;
use std::sync::{Arc, Mutex};

use futures::future;
use futures::{Sink, SinkExt, Stream, StreamExt, TryStreamExt};
use tokio::runtime;

pub use error::Error;
use message::{ClientMessage, ServerMessage};

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
                let msg = serde_json::from_str::<ClientMessage>(&content)?;
                if let Some(sink) = peers.lock()?.get_mut(&msg.peer) {
                    let msg = tungstenite::Message::Text(serde_json::to_string(&msg.forward(id))?);
                    sink.send(msg).await;
                }
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
