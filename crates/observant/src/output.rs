//! The output pipeline: annotated I420 frames from an appsrc into `webrtcsink` and its signalling server.

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
    appsrc.set_caps(Some(&crate::frame::caps(config)));

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};
    use std::net::{TcpListener, TcpStream};
    use std::time::{Duration, Instant};

    /// [`run`] on a thread of its own: a run that never returns fails the test instead of hanging it.
    fn run_within(pipeline: &gst::Pipeline) -> anyhow::Result<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        let pipeline = pipeline.clone();
        std::thread::spawn(move || tx.send(run(&pipeline)));
        rx.recv_timeout(Duration::from_secs(10))
            .expect("run did not return within 10 s")
    }

    #[test]
    fn run_ends_on_a_shutdown_request_and_on_end_of_stream() {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        request_shutdown(&pipeline);
        run_within(&pipeline).unwrap();
        let bus = pipeline.bus().unwrap();
        bus.post(gst::message::Eos::new()).unwrap();
        run_within(&pipeline).unwrap();
    }

    #[test]
    fn run_reports_a_pipeline_error_with_its_debug_text() {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let message = gst::message::Error::builder(gst::CoreError::Failed, "the sink fell over")
            .debug("a debug line")
            .build();
        pipeline.bus().unwrap().post(message).unwrap();
        let err = run_within(&pipeline).unwrap_err().to_string();
        assert!(
            err.contains("the sink fell over") && err.contains("a debug line"),
            "{err}"
        );
    }

    #[test]
    fn the_sender_carries_the_config_and_its_signalling_server_answers() {
        if !crate::gst_has(&["webrtcsink"]) {
            return;
        }
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let config = Config {
            name: "Futtertrog".into(),
            signalling_port: port,
            ice_port_min: 50000,
            ice_port_max: 50010,
            width: 64,
            height: 48,
            ..Config::default()
        };
        let (pipeline, appsrc) = build(&config).unwrap();
        assert_eq!(appsrc.caps().unwrap(), crate::frame::caps(&config));
        let webrtc = pipeline.by_name("ws").unwrap();
        assert_eq!(
            webrtc.property::<u32>("signalling-server-port"),
            u32::from(port)
        );
        let meta = webrtc.property::<gst::Structure>("meta");
        assert_eq!(meta.get::<String>("name").unwrap(), "Futtertrog");

        pipeline.set_state(gst::State::Playing).unwrap();
        // Generous: the riscv64 lane runs this under QEMU, many times slower than native.
        let deadline = Instant::now() + Duration::from_secs(60);
        let reply = loop {
            if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
                stream
                    .set_read_timeout(Some(Duration::from_secs(30)))
                    .unwrap();
                write!(
                    stream,
                    "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
                     Sec-WebSocket-Key: AAAAAAAAAAAAAAAAAAAAAA==\r\nSec-WebSocket-Version: 13\r\n\r\n"
                )
                .unwrap();
                let mut head = [0u8; 64];
                let n = stream.read(&mut head).unwrap_or(0);
                break String::from_utf8_lossy(&head[..n]).into_owned();
            }
            assert!(Instant::now() < deadline, "no signalling server on {port}");
            std::thread::sleep(Duration::from_millis(100));
        };
        assert!(reply.starts_with("HTTP/1.1 101"), "{reply}");
        pipeline.set_state(gst::State::Null).unwrap();
    }
}
