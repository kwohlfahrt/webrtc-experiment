extern crate tokio;
extern crate tokio_tungstenite;
extern crate tungstenite;

use std::collections::HashMap;
use std::marker::Unpin;
use std::sync::{Arc, Mutex};

use futures::future;
use futures::{Sink, SinkExt, Stream, StreamExt, TryStreamExt};
use tokio::runtime;

#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    WebSocket(tungstenite::error::Error),
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

async fn handle_client<U, S>(
    mut s: S,
    id: usize,
    peers: &Arc<Mutex<HashMap<usize, U>>>,
) -> Result<(), Error>
where
    U: Sink<tungstenite::Message> + Unpin,
    S: Stream<Item = Result<tungstenite::Message, tungstenite::Error>> + Unpin,
{
    while let Some(Ok(msg)) = s.next().await {
        match msg {
            tungstenite::Message::Text(content) => {
                println!("Client {} says: {}", id, content);
                let msg = tungstenite::Message::Text(
                    format!("Peer {} says: Hello, client!", id)
                );
                future::join_all(
                    peers
                        .lock()
                        .unwrap()
                        .values_mut()
                        .map(|peer| peer.send(msg.clone())),
                )
                .await;
            }
            _ => (),
        }
    }
    peers.lock().unwrap().remove(&id);
    Ok(())
}

pub fn server() -> Result<(), Error> {
    let mut rt = runtime::Builder::new().enable_all().build()?;
    let clients = Arc::new(Mutex::new(HashMap::new()));
    let mut next_id = 0;

    let listener = std::net::TcpListener::bind(("::", 4000))?;
    let listener = rt
        .enter(|| tokio::net::TcpListener::from_std(listener))?
        .err_into()
        .and_then(|s| tokio_tungstenite::accept_async(s))
        .err_into()
        .map_ok(|s| {
            let id = next_id;
            next_id += 1;

            let (sink, source) = s.split();
            clients.lock().unwrap().insert(id, sink);
            (id, source)
        });

    let result = listener.try_for_each_concurrent(None, |(i, c)| handle_client(c, i, &clients));

    rt.block_on(result)
}
