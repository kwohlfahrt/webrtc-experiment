extern crate gstreamer;
extern crate serde;
extern crate serde_json;
extern crate tokio;
extern crate tokio_tungstenite;
extern crate tungstenite;

mod error;
mod pipeline;

use std::collections::HashMap;
use std::marker::{Send, Unpin};

use futures::channel::mpsc;
use futures::future::{ok, try_select};
use futures::{FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use gst::prelude::{ObjectExt, ToValue};
use gst::{
    ElementExt, ElementExtManual, GObjectExtManualGst, GstBinExt, GstBinExtManual, PadExt,
    PadExtManual,
};
use gstreamer as gst;
use gstreamer_sdp as gst_sdp;
use gstreamer_webrtc as gst_webrtc;
use serde_json::json;
use tokio::runtime;

use crate::signalling::message::{ClientMessage, ClientMessageData, ServerMessage};

pub use error::Error;

async fn handle_messages<S>(ws: S) -> Result<(), Error>
where
    S: Stream<Item = Result<tungstenite::Message, tungstenite::Error>>
        + Unpin
        + Sink<tungstenite::Message, Error = tungstenite::Error>
        + Send
        + 'static,
{
    let mut peers: HashMap<usize, _> = HashMap::new();
    let (tx, rx) = mpsc::unbounded::<ClientMessage>();

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
                gst::GhostPad::new(Some(&format!("{}_{}", name, "sink")), &queue_pad).unwrap()
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

                    let promise = gst::Promise::new_with_change_func({
                        let tx = tx.clone();
                        let webrtcbin = webrtcbin.clone();
                        move |reply| {
                            let reply = reply.unwrap();
                            let offer = reply
                                .get_value("offer")
                                .unwrap()
                                .get::<gst_webrtc::WebRTCSessionDescription>()
                                .unwrap()
                                .unwrap();

                            webrtcbin
                                .emit("set-local-description", &[&offer, &None::<gst::Promise>])
                                .unwrap();

                            tx.unbounded_send(ClientMessage {
                                peer,
                                data: ClientMessageData::SDP {
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

                    tx.unbounded_send(ClientMessage {
                        peer,
                        data: ClientMessageData::ICECandidate {
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
                        } => {
                            remote_peers.iter().for_each(|peer| {
                                peers.insert(*peer, add_peer(*peer, true));
                            });
                        }
                        ServerMessage::AddPeer { peer } => {
                            peers.insert(peer, add_peer(peer, false));
                        }
                        ServerMessage::RemovePeer { peer } => {
                            peers.remove(&peer);
                        }
                        ServerMessage::PeerMessage {
                            message: ClientMessage { peer, data },
                        } => match data {
                            ClientMessageData::ICECandidate { data } => {
                                let (webrtcbin, _, _, _) = &peers[&peer];
                                let mline_index = data["sdpMLineIndex"].as_u64().unwrap() as u32;
                                let candidate = &data["candidate"].as_str().unwrap();
                                if candidate.len() > 0 {
                                    webrtcbin
                                        .emit("add-ice-candidate", &[&mline_index, &candidate])
                                        .unwrap();
                                }
                            }
                            ClientMessageData::SDP { data } => {
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

                                    let promise = gst::Promise::new_with_change_func({
                                        let tx = tx.clone();
                                        let webrtcbin = webrtcbin.clone();
                                        let bin = bin.clone();
                                        move |reply| {
                                            let reply = reply.unwrap();
                                            let answer = reply
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
                                            tx.unbounded_send(ClientMessage {
                                                peer,
                                                data: ClientMessageData::SDP {
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

pub fn server() -> Result<(), Error> {
    let mut rt = runtime::Builder::new().enable_all().build()?;
    gst::init().unwrap();

    let ws_connection = std::net::TcpStream::connect(("::", 4000))?;
    let ws_connection = rt.enter(|| tokio::net::TcpStream::from_std(ws_connection))?;
    let ws_connection = tokio_tungstenite::client_async("ws://localhost:4000", ws_connection)
        .map_ok(|(s, _)| s)
        .err_into();

    let result = ws_connection.and_then(handle_messages);
    rt.block_on(result)
}
