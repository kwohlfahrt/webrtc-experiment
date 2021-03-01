mod error;
mod pipeline;

use crate::signalling::message;
pub use error::Error;

use futures::TryFutureExt;

use actix::io::SinkWrite;
use actix::{
    Actor, ActorContext, ActorFuture, Addr, AsyncContext, Context, ContextFutureSpawner, Handler,
    Message, MessageResult, Running, StreamHandler, WrapFuture,
};
use actix_web::{web, App, HttpRequest, HttpServer, Responder};
use awc::error::WsProtocolError;
use awc::ws;
use awc::Client;
use futures::stream::StreamExt;
use futures::Sink;
use std::collections::HashMap;

use gst::prelude::{ObjectExt, ToValue};
use gst::{
    ElementExt, ElementExtManual, GObjectExtManualGst, GstBinExt, GstBinExtManual, PadExt,
    PadExtManual,
};
use gstreamer as gst;
use gstreamer_sdp as gst_sdp;
use gstreamer_webrtc as gst_webrtc;
use serde_json::json;

/*
use std::collections::HashMap;
use std::marker::{Send, Unpin};

use futures::channel::mpsc;
use futures::future::{ok, try_select};
use futures::{FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use tokio::runtime;
use tokio_tungstenite::tungstenite;

async fn handle_messages<S>(ws: S) -> Result<(), Error>
where
    S: Stream<Item = Result<tungstenite::Message, tungstenite::Error>>
        + Unpin
        + Sink<tungstenite::Message, Error = tungstenite::Error>
        + Send
        + 'static,
{
    let mut peers: HashMap<usize, _> = HashMap::new();
    let (tx, rx) = mpsc::unbounded::<PeerMessage>();

    let pipeline = gst::Pipeline::new(Some("pipeline"));
    let tees = pipeline::add_src(&pipeline, false);
    pipeline.set_state(gst::State::Playing).unwrap();

    let add_peer = |peer, polite: bool| {
        let bin = gst::Bin::new(None);
        let webrtcbin = gst::ElementFactory::find("webrtcbin")
            .unwrap()
            .create(Some("webrtcbin"))
            .unwrap();
        bin.add(&webrtcbin).unwrap();

        let tee_pads = tees
            .iter()
            .map(|(_, tee)| {
                let template = tee.get_pad_template(&"src_%u").unwrap();
                tee.request_pad(&template, None, None).unwrap()
            })
            .collect::<Vec<_>>();

        let bin_pads = tees
            .iter()
            .map(|(name, _)| {
                let queue = gst::ElementFactory::find("queue")
                    .unwrap()
                    .create(Some(&format!("{}_{}", name, "queue")))
                    .unwrap();
                bin.add(&queue).unwrap();
                queue.link(&webrtcbin).unwrap();

                let queue_pad = queue.get_static_pad("sink").unwrap();
                gst::GhostPad::with_target(Some(&format!("{}_{}", name, "sink")), &queue_pad)
                    .unwrap()
            })
            .collect::<Vec<_>>();

        for bin_pad in bin_pads.iter() {
            bin.add_pad(bin_pad).unwrap();
        }

        webrtcbin
            .connect("on-negotiation-needed", false, {
                let tx = tx.clone();
                move |values| {
                    let webrtcbin = values[0].get::<gst::Element>().unwrap().unwrap();

                    let promise = gst::Promise::with_change_func({
                        let tx = tx.clone();
                        let webrtcbin = webrtcbin.clone();
                        move |reply| {
                            let offer = reply
                                .unwrap()
                                .unwrap()
                                .get_value("offer")
                                .unwrap()
                                .get::<gst_webrtc::WebRTCSessionDescription>()
                                .unwrap()
                                .unwrap();

                            webrtcbin
                                .emit("set-local-description", &[&offer, &None::<gst::Promise>])
                                .unwrap();

                            tx.unbounded_send(PeerMessage {
                                peer,
                                data: PeerMessageData::SDP {
                                    data: json!({
                                        "type": "offer",
                                        "sdp": offer.get_sdp().as_text().unwrap(),
                                    }),
                                },
                            })
                            .unwrap();
                        }
                    });

                    webrtcbin
                        .emit("create-offer", &[&None::<gst::Structure>, &promise])
                        .unwrap();
                    None
                }
            })
            .unwrap();

        webrtcbin
            .connect("on-ice-candidate", false, {
                let tx = tx.clone();
                move |values| {
                    let media_index = values[1].get_some::<u32>().unwrap();
                    let candidate = values[2].get::<String>().unwrap().unwrap();

                    tx.unbounded_send(PeerMessage {
                        peer,
                        data: PeerMessageData::ICECandidate {
                            data: json!({
                                "sdpMLineIndex": media_index,
                                "candidate": candidate,
                            }),
                        },
                    })
                    .unwrap();
                    None
                }
            })
            .unwrap();

        pipeline.add(&bin).unwrap();
        bin.set_state(gst::State::Ready).unwrap();
        if !polite {
            for (bin_pad, tee_pad) in bin_pads.iter().zip(tee_pads.iter()) {
                tee_pad.link(bin_pad).unwrap();
            }
            bin.sync_state_with_parent().unwrap();
        }
        (webrtcbin, bin, bin_pads, tee_pads)
    };

    let (ws_sink, ws_src) = ws.split();
    let ws_result = ws_src.try_for_each({
        let tx = tx.clone();
        move |msg| {
            match msg {
                tungstenite::Message::Text(content) => {
                    let msg = serde_json::from_str::<ServerMessage>(&content).unwrap();
                    match msg {
                        ServerMessage::Hello {
                            peers: remote_peers,
                            ..
                        } => {
                            remote_peers.iter().for_each(|peer| {
                                peers.insert(peer.id, add_peer(peer.id, true));
                            });
                        }
                        ServerMessage::AddPeer { peer } => {
                            peers.insert(peer.id, add_peer(peer.id, false));
                        }
                        ServerMessage::RemovePeer { peer } => {
                            peers.remove(&peer);
                        }
                        ServerMessage::PeerMessage {
                            message: PeerMessage { peer, data },
                        } => match data {
                            PeerMessageData::ICECandidate { data } => {
                                let (webrtcbin, _, _, _) = &peers[&peer];
                                let mline_index = data["sdpMLineIndex"].as_u64().unwrap() as u32;
                                let candidate = &data["candidate"].as_str().unwrap();
                                if candidate.len() > 0 {
                                    webrtcbin
                                        .emit("add-ice-candidate", &[&mline_index, &candidate])
                                        .unwrap();
                                }
                            }
                            PeerMessageData::SDP { data } => {
                                let sdp_type = data["type"].as_str().unwrap();
                                if sdp_type == "answer" {
                                    let (webrtcbin, bin, _, _) = &peers[&peer];
                                    let answer = gst_sdp::SDPMessage::parse_buffer(
                                        data["sdp"].as_str().unwrap().as_bytes(),
                                    )
                                    .unwrap();
                                    let answer = gst_webrtc::WebRTCSessionDescription::new(
                                        gst_webrtc::WebRTCSDPType::Answer,
                                        answer,
                                    );
                                    webrtcbin
                                        .emit(
                                            "set-remote-description",
                                            &[&answer, &None::<gst::Promise>],
                                        )
                                        .unwrap();
                                    bin.sync_state_with_parent().unwrap();
                                } else if sdp_type == "offer" {
                                    let (webrtcbin, bin, bin_pads, src_pads) = &peers[&peer];
                                    let offer = gst_sdp::SDPMessage::parse_buffer(
                                        data["sdp"].as_str().unwrap().as_bytes(),
                                    )
                                    .unwrap();
                                    let offer = gst_webrtc::WebRTCSessionDescription::new(
                                        gst_webrtc::WebRTCSDPType::Offer,
                                        offer,
                                    );
                                    webrtcbin
                                        .emit(
                                            "set-remote-description",
                                            &[&offer, &None::<gst::Promise>],
                                        )
                                        .unwrap();

                                    for (bin_pad, src_pad) in bin_pads.iter().zip(src_pads) {
                                        if !src_pad.is_linked() {
                                            src_pad.link(bin_pad).unwrap();
                                        }
                                    }

                                    let promise = gst::Promise::with_change_func({
                                        let tx = tx.clone();
                                        let webrtcbin = webrtcbin.clone();
                                        let bin = bin.clone();
                                        move |reply| {
                                            let answer = reply
                                                .unwrap()
                                                .unwrap()
                                                .get_value("answer")
                                                .unwrap()
                                                .get::<gst_webrtc::WebRTCSessionDescription>()
                                                .unwrap()
                                                .unwrap();
                                            webrtcbin
                                                .emit(
                                                    "set-local-description",
                                                    &[&answer, &None::<gst::Promise>],
                                                )
                                                .unwrap();
                                            bin.sync_state_with_parent().unwrap();
                                            tx.unbounded_send(PeerMessage {
                                                peer,
                                                data: PeerMessageData::SDP {
                                                    data: json!({
                                                        "type": "answer",
                                                        "sdp": answer.get_sdp().as_text().unwrap(),
                                                    }),
                                                },
                                            })
                                            .unwrap();
                                        }
                                    });

                                    webrtcbin
                                        .emit("create-answer", &[&None::<gst::Structure>, &promise])
                                        .unwrap();
                                } else {
                                    unimplemented!();
                                }
                            }
                        },
                        ServerMessage::MovePeer { .. } => {}
                    };
                }
                _ => {}
            };
            ok(())
        }
    });

    let rx = rx
        .map(|msg| Ok::<_, Error>(tungstenite::Message::Text(serde_json::to_string(&msg)?)))
        .forward(ws_sink.sink_err_into());

    try_select(ws_result, rx)
        .then(|_| {
            pipeline.set_state(gst::State::Null).unwrap();
            ok(())
        })
        .await
}
*/

struct Stream<S: Sink<ws::Message> + Unpin + 'static> {
    peers: HashMap<usize, ()>,
    ws: SinkWrite<ws::Message, S>,
    pipeline: gst::Pipeline,
    tees: [(&'static str, gst::Element); 2],
}

impl<S: Sink<ws::Message> + Unpin + 'static> Stream<S> {
    pub fn new(ws: SinkWrite<ws::Message, S>) -> Self {
        let pipeline = gst::Pipeline::new(Some("pipeline"));
        let tees = pipeline::add_src(&pipeline, false);
        pipeline.set_state(gst::State::Playing).unwrap();

        Self {
            peers: HashMap::new(),
            ws,
            pipeline,
            tees,
        }
    }

    fn add_peer(&mut self, peer: usize, polite: bool) {
        let bin = gst::Bin::new(None);
        let webrtcbin = gst::ElementFactory::find("webrtcbin")
            .unwrap()
            .create(Some("webrtcbin"))
            .unwrap();
        bin.add(&webrtcbin).unwrap();

        let tee_pads = self
            .tees
            .iter()
            .map(|(_, tee)| {
                let template = tee.get_pad_template(&"src_%u").unwrap();
                tee.request_pad(&template, None, None).unwrap()
            })
            .collect::<Vec<_>>();

        let bin_pads = self
            .tees
            .iter()
            .map(|(name, _)| {
                let queue = gst::ElementFactory::find("queue")
                    .unwrap()
                    .create(Some(&format!("{}_{}", name, "queue")))
                    .unwrap();
                bin.add(&queue).unwrap();
                queue.link(&webrtcbin).unwrap();

                let queue_pad = queue.get_static_pad("sink").unwrap();
                gst::GhostPad::with_target(Some(&format!("{}_{}", name, "sink")), &queue_pad)
                    .unwrap()
            })
            .collect::<Vec<_>>();

        for bin_pad in bin_pads.iter() {
            bin.add_pad(bin_pad).unwrap();
        }

        webrtcbin
            .connect("on-negotiation-needed", false, move |values| {
                let webrtcbin = values[0].get::<gst::Element>().unwrap().unwrap();

                let promise = gst::Promise::with_change_func({
                    let webrtcbin = webrtcbin.clone();
                    move |reply| {
                        let offer = reply
                            .unwrap()
                            .unwrap()
                            .get_value("offer")
                            .unwrap()
                            .get::<gst_webrtc::WebRTCSessionDescription>()
                            .unwrap()
                            .unwrap();

                        webrtcbin
                            .emit("set-local-description", &[&offer, &None::<gst::Promise>])
                            .unwrap();

                        let msg = message::PeerMessage {
                            peer,
                            data: message::PeerMessageData::SDP {
                                data: json!({
                                    "type": "offer",
                                    "sdp": offer.get_sdp().as_text().unwrap(),
                                }),
                            },
                        };

                        self.ws
                            .write(ws::Message::Text(serde_json::to_string(&msg).unwrap()));
                    }
                });

                webrtcbin
                    .emit("create-offer", &[&None::<gst::Structure>, &promise])
                    .unwrap();
                None
            })
            .unwrap();

        webrtcbin
            .connect("on-ice-candidate", false, move |values| {
                let media_index = values[1].get_some::<u32>().unwrap();
                let candidate = values[2].get::<String>().unwrap().unwrap();

                let msg = message::PeerMessage {
                    peer,
                    data: message::PeerMessageData::ICECandidate {
                        data: json!({
                            "sdpMLineIndex": media_index,
                            "candidate": candidate,
                        }),
                    },
                };
                self.ws
                    .write(ws::Message::Text(serde_json::to_string(&msg).unwrap()));

                None
            })
            .unwrap();

        self.pipeline.add(&bin).unwrap();
        bin.set_state(gst::State::Ready).unwrap();
        if !polite {
            for (bin_pad, tee_pad) in bin_pads.iter().zip(tee_pads.iter()) {
                tee_pad.link(bin_pad).unwrap();
            }
            bin.sync_state_with_parent().unwrap();
        }
    }

    fn negotiate(&mut self) {}

    fn send_ice_candidate(&mut self) {}
}

impl<S: Sink<ws::Message> + Unpin + 'static> Actor for Stream<S> {
    type Context = Context<Self>;

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        Running::Stop
    }
}

impl<S: Sink<ws::Message> + Unpin + 'static> StreamHandler<Result<ws::Frame, WsProtocolError>>
    for Stream<S>
{
    fn handle(&mut self, msg: Result<ws::Frame, WsProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Err(_) => {
                ctx.stop();
                return;
            }
            Ok(ws::Frame::Close(reason)) => {
                ctx.stop();
                return;
            }
            Ok(ws::Frame::Text(msg)) => msg,
            _ => return,
        };

        match serde_json::from_slice::<message::ServerMessage>(&msg).unwrap() {
            _ => (),
        }
    }
}

impl<S: Sink<ws::Message> + Unpin + 'static> actix::io::WriteHandler<WsProtocolError>
    for Stream<S>
{
}

pub async fn main(address: &str) -> Result<(), Error> {
    let (response, conn) = Client::new().ws(address).connect().await.unwrap();

    let addr = Stream::create(|ctx| {
        let (sink, stream) = conn.split();
        Stream::add_stream(stream, ctx);
        Stream::new(SinkWrite::new(sink, ctx))
    });

    Ok(())
}
