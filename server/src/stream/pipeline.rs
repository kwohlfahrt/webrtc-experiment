use gst::prelude::{ObjectExt, ToValue};
use gst::{
    ElementExt, ElementExtManual, GObjectExtManualGst, GstBinExt, GstBinExtManual, PadExtManual,
};
use gstreamer as gst;

enum SrcType {
    Video,
    Audio,
}

fn add_src_type(pipeline: &gst::Pipeline, ty: SrcType, monitor: bool) -> gst::Element {
    let (ty_name, src_name, enc_name, pay_name, depay_name, dec_name, sink_name) = match ty {
        SrcType::Video => (
            "video",
            "videotestsrc",
            "vp8enc",
            "rtpvp8pay",
            "rtpvp8depay",
            "vp8dec",
            "autovideosink",
        ),
        SrcType::Audio => (
            "audio",
            "audiotestsrc",
            "opusenc",
            "rtpopuspay",
            "rtpopusdepay",
            "opusdec",
            "autoaudiosink",
        ),
    };

    let format_name = |name| format!("{}_{}", ty_name, name);

    let src = gst::ElementFactory::find(src_name)
        .unwrap()
        .create(Some(&format_name("src")))
        .unwrap();
    src.set_property("is-live", &true).unwrap();
    match ty {
        SrcType::Video => src.set_property_from_str("pattern", &"smtpe"),
        SrcType::Audio => src.set_property_from_str("wave", &"ticks"),
    };

    let enc = gst::ElementFactory::find(enc_name)
        .unwrap()
        .create(Some(&format_name("enc")))
        .unwrap();

    let pay = gst::ElementFactory::find(pay_name)
        .unwrap()
        .create(Some(&format_name("pay")))
        .unwrap();

    let tee = gst::ElementFactory::find("tee")
        .unwrap()
        .create(Some(&format_name("tee")))
        .unwrap();

    let queue = gst::ElementFactory::find("queue")
        .unwrap()
        .create(Some(&format_name("queue")))
        .unwrap();

    let sink_factory = if monitor {
        gst::ElementFactory::find(sink_name)
    } else {
        gst::ElementFactory::find("fakesink")
    };

    let sink = sink_factory
        .unwrap()
        .create(Some(&format_name("sink")))
        .unwrap();

    sink.set_property("sync", &true).unwrap();

    pipeline
        .add_many(&[&src, &enc, &pay, &tee, &queue, &sink])
        .unwrap();
    src.link(&enc).unwrap();
    enc.link(&pay).unwrap();
    let caps = match ty {
        SrcType::Video => gst::Caps::builder("application/x-rtp")
            .field(&"payload", &96)
            .field(&"media", &"video")
            .field(&"encoding-name", &"VP8")
            .build(),
        SrcType::Audio => gst::Caps::builder("application/x-rtp")
            .field(&"payload", &97)
            .field(&"media", &"audio")
            .field(&"encoding-name", &"OPUS")
            .build(),
    };
    pay.link_filtered(&tee, Some(&caps)).unwrap();
    tee.link(&queue).unwrap();

    if monitor {
        let depay = gst::ElementFactory::find(depay_name)
            .unwrap()
            .create(Some(&format_name("depay")))
            .unwrap();
        let dec = gst::ElementFactory::find(dec_name)
            .unwrap()
            .create(Some(&format_name("dec")))
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

pub fn add_src(pipeline: &gst::Pipeline, monitor: bool) -> [(&'static str, gst::Element); 2] {
    let video_src = add_src_type(pipeline, SrcType::Video, monitor);
    let audio_src = add_src_type(pipeline, SrcType::Audio, monitor);
    [("audio", audio_src), ("video", video_src)]
}
