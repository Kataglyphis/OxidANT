#!/bin/bash
#
# Env vars:
#   IMAGE   Container image to run in. Defaults to the family Linux CI image.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"

# The tag is NOT spelled here, and until now it was: this script carried the
# full reference inline with no override of any kind, so a fleet-wide tag bump
# left it behind silently and a local experiment meant editing the file. It is
# not repeated in this comment either - a literal in a comment rots exactly the
# same way, and verify_ci_image_refs.py check D now reads *.sh and *.ps1 as well
# as <root>/.github/**.yml, comments included. ANTfrastructure's
# linux/scripts/ci-image-ref.sh composes
# ${IMAGE_REGISTRY_PREFIX}:${CI_IMAGE_LINUX_TAG} from the hub's
# linux/scripts/01-core/versions.env, the fleet's one owner of both CI refs.
# Linux, not --windows: this is a `docker run` against /dev/video0 and an X11
# socket.
#
# Its stdout carries the reference and nothing else (every diagnostic goes to
# stderr), so it is safe inside a command substitution, and a missing key exits
# non-zero rather than yielding an empty string - under `set -e` that aborts
# here instead of reaching `docker run` as "run the next argument as an image".
#
# antfrastructure_path is resolved on its own line rather than nested inside that
# substitution. Nested, a missing submodule printed the helper's three-line
# diagnostic and then ran `bash ""`, adding a bare "bash: : No such file or
# directory" of its own before exiting 127. Assigned first, `set -e` stops on
# the real message - the same shape antfrastructure_source and antfrastructure_exec
# already use.
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
