#!/usr/bin/env bash
# Mutation tests for the candidate package verification wiring.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
hub="${work}/hub/linux/scripts/06-packaging"
mkdir -p "$hub" "${work}/bin" "${work}/packages" "${work}/tree/oxidant/lib" "${work}/tree/oxidant/licenses/onnxruntime"
cat > "${hub}/ort-runtime-bundle.sh" <<'STUB'
ort_assert_no_bundled_host_runtime() {
    case "$2" in cuda) pattern='libcuda.so*' ;; migraphx) pattern='libamdhip64.so*' ;; *) return 1 ;; esac
    ! find "$1" -name "$pattern" -print | grep -q .
}
STUB
cat > "${hub}/verify-ort-appimage-payload.sh" <<'STUB'
#!/usr/bin/env bash
set -e
grep -qx verified "$1"
case "$5" in nvidia|amd) ;; *) exit 1 ;; esac
STUB
touch "${hub}/stage-ort-wheel-licenses.py"
cat > "${work}/bin/python3" <<'STUB'
#!/usr/bin/env bash
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
cat > "${work}/bin/dpkg-deb" <<'STUB'
#!/usr/bin/env bash
[ "$1" = -x ]
mkdir -p "$3/opt"
tar -xzf "$2" -C "$3/opt"
STUB
chmod +x "${work}/bin/python3" "${work}/bin/dpkg-deb"
export PATH="${work}/bin:${PATH}" ANTFRASTRUCTURE_DIR="${work}/hub"
for name in libonnxruntime.so libonnxruntime.so.1 libonnxruntime_providers_shared.so; do
    printf '%s\n' "$name" > "${work}/tree/oxidant/lib/${name}"
done
printf 'license\n' > "${work}/tree/oxidant/licenses/onnxruntime/LICENSE"
printf 'notices\n' > "${work}/tree/oxidant/licenses/onnxruntime/ThirdPartyNotices.txt"
printf 'verified\n' > "${work}/packages/test.AppImage"
pack() {
    tar -czf "${work}/packages/test.tar.gz" -C "${work}/tree" oxidant
    cp "${work}/packages/test.tar.gz" "${work}/packages/test.deb"
}
check() { bash "${here}/../verify-gpu-packages.sh" "${work}/packages" "$1" "$2" >/dev/null 2>&1; }
for vendor in nvidia amd; do
    case "$vendor" in nvidia) kind=cuda; host=libcuda.so.1 ;; amd) kind=migraphx; host=libamdhip64.so.7 ;; esac
    provider="libonnxruntime_providers_${kind}.so"
    printf '%s\n' "$provider" > "${work}/tree/oxidant/lib/${provider}"
    printf 'verified\n' > "${work}/packages/test.AppImage"
    pack
    check x64 "$vendor" || { echo "valid ${vendor} candidate was rejected" >&2; exit 1; }
    if [ "$vendor" = nvidia ]; then
        check arm64 "$vendor" || { echo 'valid arm64 NVIDIA candidate was rejected' >&2; exit 1; }
    elif check arm64 "$vendor"; then
        echo 'arm64 AMD candidate was accepted' >&2; exit 1
    fi
    printf 'changed\n' > "${work}/tree/oxidant/lib/${provider}"
    tar -czf "${work}/packages/test.tar.gz" -C "${work}/tree" oxidant
    if check x64 "$vendor"; then echo 'changed tar provider was accepted' >&2; exit 1; fi
    cp "${work}/packages/test.tar.gz" "${work}/packages/test.deb"
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
    printf 'changed\n' > "${work}/packages/test.AppImage"
    if check x64 "$vendor"; then echo 'invalid AppImage was accepted' >&2; exit 1; fi
    rm "${work}/tree/oxidant/lib/${provider}"
done
if check x64 invalid; then echo 'unknown GPU vendor was accepted' >&2; exit 1; fi
printf 'GPU package verification mutation tests passed\n'
