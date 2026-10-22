use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gtk::prelude::*;
use gtk::{glib, Application, ApplicationWindow, Box, Button, Label, Picture};
use log::{error, warn};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

const APP_ID: &str = "org.kataglyphis.oxidant.gtk";

pub fn run() -> anyhow::Result<glib::ExitCode> {
    kataglyphis_media::ensure_gst_initialized()?;

    let app = Application::builder().application_id(APP_ID).build();
    let session = Rc::new(RefCell::new(None::<GuiSession>));
    let active = Rc::clone(&session);
    let failure = Rc::new(RefCell::new(None));
    let startup_failure = Rc::clone(&failure);
    app.connect_activate(move |app| {
        if let Some(session) = active.borrow().as_ref() {
            session.window.present();
            return;
        }
        match try_build_ui(app) {
            Ok(session) => *active.borrow_mut() = Some(session),
            Err(err) => {
                *startup_failure.borrow_mut() = Some(err);
                app.quit();
            }
        }
    });
    app.connect_shutdown(move |_| drop(session.borrow_mut().take()));
    let code = app.run_with_args::<&str>(&[]);
    if let Some(err) = failure.borrow_mut().take() {
        return Err(err);
    }
    Ok(code)
}

struct GuiSession {
    window: ApplicationWindow,
    pipeline: Arc<gst::Pipeline>,
    _bus_watch: gst::bus::BusWatchGuard,
}

impl Drop for GuiSession {
    fn drop(&mut self) {
        if let Err(err) = self.pipeline.set_state(gst::State::Null) {
            error!("Failed to stop GTK pipeline: {err}");
        }
    }
}

fn try_build_ui(app: &Application) -> Result<GuiSession> {
    let pipeline = gst::Pipeline::with_name("video-pipeline");

    let src = gst::ElementFactory::make("autovideosrc")
        .build()
        .context("Failed to create autovideosrc")?;

    let convert = gst::ElementFactory::make("videoconvert")
        .build()
        .context("Failed to create videoconvert")?;

    let (sink, paintable) = make_video_sink()?;

    pipeline
        .add_many([&src, &convert, &sink])
        .context("Failed to add elements to pipeline")?;

    gst::Element::link_many([&src, &convert, &sink]).context("Failed to link elements")?;

    let play_button = Button::with_label("Play");
    let pause_button = Button::with_label("Pause");
    let stop_button = Button::with_label("Stop");

    let button_box = Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .halign(gtk::Align::Center)
        .build();

    button_box.append(&play_button);
    button_box.append(&pause_button);
    button_box.append(&stop_button);

    let main_box = Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();

    if let Some(paintable) = paintable {
        let picture = Picture::builder()
            .paintable(&paintable)
            .width_request(640)
            .height_request(480)
            .can_shrink(false)
            .build();
        main_box.append(&picture);
    } else {
        let info = Label::new(Some(
            "Using a native video sink window (not embedded in GTK).",
        ));
        info.set_margin_top(6);
        info.set_margin_bottom(6);
        main_box.append(&info);
    }
    main_box.append(&button_box);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("OxidANT — GTK4 camera")
        .default_width(640)
        .default_height(540)
        .child(&main_box)
        .build();

    let pipeline = Arc::new(pipeline);

    let pipeline_play = Arc::clone(&pipeline);
    play_button.connect_clicked(move |_| {
        if let Err(err) = pipeline_play.set_state(gst::State::Playing) {
            error!("Failed to set pipeline to Playing: {err}");
        }
    });

    let pipeline_pause = Arc::clone(&pipeline);
    pause_button.connect_clicked(move |_| {
        if let Err(err) = pipeline_pause.set_state(gst::State::Paused) {
            error!("Failed to set pipeline to Paused: {err}");
        }
    });

    let pipeline_stop = Arc::clone(&pipeline);
    stop_button.connect_clicked(move |_| {
        if let Err(err) = pipeline_stop.set_state(gst::State::Null) {
            error!("Failed to set pipeline to Null: {err}");
        }
    });

    let bus = pipeline.bus().context("Pipeline has no bus")?;
    let pipeline_weak = pipeline.downgrade();

    let bus_watch = bus
        .add_watch_local(move |_, msg| {
            use gst::MessageView;

            match msg.view() {
                MessageView::Error(err) => {
                    error!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                }
                MessageView::Warning(warning) => {
                    warn!(
                        "Warning from {:?}: {} ({:?})",
                        warning.src().map(|s| s.path_string()),
                        warning.error(),
                        warning.debug()
                    );
                }
                MessageView::Eos(..) => {
                    if let Some(pipeline) = pipeline_weak.upgrade() {
                        if let Err(err) = pipeline.set_state(gst::State::Null) {
                            error!("Failed to stop pipeline on EOS: {err}");
                        }
                    }
                }
                _ => (),
            }

            glib::ControlFlow::Continue
        })
        .context("Failed to add bus watch")?;

    let pipeline_cleanup = Arc::clone(&pipeline);
    window.connect_close_request(move |_| {
        if let Err(err) = pipeline_cleanup.set_state(gst::State::Null) {
            error!("Failed to stop pipeline on close: {err}");
        }
        glib::Propagation::Proceed
    });

    window.present();
    Ok(GuiSession {
        window,
        pipeline,
        _bus_watch: bus_watch,
    })
}

fn make_video_sink() -> Result<(gst::Element, Option<gtk::gdk::Paintable>)> {
    if let Ok(sink) = gst::ElementFactory::make("gtk4paintablesink").build() {
        let paintable = sink.property::<gtk::gdk::Paintable>("paintable");
        return Ok((sink, Some(paintable)));
    }

    let sink = gst::ElementFactory::make("autovideosink")
        .build()
        .context("Failed to create a video sink")?;
    Ok((sink, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtk_window_keeps_bus_watch_until_session_shutdown() {
        if std::env::var_os("DISPLAY").is_none() {
            assert!(
                std::env::var_os("KATAGLYPHIS_REQUIRE_GTK").is_none(),
                "GTK test needs Xvfb"
            );
            eprintln!("GTK test needs DISPLAY; CI runs it under Xvfb");
            return;
        }
        gtk::init().unwrap();
        kataglyphis_media::ensure_gst_initialized().unwrap();
        let app = Application::builder()
            .application_id("org.kataglyphis.oxidant.gtk.test")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gtk::gio::Cancellable>).unwrap();
        let session = try_build_ui(&app).unwrap();
        session.pipeline.set_state(gst::State::Ready).unwrap();
        session
            .pipeline
            .post_message(gst::message::Eos::new())
            .unwrap();
        let context = glib::MainContext::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while session.pipeline.current_state() != gst::State::Null
            && std::time::Instant::now() < deadline
        {
            context.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            session.pipeline.current_state(),
            gst::State::Null,
            "EOS was not observed"
        );
        let pipeline = Arc::clone(&session.pipeline);
        pipeline.set_state(gst::State::Ready).unwrap();
        session.window.close();
        drop(session);
        assert_eq!(pipeline.current_state(), gst::State::Null);
    }
}
