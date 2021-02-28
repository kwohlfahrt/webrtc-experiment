mod error;
pub mod message;

use std::collections::HashMap;
use futures::TryFutureExt;

use actix::{
    Actor, ActorContext, ActorFuture, Addr, AsyncContext, Context, ContextFutureSpawner, Handler,
    Message, MessageResult, Running, StreamHandler, WrapFuture,
};
use actix_web::{web, App, HttpRequest, HttpServer, Responder};
use actix_web_actors::ws;
use rand::random;

pub use error::Error;
use message::{ClientMessage, Pos};

struct Server {
    clients: HashMap<usize, Client>,
}

struct Client {
    pos: Pos,
    addr: Addr<Ws>,
}

impl Actor for Server {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "Hello")]
struct Join {
    addr: Addr<Ws>,
}

impl Handler<Join> for Server {
    type Result = MessageResult<Join>;

    fn handle(&mut self, msg: Join, _: &mut Context<Self>) -> Self::Result {
        let state = message::Peer {
            id: self.clients.keys().max().map_or(1, |x| x + 1),
            pos: Pos {
                x: random::<f32>() * 800.0,
                y: random::<f32>() * 600.0,
            },
        };

        let reply = Hello {
            state,
            peers: self.clients
                .iter()
                .map(|(&id, client)| message::Peer {
                    id,
                    pos: client.pos,
                })
                .collect(),
        };

        for client in self.clients.values() {
            client.addr.do_send(AddPeer { peer: state })
        }

        self.clients.insert(
            state.id,
            Client {
                pos: state.pos,
                addr: msg.addr,
            },
        );

        MessageResult(reply)
    }
}

#[derive(Message, Copy, Clone)]
#[rtype(result = "()")]
struct Move {
    id: usize,
    pos: Pos,
}

impl std::convert::From<Move> for message::ServerMessage {
    fn from(msg: Move) -> Self {
        Self::MovePeer {
            peer: msg.id,
            pos: msg.pos,
        }
    }
}

impl Handler<Move> for Server {
    type Result = ();

    fn handle(&mut self, msg: Move, _: &mut Context<Self>) -> Self::Result {
        self.clients.entry(msg.id).and_modify(|e| e.pos = msg.pos);

        for client in self.clients.values() {
	    client.addr.do_send(msg)
	}
    }
}

impl Handler<Move> for Ws {
    type Result = ();

    fn handle(&mut self, msg: Move, ctx: &mut ws::WebsocketContext<Self>) -> Self::Result {
        let msg: message::ServerMessage = msg.into();
        ctx.text(serde_json::to_string(&msg).unwrap());
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct PeerMessage {
    source: usize,
    msg: message::PeerMessage,
}

impl Handler<PeerMessage> for Server {
    type Result = ();

    fn handle(&mut self, msg: PeerMessage, _: &mut Context<Self>) -> Self::Result {
	if let Some(peer) = self.clients.get(&msg.msg.peer) {
	    peer.addr.do_send(msg)
	}
    }
}

impl Handler<PeerMessage> for Ws {
    type Result = ();

    fn handle(&mut self, msg: PeerMessage, ctx: &mut ws::WebsocketContext<Self>) -> Self::Result {
	let msg = msg.msg.forward(msg.source);
	ctx.text(serde_json::to_string(&msg).unwrap());
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct Quit {
    id: usize,
}

impl Handler<Quit> for Server {
    type Result = ();

    fn handle(&mut self, msg: Quit, _: &mut Context<Self>) -> Self::Result {
        self.clients.remove(&msg.id);

        for client in self.clients.values() {
            client.addr.do_send(RemovePeer { peer: msg.id })
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct Hello {
    state: message::Peer,
    peers: Vec<message::Peer>,
}

impl std::convert::From<Hello> for message::ServerMessage {
    fn from(msg: Hello) -> Self {
        Self::Hello {
            state: msg.state,
            peers: msg.peers,
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct AddPeer {
    peer: message::Peer,
}

impl std::convert::From<AddPeer> for message::ServerMessage {
    fn from(msg: AddPeer) -> Self {
        Self::AddPeer { peer: msg.peer }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct RemovePeer {
    peer: usize,
}

impl std::convert::From<RemovePeer> for message::ServerMessage {
    fn from(msg: RemovePeer) -> Self {
        Self::RemovePeer { peer: msg.peer }
    }
}

struct Ws {
    id: usize,
    server: Addr<Server>,
}

impl Actor for Ws {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.server
            .send(Join {
                addr: ctx.address(),
            })
            .into_actor(self)
            .then(|hello, act, ctx| {
                match hello {
                    Ok(hello) => {
                        act.id = hello.state.id;
                        let msg: message::ServerMessage = hello.into();
                        ctx.text(serde_json::to_string(&msg).unwrap())
                    }
                    _ => ctx.stop(),
                }
                actix::fut::ready(())
            })
            .wait(ctx); // Block all other messages until we've registered.
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        self.server.do_send(Quit { id: self.id });
        Running::Stop
    }
}

impl Handler<AddPeer> for Ws {
    type Result = ();

    fn handle(&mut self, msg: AddPeer, ctx: &mut ws::WebsocketContext<Self>) -> Self::Result {
        let msg: message::ServerMessage = msg.into();
        ctx.text(serde_json::to_string(&msg).unwrap());
    }
}

impl Handler<RemovePeer> for Ws {
    type Result = ();

    fn handle(&mut self, msg: RemovePeer, ctx: &mut ws::WebsocketContext<Self>) -> Self::Result {
        let msg: message::ServerMessage = msg.into();
        ctx.text(serde_json::to_string(&msg).unwrap());
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for Ws {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Err(_) => {
                ctx.stop();
                return;
            }
            Ok(msg) => msg,
        };

        match msg {
            ws::Message::Text(text) => {
                match serde_json::from_str::<ClientMessage>(&text).unwrap() {
                    ClientMessage::Peer { message: msg } => 
                        self.server.do_send(PeerMessage {
			    source: self.id,
			    msg: msg,
			}),
                    ClientMessage::Move { pos } => self.server.do_send(Move { id: self.id, pos }),
                }
            }
            ws::Message::Close(reason) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => (),
        }
    }
}

async fn index(
    req: HttpRequest,
    stream: web::Payload,
    server: web::Data<Addr<Server>>,
) -> impl Responder {
    ws::start(
        Ws {
            id: 0,
            server: server.get_ref().clone(),
        },
        &req,
        stream,
    )
}

pub async fn main(address: &str) -> Result<(), Error> {
    let server = Server {
        clients: HashMap::new(),
    }
    .start();

    HttpServer::new(move || {
        App::new()
            .data(server.clone())
            .route("/", web::get().to(index))
    })
    .bind(address)?
    .run()
    .err_into()
    .await
}
