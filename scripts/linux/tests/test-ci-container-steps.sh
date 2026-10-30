#!/usr/bin/env bash
# Replacing the launcher shell leaves PID-1 xvfb-run waiting before it can start Cargo.
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/antfrastructure.sh
source "${SCRIPT_DIR}/../lib/antfrastructure.sh"
antfrastructure_source linux/scripts/lib/test-harness.sh || exit 1
gate="${SCRIPT_DIR}/../ci-container-steps.sh"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

mkdir -p "${work}/bin" "${work}/hub/linux/scripts/lib" "${work}/hub/linux/scripts/02-toolchain/rust"
cat > "${work}/hub/linux/scripts/lib/fetch-data-lock.py" <<'PY'
import os
import sys

assert sys.argv[1] == "--lock"
assert sys.argv[2].endswith("resources/models/models.lock")
print("FETCHED", flush=True)
sys.exit(int(os.environ.get("CI_FETCH_RC", "0")))
PY
cat > "${work}/bin/xvfb-run" <<'SH'
#!/usr/bin/env bash
launcher="$(cat "${CI_LAUNCHER_PID}")"
if [ "$$" = "${launcher}" ] || [ "${PPID}" != "${launcher}" ]; then
    echo 'launcher shell was replaced' >&2
    exit 91
fi
[ "$1" = -a ] || exit 92
shift
printf 'XVFB\n'
exec "$@"
SH
cat > "${work}/hub/linux/scripts/02-toolchain/rust/cargo_test.sh" <<'SH'
#!/usr/bin/env bash
printf 'DRIVER\n'
printf 'ARG=%s\n' "$@"
printf 'GPU=%s\nMODEL=%s\nGST=%s\nGTK=%s\n' \
    "${KATAGLYPHIS_REQUIRE_GPU}" "${KATAGLYPHIS_REQUIRE_MODEL}" \
    "${KATAGLYPHIS_REQUIRE_GST}" "${KATAGLYPHIS_REQUIRE_GTK}"
exit "${CI_DRIVER_RC}"
SH
cat > "${work}/hub/linux/scripts/02-toolchain/rust/cargo_coverage.sh" <<'SH'
#!/usr/bin/env bash
bash "$(dirname "$0")/cargo_test.sh" "$@"
rc=$?
[ "$rc" = 0 ] || exit "$rc"
printf 'WORKSPACE=%s\n' "$WORKSPACE_ROOT"
python3 - <<'PY'
import os
import xml.etree.ElementTree as ET

root = ET.Element("coverage")
for area in ("observant", "webgpu_renderer", "inference", "text", "cli", "core", "telemetry",
             "esp32", "esp32-protocol", "burn_demos", "cxx_bridge", "media", "gui", "src"):
    filename = "src/lib.rs" if area == "src" else f"crates/{area}/src/lib.rs"
    cls = ET.SubElement(root, "class", filename=filename)
    hits = os.environ.get("CI_GUI_HITS", "1") if area == "gui" else "1"
    ET.SubElement(cls, "line", number="1", hits=hits)
ET.ElementTree(root).write("cobertura.xml")
PY
SH
chmod +x "${work}/bin/xvfb-run"

run_step() {
    local step="$1" driver_rc="$2" rc=0
    shift 2
    rm -f "${work}/cobertura.xml"
    (
        cd "${work}" || exit 1
        CI_DRIVER_RC="${driver_rc}" CI_LAUNCHER_PID="${work}/launcher.pid" \
            ANTFRASTRUCTURE_DIR="${work}/hub" PATH="${work}/bin:${PATH}" \
            bash -c 'printf "%s\n" "$$" > "$CI_LAUNCHER_PID"; exec bash "$@"' \
            launcher "$gate" "$step" "$@"
    ) > "${work}/out" 2>&1 || rc=$?
    printf '%s\n' "${rc}" > "${work}/rc"
}

unset KATAGLYPHIS_REQUIRE_GPU KATAGLYPHIS_REQUIRE_MODEL KATAGLYPHIS_REQUIRE_GST KATAGLYPHIS_REQUIRE_GTK
run_step test 0 --package 'two words' ''
t_case "the actual CI entry point retains its launcher and reaches the driver"
t_assert_grep "${work}/rc" '^0$'
t_assert_grep "${work}/out" '^FETCHED$'
t_assert_grep "${work}/out" '^DRIVER$'
t_assert_no_grep "${work}/out" 'launcher shell was replaced'
t_case "arguments, including spaces and an empty value, reach the driver"
t_assert_grep "${work}/out" '^ARG=--package$'
t_assert_grep "${work}/out" '^ARG=two words$'
t_assert_grep "${work}/out" '^ARG=$'
t_case "all required services default to mandatory"
t_assert_grep "${work}/out" '^GPU=1$'
t_assert_grep "${work}/out" '^MODEL=1$'
t_assert_grep "${work}/out" '^GST=1$'
t_assert_grep "${work}/out" '^GTK=1$'

export KATAGLYPHIS_REQUIRE_GPU=caller-gpu KATAGLYPHIS_REQUIRE_MODEL=caller-model
export KATAGLYPHIS_REQUIRE_GST=caller-gst KATAGLYPHIS_REQUIRE_GTK=caller-gtk
run_step test 37
t_case "the driver's failure status reaches CI"
t_assert_grep "${work}/rc" '^37$'
t_case "caller overrides are preserved while GTK stays mandatory"
t_assert_grep "${work}/out" '^GPU=caller-gpu$'
t_assert_grep "${work}/out" '^MODEL=caller-model$'
t_assert_grep "${work}/out" '^GST=caller-gst$'
t_assert_grep "${work}/out" '^GTK=1$'

CI_FETCH_RC=23 run_step test 0
t_case "a failed fetch stops before Xvfb and the driver"
t_assert_grep "${work}/rc" '^23$'
t_assert_no_grep "${work}/out" '^DRIVER$'

unset KATAGLYPHIS_REQUIRE_GPU KATAGLYPHIS_REQUIRE_MODEL KATAGLYPHIS_REQUIRE_GST KATAGLYPHIS_REQUIRE_GTK
run_step coverage 0 --package 'two words' ''
t_case "coverage reaches its driver through Xvfb and the real per-crate checker"
t_assert_grep "${work}/rc" '^0$'
t_assert_grep "${work}/out" '^XVFB$'
t_assert_grep "${work}/out" '^FETCHED$'
t_assert_grep "${work}/out" '^DRIVER$'
t_assert_grep "${work}/out" '^crates/gui .*100.0 % .*floor +27 % +ok$'
t_case "coverage retains its workspace scope, vendor exclusion, timeout and aggregate floor"
for arg in --workspace --exclude-files 'third_party/*' --timeout 300 --fail-under 70 --package 'two words' ''; do
    t_assert_grep "${work}/out" "^ARG=${arg//\*/\\*}$"
done
t_case "coverage requires all four services"
for service in GPU MODEL GST GTK; do t_assert_grep "${work}/out" "^${service}=1$"; done

KATAGLYPHIS_REQUIRE_GTK=caller-gtk WORKSPACE_ROOT='caller workspace' run_step coverage 0
t_case "coverage keeps GTK mandatory and forwards a caller's workspace"
t_assert_grep "${work}/out" '^GTK=1$'
t_assert_grep "${work}/out" '^WORKSPACE=caller workspace$'

run_step coverage 37
t_case "a failed coverage driver stops before reading any report"
t_assert_grep "${work}/rc" '^37$'
t_assert_no_grep "${work}/out" '^crates/gui '

CI_GUI_HITS=0 run_step coverage 0
t_case "a successful coverage driver cannot bypass the unchanged GUI floor"
t_assert_grep "${work}/rc" '^1$'
t_assert_grep "${work}/out" '^crates/gui .*0.0 % .*floor +27 % +BELOW$'
t_assert_grep "${work}/out" 'Coverage below its floor: crates/gui'
t_summary
