mod error;
pub mod message;

use std::collections::HashMap;
use std::marker::Unpin;
use std::sync::{Arc, Mutex};

use futures::{future, TryFutureExt};
use futures::{Sink, SinkExt, Stream, StreamExt, TryStreamExt};
use tokio::runtime;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_tungstenite::tungstenite;

pub use error::Error;
use message::{ClientMessage, Pos, ServerMessage};

struct Peer<U> {
    pos: Pos,
    sink: U,
}

async fn handle_client<U, S>(
    mut s: S,
    id: usize,
    peers: &Arc<Mutex<HashMap<usize, Peer<U>>>>,
) -> Result<(), Error>
where
    U: Sink<tungstenite::Message> + Unpin,
    Error: From<U::Error>,
    S: Stream<Item = Result<tungstenite::Message, tungstenite::Error>> + Unpin,
{
    let pos = peers
        .lock()?
        .get(&id)
        .map(|peer| peer.pos)
        .unwrap_or_default();

    let msg = tungstenite::Message::Text(serde_json::to_string(&ServerMessage::Hello {
        state: message::Peer { id, pos },
        peers: peers
            .lock()?
            .iter()
            .filter(|(&peer_id, _)| peer_id != id)
            .map(|(id, peer)| message::Peer {
                id: *id,
                pos: peer.pos,
            })
            .collect(),
    })?);

    if let Some(peer) = peers.lock()?.get_mut(&id) {
        peer.sink.send(msg).await?;
    }

    let msg = tungstenite::Message::Text(serde_json::to_string(&ServerMessage::AddPeer {
        peer: message::Peer { id, pos },
    })?);

    future::join_all(
        peers
            .lock()?
            .iter_mut()
            .filter(|(&peer_id, _)| peer_id != id)
            .map(|(_, peer)| peer.sink.send(msg.clone())),
    )
    .await;

    while let Some(Ok(msg)) = s.next().await {
        match msg {
            tungstenite::Message::Text(content) => {
                let msg = serde_json::from_str::<ClientMessage>(&content)?;
                if let Some(peer) = peers.lock()?.get_mut(&msg.peer) {
                    let msg = tungstenite::Message::Text(serde_json::to_string(&msg.forward(id))?);
                    peer.sink.send(msg).await?;
                }
            }
            tungstenite::Message::Close(_) => {
                break;
            }
            _ => {}
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
            .map(|peer| peer.sink.send(msg.clone())),
    )
    .await;

    Ok(())
}

pub fn main() -> Result<(), Error> {
    let rt = runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let clients = Arc::new(Mutex::new(HashMap::new()));

    let listener = tokio::net::TcpListener::bind(("::", 4000))
        .map_ok(TcpListenerStream::new)
        .try_flatten_stream()
        .err_into()
        .and_then(|s| tokio_tungstenite::accept_async(s).err_into())
        .enumerate()
        .map(|(id, s)| {
            s.map(|s| {
                let (sink, source) = s.split();
                let pos = Pos {
                    x: rand::random::<f32>() * 800.0,
                    y: rand::random::<f32>() * 600.0,
                };

                clients.lock().unwrap().insert(id, Peer { pos, sink });
                (id, source)
            })
        });

    let result = listener.try_for_each_concurrent(None, |(i, c)| handle_client(c, i, &clients));

    rt.block_on(result)
}
