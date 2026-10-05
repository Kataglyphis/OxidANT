//! The output pipeline: annotated RGBA frames from an appsrc into `webrtcsink` and its signalling server.

use anyhow::{anyhow, Context as _};
use gstreamer as gst;
use gstreamer::glib;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::config::Config;

/// Name of the application message that ends [`run`], posted by the signal handler.
const SHUTDOWN: &str = "cat-cam-shutdown";

pub fn build(config: &Config) -> anyhow::Result<(gst::Pipeline, gst_app::AppSrc)> {
    let pipeline = gst::Pipeline::new();
    let appsrc = gst::ElementFactory::make("appsrc")
        .name("frames")
        .build()
        .context("appsrc")?
        .downcast::<gst_app::AppSrc>()
        .map_err(|_| anyhow!("element 'appsrc' is not an AppSrc"))?;
    appsrc.set_is_live(true);
    appsrc.set_do_timestamp(true);
    appsrc.set_format(gst::Format::Time);
    appsrc.set_caps(Some(&crate::capture::rgba_caps(config)));

    let queue = gst::ElementFactory::make("queue")
        .property("max-size-buffers", 1u32)
        .property_from_str("leaky", "downstream")
        .build()
        .context("queue")?;
    let convert = gst::ElementFactory::make("videoconvert")
        .name("out-convert")
        .build()
        .context("videoconvert")?;

    let webrtc = gst::ElementFactory::make("webrtcsink")
        .name("ws")
        .build()
        .context("webrtcsink (is the rswebrtc plugin available?)")?;
    // Built-in signalling server: a plain Element cannot set the separate signaller's `uri`.
    webrtc.set_property("run-signalling-server", true);
    webrtc.set_property("signalling-server-host", config.signalling_host.as_str());
    webrtc.set_property("signalling-server-port", u32::from(config.signalling_port));
    if let (Some(cert), Some(key)) = (&config.cert, &config.key) {
        webrtc.set_property("signalling-server-cert", cert.as_str());
        webrtc.set_property("signalling-server-key", key.as_str());
    }
    // None keeps ICE to host candidates: a LAN stream needs no outside server.
    webrtc.set_property("stun-server", config.stun());
    let meta = gst::Structure::builder("meta")
        .field("name", config.name.as_str())
        .build();
    webrtc.set_property("meta", &meta);
    if (config.ice_port_min, config.ice_port_max) != (0, 0) {
        pin_ice_ports(&webrtc, config.ice_port_min, config.ice_port_max);
    }

    pipeline
        .add_many([appsrc.upcast_ref(), &queue, &convert, &webrtc])
        .context("add elements")?;
    gst::Element::link_many([appsrc.upcast_ref(), &queue, &convert, &webrtc])
        .context("link output pipeline")?;
    Ok((pipeline, appsrc))
}

/// Each viewer's webrtcbin gathers its media ports from `min..=max`, so a firewall can open exactly those.
fn pin_ice_ports(webrtc: &gst::Element, min: u16, max: u16) {
    webrtc.connect("consumer-added", false, move |values| {
        let webrtcbin = values.get(2).and_then(|v| v.get::<gst::Element>().ok())?;
        let agent = webrtcbin.property::<glib::Object>("ice-agent");
        if agent.find_property("min-rtp-port").is_some() {
            agent.set_property("min-rtp-port", u32::from(min));
            agent.set_property("max-rtp-port", u32::from(max));
        } else {
            log::warn!("this GStreamer's ICE agent cannot pin ports; WebRTC uses any UDP port");
        }
        None
    });
}

/// Blocks until the pipeline errors, ends, or [`request_shutdown`] is called.
pub fn run(pipeline: &gst::Pipeline) -> anyhow::Result<()> {
    let bus = pipeline
        .bus()
        .ok_or_else(|| anyhow!("output pipeline has no bus"))?;
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        match msg.view() {
            gst::MessageView::Eos(..) => return Ok(()),
            gst::MessageView::Error(err) => {
                return Err(anyhow!(
                    "output pipeline: {} ({})",
                    err.error(),
                    err.debug().map(|d| d.to_string()).unwrap_or_default()
                ))
            }
            gst::MessageView::Application(app)
                if app.structure().is_some_and(|s| s.name() == SHUTDOWN) =>
            {
                return Ok(())
            }
            gst::MessageView::StateChanged(state)
                if state.src().is_some_and(|s| s.name() == "ws") =>
            {
                log::info!("webrtcsink state: {:?}", state.current());
            }
            _ => {}
        }
    }
    Ok(())
}

/// Makes [`run`] return; safe from any thread.
pub fn request_shutdown(pipeline: &gst::Pipeline) {
    let _ = pipeline.post_message(gst::message::Application::new(gst::Structure::new_empty(
        SHUTDOWN,
    )));
}
