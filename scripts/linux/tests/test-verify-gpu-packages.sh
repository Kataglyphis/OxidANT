#!/usr/bin/env bash
# Mutation tests for the candidate package verification wiring.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/antfrastructure.sh
source "${here}/../lib/antfrastructure.sh"
shared_verify="$(antfrastructure_path shared/python/verify_locked_files.py)"
shared_fetcher="$(antfrastructure_path linux/scripts/lib/fetch-data-lock.py)"
real_python="$(command -v python3)"
unset KATAGLYPHIS_REPO_ROOT
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
hub="${work}/hub/linux/scripts/06-packaging"
mkdir -p "$hub" "${work}/hub/linux/scripts/01-core" "${work}/hub/linux/scripts/lib" "${work}/hub/shared/python" "${work}/bin" "${work}/packages" "${work}/tree/oxidant/lib" "${work}/tree/oxidant/licenses/onnxruntime" "${work}/tree/oxidant/resources/models" "${work}/source/scripts/linux/lib" "${work}/source/resources/models" "${work}/appimage-models"
cp "$shared_verify" "${work}/hub/shared/python/verify_locked_files.py"
cp "$shared_fetcher" "${work}/hub/linux/scripts/lib/fetch-data-lock.py"
cp "${here}/../verify-gpu-packages.sh" "${work}/source/scripts/linux/"
cp "${here}/../lib/antfrastructure.sh" "${here}/../lib/renovate-git-mode.sh" "${here}/../lib/models-lock.sh" "${work}/source/scripts/linux/lib/"
for name in deim_dfine_n_coco.onnx deim_dfine_n_coco.LICENSE deim_dfine_n_coco.NOTICE; do
    printf 'fixture %s\n' "$name" > "${work}/tree/oxidant/resources/models/$name"
    printf '%s  %s  https://example.invalid/%s\n' \
        "$(sha256sum "${work}/tree/oxidant/resources/models/$name" | cut -d ' ' -f1)" "$name" "$name" \
        >> "${work}/source/resources/models/models.lock"
done
cp -a "${work}/tree/oxidant/resources/models/." "${work}/appimage-models/"
export REAL_PYTHON="$real_python" FIXTURE_APPIMAGE_MODELS="${work}/appimage-models"
cat > "${work}/hub/linux/scripts/01-core/platform.sh" <<'STUB'
arch_normalize() { case "$1" in x64) echo amd64 ;; arm64) echo arm64 ;; *) return 1 ;; esac; }
STUB
cat > "${hub}/ort-runtime-bundle.sh" <<'STUB'
ort_assert_no_bundled_host_runtime() {
    case "$2" in cuda) pattern='libcuda.so*' ;; migraphx) pattern='libamdhip64.so*' ;; *) return 1 ;; esac
    ! find "$1" -name "$pattern" -print | grep -q .
}
STUB
cat > "${hub}/verify-ort-appimage-payload.sh" <<'STUB'
#!/usr/bin/env bash
set -e
grep -qx '# verified' "$1"
case "$5" in nvidia|amd) ;; *) exit 1 ;; esac
STUB
touch "${hub}/stage-ort-wheel-licenses.py"
cat > "${work}/bin/python3" <<'STUB'
#!/usr/bin/env bash
if [ "$1" = -I ]; then exec "$REAL_PYTHON" "$@"; fi
shift
destination=
strip_seen=false
while [ "$#" -gt 0 ]; do
    case "$1" in
        --destination) destination="$2"; shift 2 ;;
        --library) [ "$(cat "$2")" = "${2##*/}" ] || exit 1; shift 2 ;;
        --wheel-dir) shift 2 ;;
        --strip) [ -n "$2" ] || exit 1; strip_seen=true; shift 2 ;;
        --require-unstripped) shift ;;
        *) exit 1 ;;
    esac
done
[ "$strip_seen" = true ] || exit 1
mkdir -p "$destination/licenses/onnxruntime"
printf 'license\n' > "$destination/licenses/onnxruntime/LICENSE"
printf 'notices\n' > "$destination/licenses/onnxruntime/ThirdPartyNotices.txt"
STUB
chmod +x "${work}/bin/python3"
export PATH="${work}/bin:${PATH}" ANTFRASTRUCTURE_DIR="${work}/hub"
for name in libonnxruntime.so libonnxruntime.so.1 libonnxruntime_providers_shared.so; do
    printf '%s\n' "$name" > "${work}/tree/oxidant/lib/${name}"
done
printf 'license\n' > "${work}/tree/oxidant/licenses/onnxruntime/LICENSE"
printf 'notices\n' > "${work}/tree/oxidant/licenses/onnxruntime/ThirdPartyNotices.txt"
cat > "${work}/packages/test.AppImage" <<'APPIMAGE'
#!/usr/bin/env bash
# verified
set -euo pipefail
[[ "$#" = 2 && "$1" = --appimage-extract && "$2" = 'usr/lib/oxidant/resources/models/*' ]]
mkdir -p squashfs-root/usr/lib/oxidant/resources/models
cp -a "$FIXTURE_APPIMAGE_MODELS/." squashfs-root/usr/lib/oxidant/resources/models/
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
for vendor in nvidia amd; do
    case "$vendor" in nvidia) kind=cuda; host=libcuda.so.1 ;; amd) kind=migraphx; host=libamdhip64.so.7 ;; esac
    provider="libonnxruntime_providers_${kind}.so"
    printf '%s\n' "$provider" > "${work}/tree/oxidant/lib/${provider}"
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
    printf '%s\n' "$provider" > "${work}/tree/oxidant/lib/${provider}"
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
    rm "${work}/tree/oxidant/lib/${provider}"
done
if check x64 invalid; then echo 'unknown GPU vendor was accepted' >&2; exit 1; fi
printf 'GPU package verification mutation tests passed\n'
