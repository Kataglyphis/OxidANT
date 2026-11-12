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
curl localhost:8080/visits                     # every visit of the last 30 days
curl 'localhost:8080/summary?day=2026-11-04&tz=60'  # one local day's figures; no model runs
curl -H 'content-type: application/json' -d '{"question": "How often today?", "tz_offset_min": 60}' localhost:8080/ask
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

## Questions about the visits

`/ask` and `POST /summary` let a language model talk about the visit log, through
[`crates/text`](text.md). Name the model in the config or with the flag of the same name:
`llm_model_dir` (a GenAI directory on the chain's GenAI, with `llm_provider` `cpu`, `cuda` or
`dml`), `llm_base_url` or `llm_backend` (an OpenAI-compatible server; `backends.json` comes from
`LLM_BACKENDS`), which exclude each other, and `llm_model` to override an endpoint's model.
`llm_max_new_tokens` bounds an answer (128). Nothing loads until the first question; the model then
stays loaded. Without one configured both answer 503 and say which keys to set, and `/healthz`'s
`ask` is false, which is what OmniAccelerANT's Stream page reads to offer questions.

| Request | Answer |
| --- | --- |
| `POST /ask` `{"question", "tz_offset_min"?, "language"?}` | `answer`, the `facts` it was given, `finish_reason`, `prompt_tokens`, `completion_tokens`, `thinking_chars` |
| `GET /summary?day=YYYY-MM-DD&tz=<minutes>` | `day`, `visits`, `first`, `last`, `longest_stay_s`, `total_stay_s`; no model runs |
| `POST /summary` `{"day"?, "tz_offset_min"?, "language"?}` | The same figures plus `summary` and `finish_reason`; a day without visits has `summary: null` and asks no model |

- **The model sees the log, not the picture.** `facts` is one line about the camera and one per day
  with visits, at most seven, so a month of history cannot outgrow its context. An answer is
  the model's reading of those lines and may be wrong; show `facts` beside it. The figures in
  `GET /summary` are computed, not generated.
- **A day is local to the caller.** `tz_offset_min` (`tz`) is minutes east of UTC, default 0, within
  14 hours. A daylight-saving change inside a day is not modelled. `language` is `de` or `en`.
- **One answer at a time.** A second question while one runs gets 503. A client that leaves, and
  the 300 s limit (504), cancel the answer after its current token. A failure of the model or
  endpoint is a 502 carrying its message, never an empty answer; a model that returns no words
  is a 502 naming its finish reason. A body that does not arrive within 10 s is a 408.
- **A thinking model's reasoning is not the answer.** `answer` and `summary` leave it out, and
  `/ask`'s `thinking_chars` says how many characters of the reply it was, 0 for none; what counts
  as thinking, and what is not recognised: [text.md](text.md#generating). When the reply was
  thinking only, the 502 says so: `the model gave no answer (finish_reason length): it spent its
  budget thinking (412 characters of thinking, 128 tokens); set a larger llm_max_new_tokens or
  switch thinking off at the endpoint`, or `it only thought (...)` without the budget advice when
  the model ended by itself. Thinking is switched
  off at the server, or with `llm_backend` through that backend's `request_extra`
  (`{"chat_template_kwargs": {"enable_thinking": false}}` on a Qwen server); `llm_base_url` sends
  no such key, and a model in `llm_model_dir` cannot switch it off.
- **Only the page itself.** Both POSTs spend the board's CPU, so `own_page` guards them like
  `POST /settings`, and they take JSON only, up to 4 KiB and a 1000-character question. Peers read
  neither endpoint.
- **What is not here.** Nothing sends a summary on a schedule, and no OmniAccelerANT package carries
  the GenAI library, so a packaged service reaches a model through an endpoint unless
  `ONNX_GENAI_ROOT` names a chain install. The `crates/observant` tests are not on the hosted
  mutation lane, which has no GStreamer.

## Settings

The settings come from a TOML file, `--config` or `$KATAGLYPHIS_OBSERVANT_CONFIG` (or
the older `$KATAGLYPHIS_CATCAM_CONFIG`), else `/etc/omni-accelerant/observant.toml`
(Windows: `%ProgramData%\omni-accelerant\observant.toml`, `config::system_config`; a
`catcam.toml` from before the rename is read when it is the only one there), with the
flags on top. Unknown keys are errors, `--print-config` prints the effective set, and
every earlier flag still works. `inference = "auto"` skips the model below 768 MiB of RAM
(a 512 MB board; a 1 GB Pi 3 reports 905 MiB) or when it does not load. `--web-root`
names OmniAccelerANT's web build, served on `:8080`. `width` must be a multiple of 8 and
`height` even: GStreamer pads the rows of any other I420 width, and the service reads them
tightly packed.

OmniAccelerANT packages the service as a .deb, AppImage, flatpak, MSI and MSIX. How it
picks the camera and paces the model, and the measurements behind both:
[AGENTS.md § The cat cam service](#the-cat-cam-service); its Raspberry Pi 5
runner: [§ The cat producer on a Raspberry Pi 5](#the-cat-producer-on-a-raspberry-pi-5).

The Pi runner and OmniAccelerANT's small-board bundle packager source the same
[container probe and Cargo build helper](https://github.com/Kataglyphis/OxidANT/blob/develop/scripts/linux/observant/lib/producer-container.sh).
The helper owns the shared `nerdctl` volumes and command boundaries; the Pi
runner still requires `--build` for a missing binary, while the bundle packager
builds it automatically and can select `--platform`.

## A fresh registry and webrtcsink

webrtcsink fills a process-wide codec list (gst-plugins-rs `CODECS` in
`net/webrtc/src/utils.rs`) from the encoders and payloaders registered when its pad
templates are first built. An in-process scan (`GST_REGISTRY_FORK=no`) builds them while it
registers the plugin, and it registers plugins in directory order, so on a fresh registry
webrtcsink can register before `vpx`, `openh264` and the RTP payloaders.
The list then stays empty for that process, and the stream fails with
`No codec available for encoding stream video_0`; the next start reads the finished
registry and works. Directory order differs between filesystems, which is why one host
passes and another fails. A package that carries `gst-plugin-scanner` names it in
`GST_PLUGIN_SCANNER` (the Linux launcher, or `install::adopt` on Windows), and
`kataglyphis_media::ensure_gst_initialized` then leaves `GST_REGISTRY_FORK` unset, so the
scanner builds the registry out of process.
OxidANT's own packages carry neither webrtcsink nor a scanner, and a unit test in
`crates/media/src/runtime.rs` keeps it so; OmniAccelerANT's `test-observant-package.sh` forces the
worst order on the package that carries both.
On Windows the service lists H.264 before the other codecs in its offer, and a viewer takes the
first one it knows: the Windows chain's VP8 reaches a viewer's decoder and never becomes a picture.
Other systems keep webrtcsink's own order, VP8 first.

## The cat cam service

`kataglyphis_observant` is one process doing everything an ObservANT board needs, so a
board runs it as one service unit. How to run and configure it:
[`docs/observant.md`](observant.md).

- **The camera is chosen, and chosen again.** `camera = "auto"` (the default) takes a
  Raspberry Pi camera through `rpicam-vid` piping raw I420 when `rpicam-hello
  --list-cameras` lists one, which keeps the host's own libcamera whatever its ABI;
  otherwise a CSI camera libcamera lists, then a UVC webcam (`v4l2src`, MJPEG only when
  it offers no raw format), then Media Foundation or kernel streaming on Windows. The
  Pi's `rp1-cfe`, `unicam` and ISP nodes never qualify. With no camera it streams a test
  pattern and probes again every 30 s. A capture that ends, errors or delivers nothing
  for 10 s is rebuilt with backoff, so an unplugged or late camera needs no restart.
- **Probes start only the camera device providers** (`libcameraprovider`,
  `v4l2deviceprovider`, `mfdeviceprovider`, `ksdeviceprovider`); a device monitor would
  start every provider, NDI and PipeWire included, on each probe.
  - **And only for the choice that needs them.** When `rpicam-hello` lists a camera, no
    provider starts: `libcameraprovider` loads the host's libcamera into the service,
    and on himbeere2 (2026-10-05) that brought the host's abseil, TensorFlow Lite and
    lttng into a process that runs its own glibc.
  - `test` and `image:` probe nothing.
- **`rpicam-vid` belongs to a guard** (`RpicamProcess`), which kills and reaps the
  process however the capture setup fails. Before it, a missing `rawvideoparse` (the
  `rawparse` plugin, not `videoparsersbad`) left one zombie per retry.
- **Inference is capped at `inference_fps`** (default 2) frames a second; `0` removes the
  cap. Uncapped, the detector (YOLO then) held a Pi 5's four cores at 380 %. The stream
  keeps the camera's rate.
- **Frames stay I420 from capture to output** (`frame.rs`). A Pi camera delivers I420 and
  the encoder takes it, so neither end converts. Boxes are drawn into the three planes,
  the inference thread converts only the frames it takes to RGBA (BT.601 limited range,
  what `videoconvert` assumes for these caps), and `/snapshot.jpg` converts on request.
  Asking the capture for RGBA again would convert every frame: on himbeere2 (2026-10-12)
  I420 took the idle service from 13 % to 9 % of a core, its capture thread from 5.2 %
  to 1.7 %.
- **A still, empty room idles the model** (`motion.rs`). At most once an inference period
  the capture compares a thumbnail of 8x8-block mean luma (80x60 at 640x480) with the
  last one. A frame goes to the model when at least 0.5 % of the blocks moved by more
  than 12 levels, once `idle_inference_fps` (default 0.2) allows, or, while a cat is in
  view or the last run saw one, once a `still_period` (twice the full period, since
  2026-10-13): a still cat, asleep or a false one, costs half the runs, and leaving is
  motion, which restores the full rate. `Presence` lingers past the still period, so one
  missed run never ends a visit; himbeere2's drying rack, a false cat at 0.34, went from
  69 % to 36 %. An `idle_inference_fps` at or above `inference_fps`, or an uncapped
  model, turns the gate off.
  - On himbeere2 (2026-10-11, 2 Hz, 60 s per row) a still scene without a cat took 12 %
    CPU and 11 runs, against 74 % and 100 with the gate off. The cat photo and the moving
    test pattern kept the full rate. `/healthz` counts the runs (`model_runs`).
  - A black frame, every block at luma 20 or less, skips the idle run too: a camera
    without IR sees nothing at night. Light coming on is motion. In himbeere2's dark room
    (2026-10-12) the service went from 9 % to 2 % of a core, and the model ran once in a
    minute instead of eleven times.
- **ORT runs two threads that sleep between runs.** `main` sets `KATAGLYPHIS_ORT_SPIN=0`
  unless the caller set it, and `inference_threads` (default 2, `0` one per core) becomes
  `KATAGLYPHIS_ORT_THREADS`, both before anything starts a thread. ORT's own default,
  four threads that spin-wait between runs, is for back-to-back inference, and the
  inference crate keeps it for every other caller. Measured on himbeere2 (Pi 5,
  2026-10-11; ANThology's photo at 2 Hz, 45 s per row, the live service beside it):

  | ORT setting | process CPU |
  | --- | --- |
  | spinning, 4 threads (ORT's default) | 183 % |
  | sleeping, 4 threads | 106 % |
  | sleeping, 2 threads (the default) | 81 % |
  | sleeping, 1 thread | 71 % |
  | no inference | 10 % |

  Every row found both cats, best score 0.72. Letterboxing (`KATAGLYPHIS_PREPROCESS`)
  scored the same photo 0.61, so the preprocessing stays the crate's default, stretch.
- **A cat counts after two hits in a row** (`presence.rs`), and leaves once no hit came
  for 3 s, or 2.5 inference periods when that is longer. Until then no box is drawn and
  nothing is logged. A lone detection near `score` is mostly noise: of the 72 arrivals
  the old rule logged on himbeere2 (2026-10-09 to -11, 2 Hz), 52 were a single hit, and
  42 of the 72 scored 0.2x. The journal keeps its words, `cat in view: …` on arrival and
  `no cat in view (stayed N s)`, because OmniAccelerANT's package tests grep the first.
- **It serves the page itself.** `http_port` (8080) serves `web_root`, the OmniAccelerANT
  web build, with the MIME types and COOP/COEP that `serve.sh` adds, plus an SPA fallback
  and ETags; a revalidation reads no file. A client that accepts gzip gets the
  compressible files over 1 KB gzipped (`GzipCache`): compressed on first request, kept in
  memory while the file is unchanged, with an ETag of their own and
  `Vary: accept-encoding`. On himbeere2 (2026-10-12) the boot's main files went from 20.2 MB to
  7.5 MB; the first request for the 7 MB CanvasKit spent 614 ms compressing, later ones
  2 ms. It also answers `/healthz` (camera, model runs, the cat's presence),
  serves `/snapshot.jpg` (the latest frame, boxes included, encoded on request) and forwards
  `/webrtc-ws` byte for byte to the signalling server, which listens on `127.0.0.1` by
  default now. `run-producer-pi.sh` keeps the old shape (`--signalling-host 0.0.0.0
  --http-port 0`) because `serve.sh` is that setup's web half.
- **The settings page changes `rotate`, `score` and `name` at run time** (2026-10-13).
  `POST /settings` (JSON only, any subset) goes through `Status::apply`, which validates the
  whole update before it stores any of it; `settings.json` in the state directory keeps what
  the page set and wins over the config file, key by key. Each reader takes the live value:
  the capture loop compares the angle each frame and sets the method of its `videoflip`,
  which is always in the pipeline (a pass-through at 0), so a turn needs no rebuild; the
  inference thread reads `Status::score` each run; the notifier reads `Status::name` each
  event. `webrtcsink`'s `meta` keeps the name it started with. The frame caps say square
  pixels, so `videoscale` borders a quarter turn instead of stretching it; do not drop
  `pixel-aspect-ratio` from `frame::caps`.
- **Who may read a cat cam from another origin** (2026-10-14): only the origins in its
  `peers`, and only the reads (`web::handle`'s `reads`). The page's COOP/COEP stay, so a
  peer's pictures need `Cross-Origin-Resource-Policy: cross-origin` as well as CORS. Acting endpoints never get CORS headers; `own_page` still guards them.
- **What a cat cam on a LAN must refuse** (2026-10-13). `web::own_page` guards the two
  endpoints that act, `/webrtc-ws` and `POST /settings`: a browser's `Origin` must equal the
  request's `Host`, and the host must be an IP, a dotless name, a local suffix (`.local`,
  `.lan`, `.home`, `.internal`, `.home.arpa`, `.fritz.box`) or in `allowed_hosts`. Reads stay
  open. A reverse proxy under a public name needs its name in `allowed_hosts`.
- **A helper only a `#[cfg(unix)]` path uses needs `#[cfg(unix)]` itself** (2026-10-14). The
  Windows lanes lint with `clippy --all-targets -- -D warnings`, and there an unused struct is
  an error: `I420Layout` (the rpicam path's) failed Windows arm64 run 38496162066 while every
  Linux lane passed. The Windows lint also runs on a Linux box, in about 5 minutes against
  the Windows lane's hour: `bash scripts/linux/clippy-windows.sh` as root in `:latest`
  (`--package` for another crate) installs mingw for ring's C, adds `x86_64-pc-windows-gnu`
  and runs clippy with `-D warnings` for it.
- **`rpicam-vid --codec yuv420` pads its rows** (2026-10-14) to 128 bytes, so a 480-wide
  frame has 512-byte rows, 276480 bytes. `I420Layout::rpicam` hands `rawvideoparse` the
  strides; a width that is a multiple of 128 (640) needs none, which is why 640x480 never
  showed it. Read as tight rows, the picture turns into green stripes and the motion gate
  sees motion everywhere.
- **What may start a visit** (2026-10-14, `capture::counted`): only a detection with motion
  near it (`motion::Moving`, from the pacer's looks), after `presence::outside` dropped the
  spots marked "not a cat". Test inputs that never move, such as `image:FILE`, still arrive
  once: the pacer's first look stamps every block. With the motion gate off there is no
  `Moving`, and every detection counts.
- **Per-arrival files** (`status.rs`, `Kept`): clips (`clip-<t>.mp4`) and pictures
  (`arrival-<t>.jpg`) both keep the last five by the visit log's arrival time, so `?t=`
  names the visit's own; set them after `set_cat`, which writes that time.
- **A webhook hears about every visit** (`notify.rs`). With `notify_url` set, each
  arrival and departure is POSTed as JSON (`ureq` with rustls, so https too) from the
  `cat-notify` thread, so a slow receiver cannot slow detection. Before the
  arrival goes out, `/arrival.jpg` is set to the frame the model confirmed it on, boxes
  drawn. Logs name only the URL's origin: a Home Assistant webhook's path is its secret.
- **The cat outlives a restart.** When systemd sets `$STATE_DIRECTORY` (the unit's
  `StateDirectory=`), every arrival and departure writes `visits.json` (last seen, visits,
  last score) and the arrival pictures there, each through a rename so a crash leaves no torn
  file, and the next start reads them back (a `cat.json` from before the rename too). Without
  it nothing is kept. Where no systemd provides it, the installer does: the Windows exe's
  `install::adopt` sets it to `%LOCALAPPDATA%\omni-observant` (taking over an older
  `omni-accelerant-catcam`), OmniAccelerANT's flatpak launcher to its `$XDG_STATE_HOME`.
- **The network is pinned.** Media uses UDP `ice_port_min..ice_port_max`
  (40000-40099), set on every viewer's ICE agent from `consumer-added`, and STUN is off
  unless `stun_server` names one, so a LAN stream needs nothing outside it.
- **Settings**: where they come from, unknown keys, `--print-config` and
  `inference = "auto"`: [`docs/observant.md` § *Settings*](observant.md#settings).
- **A Windows install is one folder, and the exe finds everything in it** (`install.rs`).
  - When `lib\gstreamer-1.0` sits beside the exe, `install::adopt` sets the variables the
    folder implies, first thing in `main`: `GST_PLUGIN_PATH`, an empty
    `GST_PLUGIN_SYSTEM_PATH[_1_0]`, `GST_PLUGIN_SCANNER`, `GST_REGISTRY` under
    `%LOCALAPPDATA%`, `ORT_DYLIB_PATH` and `KATAGLYPHIS_ONNX_MODEL` (`models\deim_dfine_n_coco.onnx`).
  - A variable the caller already set wins.
  - A supplied `GST_PLUGIN_SCANNER` keeps the registry scan out of process:
    `ensure_gst_initialized` then adds no `GST_REGISTRY_FORK=no`. An in-process scan can
    register webrtcsink before the encoders its codec list needs, and the stream dies with
    `No codec available for encoding stream video_0` on a fresh registry
    ([`docs/observant.md` § *A fresh registry and webrtcsink*](observant.md#a-fresh-registry-and-webrtcsink)).
  - On Linux the bundle's `observant` launcher does the same job, so `adopt` is a no-op there.
  - OmniAccelerANT's `scripts/windows/observant/Package-ObservANT.ps1` builds that folder
    and the MSI around it.
- **Logs and exit:** it logs at info (`KATAGLYPHIS_LOG_LEVEL`) and quiets the signalling
  server's per-message INFO (`WEBRTCSINK_SIGNALLING_SERVER_LOG=warn` unless set). It
  stops cleanly on SIGTERM and exits non-zero only when the output pipeline fails.

## The cat producer on a Raspberry Pi 5

`scripts/linux/observant/run-producer-pi.sh` builds and runs `crates/observant` from the
family CI image against a Pi 5 CSI camera. It moved here from OmniAccelerANT on
2026-09-15 under decision D12, because the crate it drives is this repo's;
OmniAccelerANT keeps a pointer and still owns the web half
(`scripts/linux/observant/serve.sh` over there).
The producer's container probe and Cargo build live in
`scripts/linux/observant/lib/producer-container.sh`, which OmniAccelerANT also
sources. This runner still requires `--build` before compiling a missing binary
and keeps the image's entrypoint; Omni's bundle packager builds a missing binary
automatically and overrides that entrypoint for cross-platform runs.

```bash
scripts/linux/observant/run-producer-pi.sh --build       # first run: cargo build in the image
scripts/linux/observant/run-producer-pi.sh               # start the producer
scripts/linux/observant/run-producer-pi.sh --libs-only   # refresh build/observant/hostlibs
```

Two things it deliberately does **not** spell out, because ANTfrastructure owns both and
a copy here goes stale:

- **The image reference**: it asks `third_party/ANTfrastructure/linux/scripts/ci-image-ref.sh`,
  which reads the fleet's `versions.env`. A literal `ghcr.io/…:latest` in a tracked `*.sh`
  is what the lint lane's CI-image-ref gate fails on.
- **`LD_LIBRARY_PATH`**: the runner hands in only `/hostlibs`, and the rest comes from the
  image. Its normal entrypoint restores GStreamer, appends `/opt/libcamera` and puts
  `${GCC_PREFIX}`'s runtime first; the container prologue appends what a caller-set value
  dropped (`/usr/local/lib`, OpenCV, FFmpeg) from the image's own
  `/opt/scripts/03-media/final/media-env.sh`, the same file the Dockerfiles source. The
  retyped list this replaced had `/opt/gcc-16.2.0` hard-coded, so a GCC bump upstream
  would have silently dropped the C++ runtime out of the search path.

The script's own contribution to the loader path is only `/hostlibs` (the host's
Raspberry Pi OS libcamera closure, which must win over the image's older upstream copy)
and the host multiarch directory, appended last. `collect_hostlibs` copies libcamera,
libcamera-base, libpisp and the PiSP IPA under their SONAMEs and the hub's
`elf-closure.sh --ldd` adds what they load but the C runtime (2026-10-19); a library ldd
cannot find stops the collection, where the old walk left it out.

**It runs through the image's entrypoint, not around it** (2026-09-29). Before
ANTfrastructure CON23 the entrypoint's `libcamera-env.sh` *prepended* `/opt/libcamera`
over the caller's `LD_LIBRARY_PATH`, so this runner started `bash` with `--entrypoint` to
keep `/hostlibs` ahead. Since `:latest` of 2026-09-29 the image's libcamera goes *after*
the caller's entries and GCC's runtime before them, so the bypass is gone from all three
`nerdctl run`s. The prologue refuses an older image that still puts `/opt/libcamera`
ahead of `/hostlibs`, with `pull :latest` in the message. It was proven under QEMU
first: the arm64 child, the runner's recorded `nerdctl run` with an empty stand-in
`/hostlibs`, gives GCC → GStreamer → `/hostlibs` → `/opt/libcamera` → media paths → host
multiarch, `libgstlibcamera.so` resolves `libcamera.so.0.7` from a stubbed `/hostlibs`,
and `gst-inspect-1.0 libcamerasrc` and `webrtcsink` load. The Pi 5 board run followed on
2026-10-06: `--build`, then a stream from himbeere2's CSI camera (`CHANGELOG.md`).
