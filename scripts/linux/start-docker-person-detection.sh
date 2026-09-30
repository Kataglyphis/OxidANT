#!/bin/bash
# IMAGE overrides the container image, which defaults to the family Linux CI image.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"

# The tag comes from the hub's ci-image-ref.sh, never a literal; two assignments so `set -e` stops.
if [ -z "${IMAGE:-}" ]; then
    _ci_image_ref_sh="$(antfrastructure_path linux/scripts/ci-image-ref.sh)"
    IMAGE="$(bash "${_ci_image_ref_sh}")"
fi

# Erlaube Docker den Zugriff auf das lokale X11-Display (für GUI)
xhost +local:root || true

echo "Starte Docker Container mit Webcam-Unterstützung..."

# Mount /dev/video0 für die Webcam (Logitech C922)
docker run --rm -it \
    --ipc=host \
    --device /dev/video0:/dev/video0 \
    -e DISPLAY="${DISPLAY:?DISPLAY is not set - this script needs an X display}" \
    -e QT_X11_NO_MITSHM=1 \
    -e GTK_A11Y=none \
    -e LIBGL_ALWAYS_SOFTWARE=1 \
    -e GALLIUM_DRIVER=llvmpipe \
    -e GSK_RENDERER=cairo \
    -v /tmp/.X11-unix:/tmp/.X11-unix \
    -v "$(pwd):/workspace" \
    -w /workspace \
    "${IMAGE}" \
    bash -lc '
    set -euo pipefail
    git config --global --add safe.directory /workspace || true
    # Fix für Bibliotheken aus /opt, da diese priorisiert geladen werden müssen
    export GDK_BACKEND=x11
    
    # Füge alle Library-Pfade aus /opt hinzu (z.B. OpenCV, FFmpeg, GStreamer)
    for libdir in $(find /opt ! -name "android*" -type d \( -name "lib" -o -name "lib64" -o -name "x86_64-linux-gnu" \)); do
        if [ -d "$libdir" ]; then
            export LD_LIBRARY_PATH="$libdir:${LD_LIBRARY_PATH:-}"
        fi
    done
    export LD_LIBRARY_PATH="/opt/gstreamer/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}"

    # Only what the image genuinely does NOT ship. This list used to name 24
    # packages; 22 of them are already installed by ANTfrastructure'"'"'s
    # linux/scripts/03-media/runtime/install-deps.sh, so it was a stale copy of
    # that list which would drift every time the image changed.
    #
    # libgtk-4-dev is the real gap, and a deliberate one: ANTfrastructure excludes
    # it because the foreign-arch GTK dev chain pulls target-side Python and
    # breaks cross builds on python3-minimal'"'"'s postinst. Installing it here,
    # at runtime, in a throwaway container, is the right place for it - not in
    # the image. Keep this list minimal for the same reason; if something else
    # turns out to be missing, check install-deps.sh first, it probably is not.
    apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y \
        libgtk-4-dev libavfilter9 || true

    bash scripts/linux/run-person-detection.sh
    '
