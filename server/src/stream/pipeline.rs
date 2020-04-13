use gst::prelude::{ObjectExt, ToValue};
use gst::{
    ElementExt, ElementExtManual, GObjectExtManualGst, GstBinExt, GstBinExtManual, PadExtManual,
};
use gstreamer as gst;

pub fn add_src(pipeline: &gst::Pipeline, monitor: bool) -> gst::Element {
    let src = gst::ElementFactory::find("videotestsrc")
        .unwrap()
        .create(Some("src"))
        .unwrap();
    src.set_property("is-live", &true).unwrap();
    src.set_property_from_str("pattern", &"smtpe");

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

    let sink_factory = if monitor {
        gst::ElementFactory::find("autovideosink")
    } else {
        gst::ElementFactory::find("fakesink")
    };

    let sink = sink_factory.unwrap().create(Some("sink")).unwrap();

    sink.set_property("sync", &true).unwrap();

    pipeline
        .add_many(&[&src, &enc, &pay, &tee, &queue, &sink])
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

    if monitor {
        let depay = gst::ElementFactory::find("rtpvp8depay")
            .unwrap()
            .create(Some("depay"))
            .unwrap();
        let dec = gst::ElementFactory::find("vp8dec")
            .unwrap()
            .create(Some("dec"))
            .unwrap();
        pipeline.add_many(&[&depay, &dec]).unwrap();

        queue.link(&depay).unwrap();
        depay.link(&dec).unwrap();
        dec.link(&sink).unwrap();
    } else {
        queue.link(&sink).unwrap();
    }

    tee
}
