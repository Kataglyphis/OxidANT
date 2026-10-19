# ObservANT, the cat cam

`crates/observant` is one service: the camera, the detector's cat boxes, a WebRTC stream
and the web page that plays it. It needs GStreamer and the chain-built ONNX Runtime, so
try it in the family `:latest` image:

```bash
cargo build --release -p kataglyphis_observant
target/release/kataglyphis_observant --camera test --print-config  # the settings in effect
target/release/kataglyphis_observant --camera test                 # a test pattern, no camera
curl localhost:8080/healthz                    # ok or the problem, the camera, the model's runs, the cat, the kept clips
curl -o frame.jpg localhost:8080/snapshot.jpg  # the current picture, boxes included
curl -o cat.jpg localhost:8080/arrival.jpg     # the frame the last cat arrival was seen on
curl -o cat.jpg 'localhost:8080/arrival.jpg?t=1791763273'  # one of the last five, by arrival time
curl -o cat.mp4 localhost:8080/arrival.mp4     # that arrival as H.264, 3 s before it to 7 s after
curl -o cat.mp4 'localhost:8080/arrival.mp4?t=1791763273'  # one of the last five, by arrival time
curl localhost:8080/settings                   # what the settings page can change
curl -X POST -H 'content-type: application/json' -d '{"rotate": 270, "score": 0.5}' localhost:8080/settings
curl -X POST -H 'content-type: application/json' -d '{"ignore": [[0.25, 0.75, 0.75, 1.0]]}' localhost:8080/settings  # "not a cat" here
```

## Who may see and change it

A cat arrives only where something moved in the last 10 s, and a detection over a spot in
`ignore` (fractions of the picture) never counts. `/webrtc-ws` and `POST /settings` take
only the cat cam's own page (a browser's `Origin` must match a local `Host`; a reverse
proxy's name goes into `allowed_hosts`), at most `max_viewers` tabs watch at once (8), and
the origins in `peers` may read this one's status and pictures for their overview.

## Webhooks, clips and the web build

`--notify-url <http(s) URL>` (`notify_url` in the file) POSTs a JSON event there whenever
a cat arrives or leaves, a Home Assistant webhook trigger for instance, and a `clip`
event once the arrival's video is encoded. A departure names the arrival it ends
(`arrived`, `best_score`), so a receiver can update what it sent for that arrival.
`--precompress <web build>` writes a zopfli `.gz` beside each file the server gzips and
exits; a packager runs it once, and the server then sends those files as they are.
`clip_seconds` (default 10, 0 for none) sets that video's length; it needs `openh264enc`,
`h264parse` and `mp4mux`, and without them the service records none and says so at start.

## Settings

The settings come from a TOML file, `--config` or `$KATAGLYPHIS_OBSERVANT_CONFIG` (or
the older `$KATAGLYPHIS_CATCAM_CONFIG`), else `/etc/omni-accelerant/observant.toml`
(Windows: `%ProgramData%\omni-accelerant\observant.toml`, `config::system_config`; a
`catcam.toml` from before the rename is read when it is the only one there), with the
flags on top. Unknown keys are errors, `--print-config` prints the effective set, and
every earlier flag still works. `inference = "auto"` skips the model below 768 MiB of RAM
(a 512 MB board; a 1 GB Pi 3 reports 905 MiB) or when it does not load. `--web-root`
names OmniAccelerANT's web build, served on `:8080`.

OmniAccelerANT packages the service as a .deb, AppImage, flatpak, MSI and MSIX. How it
picks the camera and paces the model, and the measurements behind both:
[AGENTS.md § The cat cam service](../AGENTS.md#the-cat-cam-service); its Raspberry Pi 5
runner: [§ The cat producer on a Raspberry Pi 5](../AGENTS.md#the-cat-producer-on-a-raspberry-pi-5).
