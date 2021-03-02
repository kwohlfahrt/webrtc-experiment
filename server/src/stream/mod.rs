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

use futures::channel::mpsc;

struct Stream<S: Sink<ws::Message> + Unpin + 'static> {
    peers: HashMap<usize, Peer>,
    ws: SinkWrite<ws::Message, S>,
    tx: mpsc::UnboundedSender<message::PeerMessage>,
    pipeline: gst::Pipeline,
    tees: [(&'static str, gst::Element); 2],
}

struct Peer {
    bin: gst::Bin,
    webrtcbin: gst::Bin,
}

impl<S: Sink<ws::Message> + Unpin + 'static> Stream<S> {
    pub fn new(
        ws: SinkWrite<ws::Message, S>,
        tx: mpsc::UnboundedSender<message::PeerMessage>,
    ) -> Self {
        let pipeline = gst::Pipeline::new(Some("pipeline"));
        let tees = pipeline::add_src(&pipeline, false);
        pipeline.set_state(gst::State::Playing).unwrap();

        Self {
            peers: HashMap::new(),
            ws,
            tx,
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
            .connect("on-negotiation-needed", false, {
                let tx = self.tx.clone();

                move |values| {
                    let webrtcbin = values[0].get::<gst::Element>().unwrap().unwrap();

                    let promise = gst::Promise::with_change_func({
                        let webrtcbin = webrtcbin.clone();
			let tx = tx.clone();
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

                            tx.unbounded_send(msg).unwrap();
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
                let tx = self.tx.clone();
                move |values| {
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
                    tx.unbounded_send(msg).unwrap();
                    None
                }
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
            Ok(ws::Frame::Close(_)) => {
                ctx.stop();
                return;
            }
            Ok(ws::Frame::Text(msg)) => msg,
            _ => return,
        };

        match serde_json::from_slice::<message::ServerMessage>(&msg).unwrap() {
	    message::ServerMessage::Hello { peers, .. } => {
		for peer in peers {
		    self.add_peer(peer.id, true);
		}
	    },
	    message::ServerMessage::AddPeer { peer } => {
		self.add_peer(peer.id, false);
	    },
	    message::ServerMessage::RemovePeer { peer } => {
		self.peers.remove(&peer);
	    },
	    message::ServerMessage::PeerMessage { message } => {
		let peer = message.peer;
		match message.data {
		    message::PeerMessageData::ICECandidate { data } => {
			let webrtcbin = &self.peers[&peer].bin;
			let mline_index = data["sdpMLineIndex"].as_u64().unwrap() as u32;
			let candidate = &data["candidate"].as_str().unwrap();
			if candidate.len() > 0 {
			    webrtcbin
				.emit("add-ice-candidate", &[&mline_index, &candidate])
				.unwrap();
			}
		    }
		    message::PeerMessageData::SDP { data } => {
			let sdp_type = data["type"].as_str().unwrap();
			if sdp_type == "answer" {
			    let webrtcbin = &self.peers[&peer].webrtcbin;
			    let bin = &self.peers[&peer].bin;
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
			    let webrtcbin  = &self.peers[&peer].webrtcbin;
			    let bin = &self.peers[&peer].webrtcbin;
			    let bin_pads = &self.peers[&peer].bin_pads;
			    let src_pads = &self.peers[&peer].src_pads;

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
				let tx = self.tx.clone();
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
				    tx.unbounded_send(message::PeerMessage {
					peer,
					data: message::PeerMessageData::SDP {
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


			}
		    }
		}
	    },
	    _ => (),
	    }
	}
    }

    impl<S: Sink<ws::Message> + Unpin + 'static> StreamHandler<message::PeerMessage> for Stream<S> {
	fn handle(&mut self, msg: message::PeerMessage, _: &mut Self::Context) {
	    self.ws.write(ws::Message::Text(serde_json::to_string(&msg).unwrap()));
	}
    }

    impl<S: Sink<ws::Message> + Unpin + 'static> actix::io::WriteHandler<WsProtocolError>
	for Stream<S>
    {
    }

    pub async fn main(address: &str) -> Result<(), Error> {
	let (response, conn) = Client::new().ws(address).connect().await.unwrap();
	let (tx, rx) = mpsc::unbounded::<message::PeerMessage>();

	Stream::create(|ctx| {
	    let (sink, stream) = conn.split();
	    Stream::add_stream(stream, ctx);
	    Stream::add_stream(rx, ctx);
	    Stream::new(SinkWrite::new(sink, ctx), tx)
	});

	Ok(())
    }
