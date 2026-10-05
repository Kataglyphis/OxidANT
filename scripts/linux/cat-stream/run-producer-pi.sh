#!/usr/bin/env bash
# Cat producer on a Pi CSI camera; the host libcamera stack is mounted over the image's, which the Pi 5 kernel outgrew.
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/../../.." && pwd)"

# The image ref comes from versions.env, in two assignments so `set -e` sees a failed substitution.
# shellcheck source=../lib/antfrastructure.sh
source "${script_dir}/../lib/antfrastructure.sh"
_ci_image_ref_sh="$(antfrastructure_path linux/scripts/ci-image-ref.sh)"
image="$(bash "${_ci_image_ref_sh}")"
target_volume="kataglyphis-cat-target"
cargo_volume="kataglyphis-cat-cargo"
container_name="cat-producer"
build_dir="${repo_root}/build/cat-stream"
hostlibs_dir="${build_dir}/hostlibs"
producer="/cargo-target/release/kataglyphis_cat_webrtc"
port=8443
name="Trouble Tabbls Cat Cam"
model=""
width=640
height=480
fps=30
rotate=""
do_build=false
libs_only=false

usage() {
  cat <<'EOF'
usage: run-producer-pi.sh [--build] [--libs-only] [--port N] [--model FILE]
                          [--width N] [--height N] [--fps N] [--name NAME]
                          [--rotate DEG]

  --build      build the producer in the container first (long cargo build)
  --libs-only  refresh build/cat-stream/hostlibs and exit
  --rotate     rotate the stream 90/180/270 (a camera mounted upside down)

Runs the cat producer from the family CI container against the Pi's CSI
camera; OmniAccelerANT's scripts/linux/cat-stream/serve.sh is the web/HTTPS
half. Stop the producer with `nerdctl rm -f cat-producer`.
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --build) do_build=true; shift ;;
    --libs-only) libs_only=true; shift ;;
    --port) port="${2:?--port needs a value}"; shift 2 ;;
    --model) model="${2:?--model needs a value}"; shift 2 ;;
    --width) width="${2:?--width needs a value}"; shift 2 ;;
    --height) height="${2:?--height needs a value}"; shift 2 ;;
    --fps) fps="${2:?--fps needs a value}"; shift 2 ;;
    --rotate) rotate="${2:?--rotate needs a value}"; shift 2 ;;
    --name) name="${2:?--name needs a value}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
  esac
done

command -v nerdctl >/dev/null 2>&1 || {
  printf 'nerdctl not found - is containerd running?\n' >&2
  exit 1
}

# Collected once; delete build/cat-stream/hostlibs to refresh after a system upgrade.
collect_hostlibs() {
  local libcamera_real libcamera_base
  libcamera_real="$(readlink -f /usr/lib/aarch64-linux-gnu/libcamera.so.0.7 2>/dev/null || true)"
  if [ -z "${libcamera_real}" ] || [ ! -e "${libcamera_real}" ]; then
    printf 'host libcamera not found - this script expects Raspberry Pi OS with libcamera installed\n' >&2
    exit 1
  fi
  libcamera_base="$(dirname "${libcamera_real}")"
  local pisp
  pisp="$(readlink -f /lib/aarch64-linux-gnu/libpisp.so.1 2>/dev/null || true)"
  if [ -z "${pisp}" ] || [ ! -e "${pisp}" ]; then
    pisp="$(readlink -f /usr/lib/aarch64-linux-gnu/libpisp.so.1 2>/dev/null || true)"
  fi
  if [ -z "${pisp}" ] || [ ! -e "${pisp}" ]; then
    printf 'host libpisp not found\n' >&2
    exit 1
  fi
  local ipa="${libcamera_base}/libcamera/ipa/ipa_rpi_pisp.so"
  [ -e "${ipa}" ] || { printf 'host rpi/pisp IPA not found at %s\n' "${ipa}" >&2; exit 1; }

  rm -rf "${hostlibs_dir}"
  mkdir -p "${hostlibs_dir}"
  printf 'collecting host libcamera libraries into %s\n' "${hostlibs_dir}"
  # The IPA is copied only for its dependencies; libcamera loads it from its own mounted directory.
  local queue=(
    "$(readlink -f "${libcamera_base}/libcamera.so.0.7")"
    "$(readlink -f "${libcamera_base}/libcamera-base.so.0.7")"
    "${pisp}"
    "${ipa}"
  )
  declare -A seen=()
  while ((${#queue[@]})); do
    local f="${queue[0]}"; queue=("${queue[@]:1}")
    if [ -z "$f" ] || [ ! -e "$f" ]; then
      continue
    fi
    local base; base="$(basename "$f")"
    case "$base" in
      libc.so.6|libm.so.6|libpthread.so.0|libdl.so.2|librt.so.1|libgcc_s.so.1|libstdc++.so.6|ld-linux*) continue ;;
    esac
    [ -n "${seen[$base]:-}" ] && continue
    seen[$base]=1
    cp -L "$f" "${hostlibs_dir}/${base}"
    while IFS= read -r dep; do
      [ -n "$dep" ] && [ -e "$dep" ] && queue+=("$dep")
    done < <(ldd "$f" 2>/dev/null | awk '/=> \// {print $3} /^\t\/[^ ]+ \(/ {print $1}')
  done
  # NEEDED names sonames, but files are named after their real paths, so link each soname.
  local lib soname
  for lib in "${hostlibs_dir}"/*; do
    soname="$(readelf -d "$lib" 2>/dev/null | awk '/SONAME/ {gsub(/[][]/,""); print $NF}')"
    if [ -n "${soname}" ] && [ ! -e "${hostlibs_dir}/${soname}" ]; then
      ln -s "$(basename "$lib")" "${hostlibs_dir}/${soname}"
    fi
  done
  printf 'collected %d libraries (%s)\n' "${#seen[@]}" "$(du -sh "${hostlibs_dir}" | cut -f1)"
}

if [ "${libs_only}" = true ]; then
  collect_hostlibs
  exit 0
fi
[ -d "${hostlibs_dir}" ] || collect_hostlibs

# Rootless: the `video` group does not reach the user namespace, so grant the invoking user ACLs.
for dev in /dev/media* /dev/video* /dev/dma_heap/*; do
  [ -e "$dev" ] || continue
  [ -w "$dev" ] && continue
  printf 'granting %s ACL access to %s\n' "$dev" "$(id -un)"
  sudo setfacl -m "u:$(id -un):rw" "$dev"
done

# Runs in the container (quoted heredoc): re-appends image media paths after /hostlibs, refuses pre-CON23 images.
host_multiarch_dir="/usr/lib/aarch64-linux-gnu"
container_prologue="$(cat <<'PROLOGUE'
set -euo pipefail
for _hub_env in /opt/scripts/03-media/final/media-env.sh /opt/scripts/core/path-helpers.sh; do
  if [ ! -r "${_hub_env}" ]; then
    printf 'ANTfrastructure helper missing from the image: %s\n' "${_hub_env}" >&2
    printf 'This runner needs the family CI image, not a bare distro image.\n' >&2
    exit 1
  fi
done
# shellcheck source=/dev/null
. /opt/scripts/core/path-helpers.sh
_image_ld="$(LD_LIBRARY_PATH='' bash -c '. /opt/scripts/03-media/final/media-env.sh; printf "%s" "${LD_LIBRARY_PATH}"')"
IFS=: read -r -a _image_dirs <<<"${_image_ld}"
for _dir in "${_image_dirs[@]}" "${HOST_MULTIARCH_DIR}"; do
  [ -n "${_dir}" ] || continue
  if ! _path_contains "${LD_LIBRARY_PATH:-}" "${_dir}"; then
    LD_LIBRARY_PATH="${LD_LIBRARY_PATH:+${LD_LIBRARY_PATH}:}${_dir}"
  fi
done
export LD_LIBRARY_PATH
IFS=: read -r -a _ld_dirs <<<"${LD_LIBRARY_PATH}"
for _dir in "${_ld_dirs[@]}"; do
  [ "${_dir}" = /hostlibs ] && break
  case "${_dir}" in
    "${LIBCAMERA_PREFIX:-/opt/libcamera}"|"${LIBCAMERA_PREFIX:-/opt/libcamera}"/*)
      printf 'the image puts %s ahead of /hostlibs - it predates ANTfrastructure CON23; pull :latest\n' "${_dir}" >&2
      exit 1
      ;;
  esac
done
: "${ORT_DYLIB_PATH:=${ORT_LIB_LOCATION:-/usr/local/lib/onnxruntime-cpu/lib}/libonnxruntime.so}"
export ORT_DYLIB_PATH
exec "$@"
PROLOGUE
)"

nerdctl_args=(
  run --rm
  --name "${container_name}"
  --user 0:0
  --network host
  # The RPi IPA proxy forks a worker, which the default seccomp profile answers with ENOSYS.
  --security-opt seccomp=unconfined
  -v /sys:/sys:ro
  -v /dev:/dev
  -v /run/udev:/run/udev:ro
  -v "${hostlibs_dir}":/hostlibs:ro
  -v /usr/lib/aarch64-linux-gnu/libcamera:/usr/lib/aarch64-linux-gnu/libcamera:ro
  -v /usr/share/libcamera:/usr/share/libcamera:ro
  -v /usr/share/libpisp:/usr/share/libpisp:ro
  -v "${repo_root}":/workspace
  -v "${target_volume}":/cargo-target
  -e LD_LIBRARY_PATH=/hostlibs
  -e "HOST_MULTIARCH_DIR=${host_multiarch_dir}"
  -e RUST_LOG="${RUST_LOG:-info}"
  "${image}"
  bash -c "${container_prologue}" cat-producer "${producer}"
  --libcamera
  --listen-port "${port}"
  # serve.sh is this setup's web half, here or on another host (--producer-host): keep signalling reachable.
  --signalling-host 0.0.0.0
  --http-port 0
  --name "${name}"
  --width "${width}"
  --height "${height}"
  --fps "${fps}"
)

if [ -n "${model}" ]; then
  nerdctl_args+=(--model "${model}")
fi

if [ -n "${rotate}" ]; then
  nerdctl_args+=(--rotate "${rotate}")
fi

producer_present() {
  nerdctl run --rm --user 0:0 -v "${target_volume}":/cargo-target "${image}" \
    bash -c "test -x ${producer}" >/dev/null 2>&1
}

if [ "${do_build}" = true ] || ! producer_present; then
  [ "${do_build}" = true ] || {
    printf 'producer binary missing in volume %s - rerun with --build\n' "${target_volume}" >&2
    exit 1
  }
  printf 'building kataglyphis_cat_webrtc in the container (this takes a while)\n'
  nerdctl run --rm --user 0:0 --network host \
    -v "${repo_root}":/workspace \
    -v "${target_volume}":/cargo-target \
    -v "${cargo_volume}":/cargo-home \
    -e CARGO_TARGET_DIR=/cargo-target -e CARGO_HOME=/cargo-home \
    "${image}" \
    bash -lc 'cd /workspace && cargo build --release --locked -p kataglyphis_cat_webrtc'
fi

# A producer from an interrupted run still holds the name.
nerdctl rm -f "${container_name}" >/dev/null 2>&1 || true

printf 'starting the producer: libcamerasrc -> YOLO -> webrtcsink on port %s\n' "${port}"
printf 'stop it with: nerdctl rm -f %s\n' "${container_name}"
exec nerdctl "${nerdctl_args[@]}"
