extern crate gstreamer;
extern crate serde;
extern crate serde_json;
extern crate tokio;
extern crate tokio_tungstenite;
extern crate tungstenite;

mod error;

use std::collections::HashMap;
use std::marker::{Send, Unpin};
use std::sync::{Arc, Mutex};

use futures::channel::mpsc;
use futures::future::{ok, ready, try_select};
use futures::{select, FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use gst::prelude::ObjectExt;
use gst::{
    ElementExt, ElementExtManual, GObjectExtManualGst, GstBinExt, GstBinExtManual, PadExtManual,
};
use gstreamer as gst;
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
    let mut peers: HashMap<usize, ()> = HashMap::new();
    let (tx, rx) = mpsc::unbounded::<ClientMessage>();

    let src = gst::ElementFactory::find("videotestsrc")
        .unwrap()
        .create(Some("src"))
        .unwrap();
    src.set_property("is-live", &true).unwrap();

    let tee = gst::ElementFactory::find("tee")
        .unwrap()
        .create(Some("tee"))
        .unwrap();

    let fake_sink = gst::ElementFactory::find("fakesink")
        .unwrap()
        .create(Some("fake_sink"))
        .unwrap();
    fake_sink.set_property("sync", &true).unwrap();

    let pipeline = gst::Pipeline::new(Some("pipeline"));
    pipeline.add_many(&[&src, &tee, &fake_sink]).unwrap();
    src.link(&tee).unwrap();
    tee.link(&fake_sink).unwrap();

    let add_peer = |peers: &mut HashMap<usize, ()>, peer: usize, polite: bool| {
        peers.insert(peer, ());

        if polite {
            return;
        }
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

        // TODO: Use Bus & Futures here
        webrtcbin
            .connect("on-negotiation-needed", false, {
                let tx = tx.clone();
                move |values| {
                    let webrtcbin = values[0].get::<gst::Element>().unwrap().unwrap();
                    println!("Negotiation needed");

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
                            println!("SDP Offer:\n{:?}", offer);
                            tx.unbounded_send(ClientMessage {
                                peer,
                                data: ClientMessageData::SDPOffer {
                                    data: json!({
                                        "type": "offer",
                                        "sdp": offer,
                                    }),
                                },
                            })
                            .unwrap();
                        }
                    });

                    // FIXME: Should signals be manually emitted?
                    webrtcbin
                        .emit("create-offer", &[&None::<gst::Structure>, &promise])
                        .unwrap();
                    None
                }
            })
            .unwrap();

        webrtcbin
            .connect("on-ice-candidate", false, |values| {
                let webrtcbin = values[0].get::<gst::Element>().unwrap().unwrap();
                let media_index = values[1].get_some::<u32>().unwrap();
                let candidate = values[2].get::<String>().unwrap().unwrap();
                println!("ICE Candidate for media {}: \n{:?}", media_index, candidate);

                // TODO: Send to peer
                None
            })
            .unwrap();

        pipeline.add(&bin).unwrap();
        src_pad.link(&pad).unwrap();
        println!("Added peer");
        bin.sync_state_with_parent().unwrap();
    };

    pipeline.set_state(gst::State::Playing).unwrap();

    let (ws_sink, ws_src) = ws.split();
    let ws_result = ws_src.try_for_each(move |msg| {
        match msg {
            tungstenite::Message::Text(content) => {
                let msg = serde_json::from_str::<ServerMessage>(&content).unwrap();
                match msg {
                    ServerMessage::Hello {
                        peers: remote_peers,
                    } => {
                        remote_peers
                            .iter()
                            .for_each(|peer| add_peer(&mut peers, *peer, true));
                    }
                    ServerMessage::AddPeer { peer } => {
                        add_peer(&mut peers, peer, false);
                    }
                    ServerMessage::RemovePeer { peer } => {
                        peers.remove(&peer);
                    }
                    ServerMessage::PeerMessage { .. } => {}
                };
            }
            _ => {}
        };
        ok(())
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
