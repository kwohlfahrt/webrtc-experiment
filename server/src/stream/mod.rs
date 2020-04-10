extern crate gstreamer;
extern crate serde;
extern crate serde_json;
extern crate tokio;
extern crate tokio_tungstenite;
extern crate tungstenite;

mod error;

use std::collections::HashMap;
use std::marker::{Send, Unpin};

use futures::channel::mpsc;
use futures::future::{ok, try_select};
use futures::{FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use gst::prelude::ObjectExt;
use gst::{
    ElementExt, ElementExtManual, GObjectExtManualGst, GstBinExt, GstBinExtManual, PadExtManual,
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

    let src = gst::ElementFactory::find("videotestsrc")
        .unwrap()
        .create(Some("src"))
        .unwrap();
    src.set_property("is-live", &true).unwrap();

    let enc = gst::ElementFactory::find("vp8enc")
        .unwrap()
        .create(Some("enc"))
        .unwrap();

    let pay = gst::ElementFactory::find("rtpvp8pay")
        .unwrap()
        .create(Some("pay"))
        .unwrap();

    let tee = gst::ElementFactory::find("tee")
        .unwrap()
        .create(Some("tee"))
        .unwrap();

    let queue = gst::ElementFactory::find("queue")
        .unwrap()
        .create(Some("queue"))
        .unwrap();

    let fake_sink = gst::ElementFactory::find("fakesink")
        .unwrap()
        .create(Some("fake_sink"))
        .unwrap();
    fake_sink.set_property("sync", &true).unwrap();

    let pipeline = gst::Pipeline::new(Some("pipeline"));
    pipeline
        .add_many(&[&src, &enc, &pay, &tee, &queue, &fake_sink])
        .unwrap();
    src.link(&enc).unwrap();
    enc.link(&pay).unwrap();
    let caps = gst::Caps::builder("application/x-rtp")
        .field(&"payload", &96)
        .field(&"media", &"video")
        .field(&"encoding-name", &"VP8")
        .build();
    pay.link_filtered(&tee, Some(&caps)).unwrap();
    tee.link(&queue).unwrap();
    queue.link(&fake_sink).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();

    let add_peer = |peer, polite: bool| {
        let queue = gst::ElementFactory::find("queue")
            .unwrap()
            .create(Some("queue"))
            .unwrap();

        let webrtcbin = gst::ElementFactory::find("webrtcbin")
            .unwrap()
            .create(Some("webrtcbin"))
            .unwrap();

        let bin = gst::Bin::new(None);
        bin.add_many(&[&queue, &webrtcbin]).unwrap();
        queue.link(&webrtcbin).unwrap();

        let pad = {
            let pad = queue.get_static_pad("sink").unwrap();
            gst::GhostPad::new(Some("sink"), &pad).unwrap()
        };
        bin.add_pad(&pad).unwrap();

        let src_pad = {
            let template = tee.get_pad_template(&"src_%u").unwrap();
            tee.request_pad(&template, None, None).unwrap()
        };

        // TODO: Use Bus & Futures here?
        webrtcbin
            .connect("on-negotiation-needed", false, {
                let tx = tx.clone();
                move |values| {
                    let webrtcbin = values[0].get::<gst::Element>().unwrap().unwrap();

                    let promise = gst::Promise::new_with_change_func({
                        let tx = tx.clone();
                        move |reply| {
                            let reply = reply.unwrap();
                            let offer = reply
                                .get_value("offer")
                                .unwrap()
                                .get::<gst_webrtc::WebRTCSessionDescription>()
                                .unwrap()
                                .unwrap()
                                .get_sdp()
                                .as_text()
                                .unwrap();
                            tx.unbounded_send(ClientMessage {
                                peer,
                                data: ClientMessageData::SDP {
                                    data: json!({
                                        "type": "offer",
                                        "sdp": offer,
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

        webrtcbin.connect_pad_added({
            let bin = bin.clone();
            move |_, pad| {
                let fake_sink = gst::ElementFactory::find("fakesink")
                    .unwrap()
                    .create(Some("fake_sink"))
                    .unwrap();
                let sink_pad = fake_sink.get_static_pad("sink").unwrap();
                bin.add(&fake_sink).unwrap();
                pad.link(&sink_pad).unwrap();
                fake_sink.sync_state_with_parent().unwrap();
            }
        });

        pipeline.add(&bin).unwrap();
        bin.sync_state_with_parent().unwrap();
        if !polite {
            src_pad.link(&pad).unwrap();
        }
        (webrtcbin, pad, src_pad)
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
                                let mline_index = data["sdpMLineIndex"].as_u64().unwrap() as u32;
                                let candidate = &data["candidate"].as_str().unwrap();
                                let webrtcbin = &peers[&peer].0;
                                webrtcbin
                                    .emit("add-ice-candidate", &[&mline_index, &candidate])
                                    .unwrap();
                            }
                            ClientMessageData::SDP { data } => {
                                let sdp_type = data["type"].as_str().unwrap();
                                if sdp_type == "answer" {
                                    let webrtcbin = &peers[&peer].0;
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
                                } else if sdp_type == "offer" {
                                    let (webrtcbin, pad, src_pad) = &peers[&peer];
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

                                    src_pad.link(pad).unwrap();

                                    let promise = gst::Promise::new_with_change_func({
                                        let tx = tx.clone();
                                        let webrtcbin = webrtcbin.clone();
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
                                            let answer = answer.get_sdp().as_text().unwrap();
                                            tx.unbounded_send(ClientMessage {
                                                peer,
                                                data: ClientMessageData::SDP {
                                                    data: json!({
                                                        "type": "answer",
                                                        "sdp": answer,
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
