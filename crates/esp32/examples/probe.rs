//! Terminal probe for the ESP32-S3 companion.
//!
//! `cargo run -p kataglyphis_esp32 --example probe -- [PORT]`
//!
//! With no argument it picks the first Espressif board it finds. The board must run
//! `firmware/esp32-s3`; it prints every event for ten seconds and exercises the LED.

use std::thread;
use std::time::Duration;

use kataglyphis_esp32::{list_devices, Link, LinkEvent};

fn main() -> anyhow::Result<()> {
    let requested = std::env::args().nth(1);
    let path = match requested {
        Some(path) => path,
        None => {
            let devices = list_devices()?;
            for device in &devices {
                println!(
                    "port {} · {:?} · {:?} · espressif={}",
                    device.path, device.product, device.serial_number, device.espressif
                );
            }
            devices
                .iter()
                .find(|device| device.espressif)
                .or_else(|| devices.first())
                .map(|device| device.path.clone())
                .ok_or_else(|| anyhow::anyhow!("no USB serial port found"))?
        }
    };

    println!("opening {path}");
    let link = Link::open(&path, |event| match event {
        LinkEvent::Hello(info) => println!("hello: {info:?}"),
        LinkEvent::Telemetry(telemetry) => println!("telemetry: {telemetry:?}"),
        LinkEvent::Pong { id } => println!("pong: {id}"),
        LinkEvent::Ack { command } => println!("ack: command {command}"),
        LinkEvent::Button { pressed, count } => {
            println!("button: pressed={pressed} count={count}");
        }
        LinkEvent::Error { code } => println!("device error: {code}"),
        LinkEvent::CameraDetection {
            count,
            score_permille,
        } => println!("camera: {count} hit(s), best {score_permille} per mille"),
        LinkEvent::Disconnected(reason) => println!("disconnected: {reason}"),
    })?;

    thread::sleep(Duration::from_secs(2));
    println!("blue pulse");
    link.set_led(0, 0, 255, 1_000);
    link.ping(42);
    thread::sleep(Duration::from_secs(3));
    println!("led off");
    link.set_led(0, 0, 0, 0);
    link.notify_detection(2, 0.93);
    thread::sleep(Duration::from_secs(3));
    println!("connected={}", link.is_connected());
    Ok(())
}
