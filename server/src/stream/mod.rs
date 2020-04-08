extern crate gstreamer;
extern crate serde;
extern crate serde_json;
extern crate tokio;
extern crate tokio_tungstenite;
extern crate tungstenite;

mod error;

use std::collections::HashMap;
use std::marker::Unpin;

use futures::{Stream, StreamExt, TryFutureExt, TryStreamExt};
use gst::prelude::ObjectExt;
use gst::{
    ElementExt, ElementExtManual, GObjectExtManualGst, GstBinExt, GstBinExtManual, PadExtManual,
};
use gstreamer as gst;
use tokio::runtime;

use crate::signalling::{ClientMessage, ServerMessage};

pub use error::Error;

async fn handle_messages<S>(mut s: S) -> Result<(), Error>
where
    S: Stream<Item = Result<tungstenite::Message, tungstenite::Error>> + Unpin,
{
    let mut peers: HashMap<usize, ()> = HashMap::new();

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

    pipeline.set_state(gst::State::Playing).unwrap();

    let add_peer = |peers: &mut HashMap<usize, ()>, peer: &usize, polite: bool| {
        peers.insert(*peer, ());

        /* if polite {return;} */
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
            .connect("on-negotiation-needed", false, |values| {
                let webrtcbin = values[0].get::<gst::Element>().unwrap().unwrap();

                let promise = gst::Promise::new_with_change_func(|reply| {
                    let reply = reply.unwrap();
                    let offer = reply.get_value("offer").unwrap();
                    println!("{:?}", offer);
                });

                // Should signals be manually emitted?
                webrtcbin
                    .emit("create-offer", &[&None::<gst::Structure>, &promise])
                    .unwrap();
                None
            })
            .unwrap();

        pipeline.add(&bin).unwrap();
        src_pad.link(&pad).unwrap();
    };

    let bus = pipeline.get_bus().unwrap();
    let _msgs = gst::BusStream::new(&bus);

    while let Some(Ok(msg)) = s.next().await {
        match msg {
            tungstenite::Message::Text(content) => {
                let msg = serde_json::from_str::<ServerMessage>(&content)?;
                match msg {
                    ServerMessage::Hello {
                        peers: remote_peers,
                    } => {
                        remote_peers
                            .iter()
                            .for_each(|peer| add_peer(&mut peers, peer, true));
                    }
                    ServerMessage::AddPeer { peer } => {
                        add_peer(&mut peers, &peer, false);
                    }
                    ServerMessage::RemovePeer { peer } => {
                        peers.remove(&peer);
                    }
                    ServerMessage::PeerMessage { .. } => {}
                };
            }
            _ => {}
        }
    }
    Ok(())
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
