#!/usr/bin/env bash
# Exercise every package route and reject GPU profiles that only used CPU or copies.
set -euo pipefail
# shellcheck source=scripts/linux/tests/lib-fake-gpu.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-fake-gpu.sh"
# shellcheck source=/dev/null
source "${fake_gpu_tests_dir}/../lib/ort-runtime-bundle.sh"
repo="${fake_gpu_repo}"
source_hub="$(fake_gpu_hub_dir)"
fake_gpu_workspace
mkdir -p "${work}/packages" "${work}/runner"
export RUNNER_TEMP="${work}/runner" FAKE_SEEN="${work}/seen"
fake_gpu_poison_env
export KATAGLYPHIS_ORT_DEVICE=cpu
for device in cuda migraphx; do
    for format in tar deb appimage; do
        bundle="${work}/${format} bundle"
        fake_gpu_bundle_tree "${bundle}"
        printf '%s' "${format}" > "${bundle}/format"
        fake_gpu_write_launcher "${bundle}/oxidant" <<'LAUNCHER'
fake_gpu_assert_unset LD_PRELOAD LD_AUDIT KATAGLYPHIS_ONNX_MODEL KATAGLYPHIS_ONNX_BACKEND ORT_DYLIB_PATH ORT_LIB_LOCATION
if [[ ${1:-} == onnx-detect ]]; then
    fake_gpu_assert_unset KATAGLYPHIS_PREPROCESS KATAGLYPHIS_SWAP_XY KATAGLYPHIS_ORT_THREADS KATAGLYPHIS_ORT_SPIN
    fake_gpu_detect_args "$@"
    [[ -f $3 ]]
    format="$(cat "${here}/format")"
    provider="$(fake_gpu_provider "${KATAGLYPHIS_ORT_DEVICE}")"
    if [[ ${KATAGLYPHIS_ORT_DEVICE} == cpu ]]; then
        test "${LD_LIBRARY_PATH}" = "${here}/lib"
    else
        test "${LD_LIBRARY_PATH}" = "${here}/lib:/host/gpu-sdk"
    fi
    fake_gpu_two_run_profile "$7.json" "${provider}" "${provider}"
    score="$(fake_gpu_drift_score "${FAKE_PARITY_FORMAT:-}" "$format" 0.9)"
    fake_gpu_detections "$5" "$score"
    cp "$5" "$9"
    exit 0
fi
test "${LD_LIBRARY_PATH}" = "${here}/lib:/host/gpu-sdk"
fake_gpu_check_args "$@"
format="$(cat "${here}/format")"
printf '%s\n' "${format}" >> "${FAKE_SEEN}"
[[ ${KATAGLYPHIS_ORT_DEVICE} != cpu ]]
provider="$(fake_gpu_provider "${KATAGLYPHIS_ORT_DEVICE}")"
if [ "${FAKE_CPU_FORMAT:-}" = "${format}" ]; then provider=CPUExecutionProvider; fi
operator=Conv
if [ "${FAKE_COPY_FORMAT:-}" = "${format}" ]; then operator=MemcpyFromHost; fi
: '--library-path "$here/lib/glibc:$here/lib"'
fake_gpu_node_profile "$3.json" "${provider}" "${operator}"
LAUNCHER
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
    bash "${fake_gpu_tests_dir}/../profile-gpu-packages.sh" "${work}/packages" "${device}" >/dev/null
    printf 'tar\ndeb\nappimage\n' > "${work}/expected"
    cmp "${work}/expected" "${FAKE_SEEN}"
    printf fixture > "${work}/two cats.jpg"
    bash "${fake_gpu_tests_dir}/../profile-gpu-packages.sh" "${work}/packages" "${device}" "${work}/two cats.jpg" >/dev/null
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
    fake_gpu_refuses 'Unpinned comparator override was accepted' "${work}/poison.log" \
        env GITHUB_WORKSPACE="${trusted}" GPU_PACKAGE_DIR="${work}/packages" ANTFRASTRUCTURE_DIR="${poison}" \
        bash -c "${unsafe_command}"
    grep -q 'poisoned comparator reached' "${work}/poison.log"
    for format in tar deb appimage; do
        fake_gpu_refuses "${format} detector mismatch was accepted for ${device}" /dev/null env FAKE_PARITY_FORMAT="${format}" bash "${fake_gpu_tests_dir}/../profile-gpu-packages.sh" "${work}/packages" "${device}" "${work}/two cats.jpg"
        fake_gpu_refuses "${format} CPU profile was accepted for ${device}" /dev/null env FAKE_CPU_FORMAT="${format}" bash "${fake_gpu_tests_dir}/../profile-gpu-packages.sh" "${work}/packages" "${device}"
        fake_gpu_refuses "${format} GPU transfer was accepted as model execution" /dev/null env FAKE_COPY_FORMAT="${format}" bash "${fake_gpu_tests_dir}/../profile-gpu-packages.sh" "${work}/packages" "${device}"
    done
    mv "${work}/packages/test.deb" "${work}/missing.deb"
    fake_gpu_refuses 'missing DEB was accepted' /dev/null bash "${fake_gpu_tests_dir}/../profile-gpu-packages.sh" "${work}/packages" "${device}"
    mv "${work}/missing.deb" "${work}/packages/test.deb"
    cp "${work}/packages/test.AppImage" "${work}/packages/extra.AppImage"
    fake_gpu_refuses 'multiple AppImages were accepted' /dev/null bash "${fake_gpu_tests_dir}/../profile-gpu-packages.sh" "${work}/packages" "${device}"
    rm -rf -- "${work}/tar bundle" "${work}/deb bundle" "${work}/appimage bundle" \
        "${work}/tar-root" "${work}/deb-root" "${work}/packages"/*
done
echo 'GPU package profiling fixtures passed'
