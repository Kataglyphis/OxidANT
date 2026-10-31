#!/usr/bin/env bash
# Exercise every package route and reject GPU profiles that only used CPU or copies.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
source_hub="${ANTFRASTRUCTURE_DIR:-${repo}/third_party/ANTfrastructure}"
# shellcheck source=/dev/null
source "${here}/../lib/ort-runtime-bundle.sh"
work="$(mktemp -d)"
trap 'rm -rf -- "${work}"' EXIT
mkdir -p "${work}/packages" "${work}/runner"
export RUNNER_TEMP="${work}/runner" FAKE_SEEN="${work}/seen"
export LD_PRELOAD='' LD_AUDIT=''
export KATAGLYPHIS_ONNX_MODEL=/wrong KATAGLYPHIS_ONNX_BACKEND=tract KATAGLYPHIS_ORT_DEVICE=cpu
export ORT_DYLIB_PATH=/wrong ORT_LIB_LOCATION=/wrong LD_LIBRARY_PATH=/host/gpu-sdk
for device in cuda migraphx; do
    for format in tar deb appimage; do
        bundle="${work}/${format} bundle"
        mkdir -p "${bundle}/resources/models" "${bundle}/lib"
        printf model > "${bundle}/resources/models/deim_dfine_n_coco.onnx"
        printf runtime > "${bundle}/lib/libonnxruntime.so"
        printf '%s' "${format}" > "${bundle}/format"
        cat > "${bundle}/oxidant" <<'LAUNCHER'
#!/bin/bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
test -z "${LD_PRELOAD+x}${LD_AUDIT+x}${KATAGLYPHIS_ONNX_MODEL+x}${KATAGLYPHIS_ONNX_BACKEND+x}${ORT_DYLIB_PATH+x}${ORT_LIB_LOCATION+x}"
if [[ ${1:-} == onnx-detect ]]; then
    [[ -z ${KATAGLYPHIS_PREPROCESS+x}${KATAGLYPHIS_SWAP_XY+x}${KATAGLYPHIS_ORT_THREADS+x}${KATAGLYPHIS_ORT_SPIN+x} ]]
    [[ $2 == --image && -f $3 && $4 == --output && $6 == --profile ]]
    format="$(cat "${here}/format")"
    case "${KATAGLYPHIS_ORT_DEVICE}" in
        cpu) [[ $# -eq 9 && $8 == --repeat-output ]]; test "${LD_LIBRARY_PATH}" = "${here}/lib"; provider=CPUExecutionProvider ;;
        cuda) [[ $# -eq 9 && $8 == --repeat-output ]]; test "${LD_LIBRARY_PATH}" = "${here}/lib:/host/gpu-sdk"; provider=CUDAExecutionProvider ;;
        migraphx) [[ $# -eq 9 && $8 == --repeat-output ]]; test "${LD_LIBRARY_PATH}" = "${here}/lib:/host/gpu-sdk"; provider=MIGraphXExecutionProvider ;;
        *) exit 1 ;;
    esac
    printf '[{"cat":"Session","name":"model_run","ph":"X","pid":1,"ts":0,"dur":90},{"cat":"Node","ph":"X","pid":1,"ts":10,"dur":20,"args":{"provider":"%s","op_name":"Conv"}}' "${provider}" > "$7.json"
    printf ',{"cat":"Session","name":"model_run","ph":"X","pid":1,"ts":100,"dur":90},{"cat":"Node","ph":"X","pid":1,"ts":110,"dur":20,"args":{"provider":"%s","op_name":"Conv"}}' "${provider}" >> "$7.json"
    printf ']\n' >> "$7.json"
    score=0.9
    if [[ ${FAKE_PARITY_FORMAT:-} == "$format" && $KATAGLYPHIS_ORT_DEVICE != cpu ]]; then score=0.6; fi
    printf '{"schema_version":1,"width":100,"height":100,"detections":[{"class":15,"score":%s,"box":[10,60,30,95]},{"class":15,"score":0.7,"box":[50,20,80,60]}]}' "$score" > "$5"
    cp "$5" "$9"
    exit 0
fi
test "${LD_LIBRARY_PATH}" = "${here}/lib:/host/gpu-sdk"
test "$#" -eq 3 && test "$1" = onnx-check && test "$2" = --profile
format="$(cat "${here}/format")"
printf '%s\n' "${format}" >> "${FAKE_SEEN}"
case "${KATAGLYPHIS_ORT_DEVICE}" in
    cuda) provider=CUDAExecutionProvider ;;
    migraphx) provider=MIGraphXExecutionProvider ;;
    *) exit 1 ;;
esac
if [ "${FAKE_CPU_FORMAT:-}" = "${format}" ]; then provider=CPUExecutionProvider; fi
operator=Conv
if [ "${FAKE_COPY_FORMAT:-}" = "${format}" ]; then operator=MemcpyFromHost; fi
: '--library-path "$here/lib/glibc:$here/lib"'
printf '[{"cat":"Node","args":{"provider":"%s","op_name":"%s"}}]\n' "${provider}" "${operator}" > "$3.json"
LAUNCHER
        chmod +x "${bundle}/oxidant"
        configure_gpu_launcher "${bundle}/oxidant" "${device}"
    done
    mkdir -p "${work}/tar-root" "${work}/deb-root/DEBIAN" "${work}/deb-root/opt"
    cp -a "${work}/tar bundle" "${work}/tar-root/oxidant"
    cp -a "${work}/deb bundle" "${work}/deb-root/opt/oxidant"
    cat > "${work}/deb-root/DEBIAN/control" <<'CONTROL'
Package: oxidant-fixture
Version: 1
Architecture: all
Maintainer: Test <test@example.invalid>
Description: GPU profile routing fixture
CONTROL
    tar -czf "${work}/packages/test.tar.gz" -C "${work}/tar-root" oxidant
    dpkg-deb --root-owner-group --build "${work}/deb-root" "${work}/packages/test.deb" >/dev/null
    export FAKE_APPIMAGE_BUNDLE="${work}/appimage bundle"
    cat > "${work}/packages/test.AppImage" <<'APPIMAGE'
#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = --appimage-extract ]; then
    test -z "${LD_PRELOAD+x}${LD_AUDIT+x}${APPIMAGE_EXTRACT_AND_RUN+x}"
    mkdir -p squashfs-root/usr/lib/oxidant
    cp -a "${FAKE_APPIMAGE_BUNDLE}/." squashfs-root/usr/lib/oxidant/
    exit 0
fi
test "${APPIMAGE_EXTRACT_AND_RUN:-}" = 1
cp -a "${FAKE_APPIMAGE_BUNDLE}" "${TMPDIR}/oxidant"
exec "${TMPDIR}/oxidant/oxidant" "$@"
APPIMAGE
    # Actions downloads the AppImage without the executable bit.
    chmod 0644 "${work}/packages/test.AppImage"
    : > "${FAKE_SEEN}"
    bash "${here}/../profile-gpu-packages.sh" "${work}/packages" "${device}" >/dev/null
    printf 'tar\ndeb\nappimage\n' > "${work}/expected"
    cmp "${work}/expected" "${FAKE_SEEN}"
    printf fixture > "${work}/two cats.jpg"
    bash "${here}/../profile-gpu-packages.sh" "${work}/packages" "${device}" "${work}/two cats.jpg" >/dev/null
    trusted="${work}/trusted workspace"
    poison="${work}/poison hub"
    mkdir -p "${trusted}/third_party/ANTfrastructure/shared/python" \
        "${trusted}/third_party/ANTfrastructure/linux/scripts/lib" \
        "${trusted}/crates/inference/tests/fixtures" "${poison}/shared/python" "${poison}/linux/scripts/lib"
    cp "${source_hub}/linux/scripts/lib/"{gpu-profile-env,log-bootstrap}.sh "${trusted}/third_party/ANTfrastructure/linux/scripts/lib/"
    cp "${source_hub}/linux/scripts/lib/"{gpu-profile-env,log-bootstrap}.sh "${poison}/linux/scripts/lib/"
    cp "${source_hub}/shared/python/compare_detector_parity.py" "${trusted}/third_party/ANTfrastructure/shared/python/"
    cp "${source_hub}/shared/python/verify_ort_profile.py" "${trusted}/third_party/ANTfrastructure/shared/python/"
    cp "${source_hub}/shared/python/verify_ort_profile.py" "${poison}/shared/python/"
    cp "${work}/two cats.jpg" "${trusted}/crates/inference/tests/fixtures/two_cats.jpg"
    printf 'raise SystemExit("poisoned comparator reached")\n' > "${poison}/shared/python/compare_detector_parity.py"
    case "${device}" in
        cuda) workflow="${repo}/.github/workflows/linux-nvidia-gpu.yml" ;;
        migraphx) workflow="${repo}/.github/workflows/linux-x64-amd-gpu.yml" ;;
    esac
    command="$(sed -n '/^          command: env ANTFRASTRUCTURE_DIR=/s/^          command: //p' "${workflow}")"
    [ -n "${command}" ] || { echo 'Pinned promotion command missing' >&2; exit 1; }
    env GITHUB_WORKSPACE="${trusted}" GPU_PACKAGE_DIR="${work}/packages" ANTFRASTRUCTURE_DIR="${poison}" \
        bash -c "${command}" >/dev/null
    # shellcheck disable=SC2016
    unsafe_command="$(printf '%s\n' "${command}" | sed 's|ANTFRASTRUCTURE_DIR="$GITHUB_WORKSPACE/third_party/ANTfrastructure" ||')"
    [ "${unsafe_command}" != "${command}" ] || { echo 'Pinned hub override mutant was not created' >&2; exit 1; }
    if env GITHUB_WORKSPACE="${trusted}" GPU_PACKAGE_DIR="${work}/packages" ANTFRASTRUCTURE_DIR="${poison}" \
        bash -c "${unsafe_command}" >"${work}/poison.log" 2>&1; then
        echo 'Unpinned comparator override was accepted' >&2; exit 1
    fi
    grep -q 'poisoned comparator reached' "${work}/poison.log"
    for format in tar deb appimage; do
        if FAKE_PARITY_FORMAT="${format}" bash "${here}/../profile-gpu-packages.sh" "${work}/packages" "${device}" "${work}/two cats.jpg" >/dev/null 2>&1; then
            echo "${format} detector mismatch was accepted for ${device}" >&2; exit 1
        fi
        if FAKE_CPU_FORMAT="${format}" bash "${here}/../profile-gpu-packages.sh" "${work}/packages" "${device}" >/dev/null 2>&1; then
            echo "${format} CPU profile was accepted for ${device}" >&2; exit 1
        fi
        if FAKE_COPY_FORMAT="${format}" bash "${here}/../profile-gpu-packages.sh" "${work}/packages" "${device}" >/dev/null 2>&1; then
            echo "${format} GPU transfer was accepted as model execution" >&2; exit 1
        fi
    done
    mv "${work}/packages/test.deb" "${work}/missing.deb"
    if bash "${here}/../profile-gpu-packages.sh" "${work}/packages" "${device}" >/dev/null 2>&1; then
        echo 'missing DEB was accepted' >&2; exit 1
    fi
    mv "${work}/missing.deb" "${work}/packages/test.deb"
    cp "${work}/packages/test.AppImage" "${work}/packages/extra.AppImage"
    if bash "${here}/../profile-gpu-packages.sh" "${work}/packages" "${device}" >/dev/null 2>&1; then
        echo 'multiple AppImages were accepted' >&2; exit 1
    fi
    rm -rf -- "${work}/tar bundle" "${work}/deb bundle" "${work}/appimage bundle" \
        "${work}/tar-root" "${work}/deb-root" "${work}/packages"/*
done
echo 'GPU package profiling fixtures passed'
