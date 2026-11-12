#!/usr/bin/env bash
# Mutation tests for the candidate package verification wiring.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/antfrastructure.sh
source "${here}/../lib/antfrastructure.sh"
shared_verify="$(antfrastructure_path shared/python/verify_locked_files.py)"
shared_fetcher="$(antfrastructure_path linux/scripts/lib/fetch-data-lock.py)"
shared_package_proof="$(antfrastructure_path linux/scripts/06-packaging/ort-package-proof.sh)"
shared_runtime_bundle="$(antfrastructure_path linux/scripts/06-packaging/ort-runtime-bundle.sh)"
real_python="$(command -v python3)"
unset KATAGLYPHIS_REPO_ROOT
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
hub="${work}/hub/linux/scripts/06-packaging"
mkdir -p "$hub" "${work}/hub/linux/scripts/01-core" "${work}/hub/linux/scripts/lib" "${work}/hub/shared/python" "${work}/bin" "${work}/packages" "${work}/tree/oxidant/lib" "${work}/tree/oxidant/licenses/onnxruntime" "${work}/tree/oxidant/resources/models" "${work}/source/scripts/linux/lib" "${work}/source/resources/models" "${work}/appimage-models"
cp "$shared_verify" "${work}/hub/shared/python/verify_locked_files.py"
cp "$shared_fetcher" "${work}/hub/linux/scripts/lib/fetch-data-lock.py"
cp "${here}/../verify-gpu-packages.sh" "${work}/source/scripts/linux/"
cp "${here}/../lib/antfrastructure.sh" "${here}/../lib/renovate-git-mode.sh" "${here}/../lib/models-lock.sh" "${here}/../lib/gpu-check-common.sh" \
    "${here}/../lib/ort-runtime-bundle.sh" "${work}/source/scripts/linux/lib/"
# The image's plugin tree: the module, its licence texts and its provenance.
notices=licenses/onnxruntime-ep-amdgpu
mkdir -p "${work}/ep/lib" "${work}/ep/licenses/onnxruntime-ep-amdgpu" "${work}/ep/licenses/fmt" "${work}/appimage-notices"
printf 'plugin\n' > "${work}/ep/lib/libmigraphx-ep.so"
printf 'plugin license\n' > "${work}/ep/licenses/onnxruntime-ep-amdgpu/LICENSE"
printf 'seed license\n' > "${work}/ep/licenses/fmt/LICENSE.rst"
printf 'onnxruntime-ep-amdgpu fixture\n' > "${work}/ep/provenance.txt"
export ORT_AMDGPU_EP_ROOT="${work}/ep"
stage_notices() { rm -rf "$1"; mkdir -p "$1"; cp -R "${work}/ep/licenses/." "$1/"; cp "${work}/ep/provenance.txt" "$1/"; }
stage_notices "${work}/appimage-notices"
for name in deim_dfine_n_coco.onnx deim_dfine_n_coco.LICENSE deim_dfine_n_coco.NOTICE; do
    printf 'fixture %s\n' "$name" > "${work}/tree/oxidant/resources/models/$name"
    printf '%s  %s  https://example.invalid/%s\n' \
        "$(sha256sum "${work}/tree/oxidant/resources/models/$name" | cut -d ' ' -f1)" "$name" "$name" \
        >> "${work}/source/resources/models/models.lock"
done
cp -a "${work}/tree/oxidant/resources/models/." "${work}/appimage-models/"
export REAL_PYTHON="$real_python" FIXTURE_APPIMAGE_MODELS="${work}/appimage-models" FIXTURE_APPIMAGE_NOTICES="${work}/appimage-notices"
cat > "${work}/hub/linux/scripts/01-core/platform.sh" <<'STUB'
arch_normalize() { case "$1" in x64) echo amd64 ;; arm64) echo arm64 ;; *) return 1 ;; esac; }
STUB
cp "$shared_runtime_bundle" "${hub}/ort-runtime-bundle.sh"
cat > "${hub}/verify-ort-appimage-payload.sh" <<'STUB'
#!/usr/bin/env bash
set -e
grep -qx '# verified' "$1"
case "$5" in nvidia|amd) ;; *) exit 1 ;; esac
STUB
cp "$shared_package_proof" "${hub}/ort-package-proof.sh"
touch "${hub}/stage-ort-wheel-licenses.py"
cat > "${work}/bin/python3" <<'STUB'
#!/usr/bin/env bash
if [ "$1" = -I ]; then exec "$REAL_PYTHON" "$@"; fi
shift
destination=
strip_seen=false
exact=false
while [ "$#" -gt 0 ]; do
    case "$1" in
        --destination) destination="$2"; shift 2 ;;
        --library) [ "$(cat "$2")" = "${2##*/}" ] || exit 1; shift 2 ;;
        --wheel-dir) shift 2 ;;
        --strip) [ -n "$2" ] || exit 1; strip_seen=true; shift 2 ;;
        --require-unstripped) exact=true; shift ;;
        *) exit 1 ;;
    esac
done
[ "$strip_seen" = true ] || exit 1
# Exact bytes hold for the NVIDIA prefix only: the image strips the CPU core an AMD package carries.
[ "$exact" = "$FIXTURE_EXACT_WHEEL" ] || exit 1
mkdir -p "$destination/licenses/onnxruntime"
printf 'license\n' > "$destination/licenses/onnxruntime/LICENSE"
printf 'notices\n' > "$destination/licenses/onnxruntime/ThirdPartyNotices.txt"
STUB
chmod +x "${work}/bin/python3"
export PATH="${work}/bin:${PATH}" ANTFRASTRUCTURE_DIR="${work}/hub"
for name in libonnxruntime.so libonnxruntime.so.1; do
    printf '%s\n' "$name" > "${work}/tree/oxidant/lib/${name}"
done
printf 'license\n' > "${work}/tree/oxidant/licenses/onnxruntime/LICENSE"
printf 'notices\n' > "${work}/tree/oxidant/licenses/onnxruntime/ThirdPartyNotices.txt"
cat > "${work}/packages/test.AppImage" <<'APPIMAGE'
#!/usr/bin/env bash
# verified
set -euo pipefail
[[ "$#" = 2 && "$1" = --appimage-extract ]]
case "$2" in
    'usr/lib/oxidant/resources/models/*') payload="$FIXTURE_APPIMAGE_MODELS" ;;
    'usr/lib/oxidant/licenses/onnxruntime-ep-amdgpu/*') payload="$FIXTURE_APPIMAGE_NOTICES" ;;
    *) exit 1 ;;
esac
mkdir -p "squashfs-root/${2%/\*}"
cp -a "$payload/." "squashfs-root/${2%/\*}/"
APPIMAGE
chmod +x "${work}/packages/test.AppImage"
pack_deb() {
    local root
    root="$(mktemp -d "${work}/deb.XXXXXX")"
    mkdir -p "$root/DEBIAN" "$root/opt/oxidant"
    cp -a "${work}/tree/oxidant/." "$root/opt/oxidant/"
    printf 'Package: %s\nVersion: %s\nArchitecture: %s\nMaintainer: Test <test@example.invalid>\nDescription: GPU package fixture\n' \
        "${deb_package:-oxidant}" "${deb_version:-1.0+gpu-$vendor}" "${deb_arch:-amd64}" > "$root/DEBIAN/control"
    dpkg-deb --root-owner-group --build "$root" "${work}/packages/test.deb" >/dev/null
}
pack() { tar -czf "${work}/packages/test.tar.gz" -C "${work}/tree" oxidant; pack_deb; }
check() { bash "${work}/source/scripts/linux/verify-gpu-packages.sh" "${work}/packages" "$1" "$2" >/dev/null 2>&1; }
shared_provider="${work}/tree/oxidant/lib/libonnxruntime_providers_shared.so"
# The wheel stub accepts a library whose text is its name; the plugin is compared with the image's module instead.
write_provider() {
    if [ "$vendor" = amd ]; then cp "${work}/ep/lib/${provider}" "${work}/tree/oxidant/lib/${provider}"; else printf '%s\n' "$provider" > "${work}/tree/oxidant/lib/${provider}"; fi
}
for vendor in nvidia amd; do
    case "$vendor" in
        nvidia) host=libcuda.so.1; provider=libonnxruntime_providers_cuda.so; printf '%s\n' "${shared_provider##*/}" > "$shared_provider" ;;
        amd) host=libamdhip64.so.7; provider=libmigraphx-ep.so; stage_notices "${work}/tree/oxidant/${notices}" ;;
    esac
    [ "$vendor" = nvidia ] && export FIXTURE_EXACT_WHEEL=true || export FIXTURE_EXACT_WHEEL=false
    write_provider
    sed -i 's/^# changed$/# verified/' "${work}/packages/test.AppImage"
    pack
    check x64 "$vendor" || { bash "${work}/source/scripts/linux/verify-gpu-packages.sh" "${work}/packages" x64 "$vendor" || true; echo "valid ${vendor} candidate was rejected" >&2; exit 1; }
    deb_arch=arm64 pack_deb
    if check x64 "$vendor"; then echo 'wrong DEB architecture was accepted' >&2; exit 1; fi
    if [ "$vendor" = nvidia ]; then
        check arm64 "$vendor" || { echo 'valid arm64 NVIDIA candidate was rejected' >&2; exit 1; }
    elif check arm64 "$vendor"; then
        echo 'arm64 AMD candidate was accepted' >&2; exit 1
    fi
    deb_package=other pack_deb
    if check x64 "$vendor"; then echo 'wrong DEB identity was accepted' >&2; exit 1; fi
    deb_version=1.0+gpu-other pack_deb
    if check x64 "$vendor"; then echo 'wrong DEB vendor version was accepted' >&2; exit 1; fi
    pack_deb
    for model in deim_dfine_n_coco.onnx deim_dfine_n_coco.LICENSE deim_dfine_n_coco.NOTICE; do
        for mutation in missing changed; do
            original="${work}/tree/oxidant/resources/models/$model"
            cp "$original" "${work}/original-model"
            if [ "$mutation" = missing ]; then rm "$original"; else printf changed > "$original"; fi
            tar -czf "${work}/packages/test.tar.gz" -C "${work}/tree" oxidant
            cp "${work}/original-model" "$original"
            if check x64 "$vendor"; then echo "$mutation tar model file $model was accepted" >&2; exit 1; fi
            tar -czf "${work}/packages/test.tar.gz" -C "${work}/tree" oxidant
            if [ "$mutation" = missing ]; then rm "$original"; else printf changed > "$original"; fi
            pack_deb
            cp "${work}/original-model" "$original"
            if check x64 "$vendor"; then echo "$mutation DEB model file $model was accepted" >&2; exit 1; fi
            pack_deb
            image_model="${work}/appimage-models/$model"
            if [ "$mutation" = missing ]; then rm "$image_model"; else printf changed > "$image_model"; fi
            if check x64 "$vendor"; then echo "$mutation AppImage model file $model was accepted" >&2; exit 1; fi
            cp "${work}/original-model" "$image_model"
        done
    done
    printf 'changed\n' > "${work}/tree/oxidant/lib/${provider}"
    tar -czf "${work}/packages/test.tar.gz" -C "${work}/tree" oxidant
    if check x64 "$vendor"; then echo 'changed tar provider was accepted' >&2; exit 1; fi
    pack_deb
    write_provider
    tar -czf "${work}/packages/test.tar.gz" -C "${work}/tree" oxidant
    if check x64 "$vendor"; then echo 'changed DEB provider was accepted' >&2; exit 1; fi
    printf 'changed\n' > "${work}/tree/oxidant/licenses/onnxruntime/LICENSE"
    pack
    if check x64 "$vendor"; then echo 'changed notice was accepted' >&2; exit 1; fi
    printf 'license\n' > "${work}/tree/oxidant/licenses/onnxruntime/LICENSE"
    printf 'changed\n' > "${work}/tree/oxidant/lib/libonnxruntime_providers_unexpected.so"
    pack
    if check x64 "$vendor"; then echo 'additional changed ORT provider was accepted' >&2; exit 1; fi
    rm "${work}/tree/oxidant/lib/libonnxruntime_providers_unexpected.so"
    touch "${work}/tree/oxidant/lib/${host}"
    pack
    if check x64 "$vendor"; then echo 'bundled host runtime was accepted' >&2; exit 1; fi
    rm "${work}/tree/oxidant/lib/${host}"
    pack
    sed -i 's/^# verified$/# changed/' "${work}/packages/test.AppImage"
    if check x64 "$vendor"; then echo 'invalid AppImage was accepted' >&2; exit 1; fi
    sed -i 's/^# changed$/# verified/' "${work}/packages/test.AppImage"
    rm "${work}/tree/oxidant/lib/${provider}"
    pack
    if check x64 "$vendor"; then echo "a ${vendor} package without its module was accepted" >&2; exit 1; fi
    if [ "$vendor" = nvidia ]; then rm "$shared_provider"; continue; fi
    write_provider
    # A provider of the old form, with the text the wheel stub accepts.
    printf '%s\n' "${shared_provider##*/}" > "$shared_provider"
    pack
    if check x64 amd; then echo 'an in-tree provider in an AMD package was accepted' >&2; exit 1; fi
    rm "$shared_provider"
    # Each format loses one text and gets one changed; the three texts take turns.
    for case in tar:missing:fmt/LICENSE.rst tar:changed:provenance.txt deb:missing:provenance.txt \
        deb:changed:onnxruntime-ep-amdgpu/LICENSE appimage:missing:onnxruntime-ep-amdgpu/LICENSE appimage:changed:fmt/LICENSE.rst; do
        IFS=: read -r format mutation text <<< "$case"
        stage_notices "${work}/tree/oxidant/${notices}"
        stage_notices "${work}/appimage-notices"
        pack
        case "$format" in appimage) target="${work}/appimage-notices/${text}" ;; *) target="${work}/tree/oxidant/${notices}/${text}" ;; esac
        if [ "$mutation" = missing ]; then rm "$target"; else printf changed > "$target"; fi
        case "$format" in tar) tar -czf "${work}/packages/test.tar.gz" -C "${work}/tree" oxidant ;; deb) pack_deb ;; esac
        if check x64 amd; then echo "$mutation $format plugin text $text was accepted" >&2; exit 1; fi
    done
    stage_notices "${work}/tree/oxidant/${notices}"
    stage_notices "${work}/appimage-notices"
    ln -sf "${work}/ep/provenance.txt" "${work}/tree/oxidant/${notices}/provenance.txt"
    pack
    if check x64 amd; then echo 'a linked plugin text was accepted' >&2; exit 1; fi
    stage_notices "${work}/tree/oxidant/${notices}"
    pack
    check x64 amd || { echo 'the restored AMD candidate was rejected' >&2; exit 1; }
    rm -rf "${work}/tree/oxidant/lib/${provider}" "${work}/tree/oxidant/${notices}"
done
if check x64 invalid; then echo 'unknown GPU vendor was accepted' >&2; exit 1; fi
printf 'GPU package verification mutation tests passed\n'
