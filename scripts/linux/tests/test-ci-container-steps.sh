#!/usr/bin/env bash
# Replacing the launcher shell leaves PID-1 xvfb-run waiting before it can start Cargo.
set -u

# shellcheck source=scripts/linux/tests/lib-suite.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-suite.sh"
gate="${tests_dir}/../ci-container-steps.sh"

mkdir -p "${work}/bin" "${work}/hub/linux/scripts/lib" "${work}/hub/linux/scripts/02-toolchain/rust"
cat > "${work}/hub/linux/scripts/lib/fetch-data-lock.py" <<'PY'
import os
import sys

assert sys.argv[1] == "--lock"
if sys.argv[2].endswith("resources/models/llm.lock"):
    assert sys.argv[3] == "--extract"
    print("FETCHED-LLM", flush=True)
else:
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
cat > "${work}/hub/linux/scripts/02-toolchain/rust/cargo_build_doc.sh" <<'SH'
#!/usr/bin/env bash
printf 'warning: missing documentation for a function\n' >&2
printf 'DOCARG=%s\n' "$@"
printf 'DOCCOLOR=%s\n' "${CARGO_TERM_COLOR-unset}"
exit "${CI_DOC_RC:-0}"
SH
cat > "${work}/bin/cargo" <<'SH'
#!/usr/bin/env bash
case "$1" in
    build) printf 'CARGO-BUILD %s\n' "$*" ;;
    metadata) printf '{"target_directory": "%s"}\n' "${CI_TARGET_DIR}" ;;
    test)
        printf 'CARGO-TEST scalar=%s genai=%s model=%s %s\n' "${ORT_MLAS_RISCV_FORCE_SCALAR-unset}" \
            "${KATAGLYPHIS_REQUIRE_GENAI-unset}" "${KATAGLYPHIS_REQUIRE_MODEL-unset}" "$*"
        exit "${CI_CARGO_TEST_RC:-0}"
        ;;
    *) exit 94 ;;
esac
SH
printf '#!/usr/bin/env bash\n' > "${work}/hub/linux/scripts/02-toolchain/rust/_cargo_wrapper.sh"
printf 'riscv64_cross_env() { printf "CROSS-ENV\\n"; }\n' > "${work}/hub/linux/scripts/lib/riscv64-cross.sh"
mkdir -p "${work}/target/debug" "${work}/hub/shared/python"
cp "${tests_dir}/fake-detector-cli.sh" "${work}/target/debug/kataglyphis_cli"
for tool in compare_detector_parity.py verify_ort_profile.py; do
    cp "$(antfrastructure_path "shared/python/${tool}")" "${work}/hub/shared/python/"
done
chmod +x "${work}/bin/xvfb-run" "${work}/bin/cargo" "${work}/target/debug/kataglyphis_cli"

run_step() {
    local step="$1" driver_rc="$2" rc=0
    shift 2
    rm -f "${work}/cobertura.xml"
    (
        cd "${work}" || exit 1
        CI_DRIVER_RC="${driver_rc}" CI_LAUNCHER_PID="${work}/launcher.pid" CI_TARGET_DIR="${work}/target" \
            ANTFRASTRUCTURE_DIR="${work}/hub" PATH="${work}/bin:${PATH}" \
            bash -c 'printf "%s\n" "$$" > "$CI_LAUNCHER_PID"; exec bash "$@"' \
            launcher "$gate" "$step" "$@"
    ) > "${work}/out" 2>&1 || rc=$?
    printf '%s\n' "${rc}" > "${work}/rc"
}

# Without caller overrides, with an argument that holds a space and an empty one.
run_step_defaults() {
    unset KATAGLYPHIS_REQUIRE_GPU KATAGLYPHIS_REQUIRE_MODEL KATAGLYPHIS_REQUIRE_GST KATAGLYPHIS_REQUIRE_GTK
    run_step "$1" 0 --package 'two words' ''
}

run_step_defaults test
t_case "the actual CI entry point retains its launcher and reaches the driver"
t_assert_grep "${work}/rc" '^0$'
t_assert_grep "${work}/out" '^FETCHED$'
t_assert_grep "${work}/out" '^DRIVER$'
t_assert_no_grep "${work}/out" 'launcher shell was replaced'
t_case "the text contract's language model is fetched and unpacked before the tests"
t_assert_grep "${work}/out" '^FETCHED-LLM$'
t_case "arguments, including spaces and an empty value, reach the driver"
t_assert_grep "${work}/out" '^ARG=--package$'
t_assert_grep "${work}/out" '^ARG=two words$'
t_assert_grep "${work}/out" '^ARG=$'
t_case "after the tests, the CLI is built and the backend parity check runs on ModelANT's photo"
t_assert_grep "${work}/out" '^CARGO-BUILD build --locked -p kataglyphis_cli$'
t_assert_grep "${work}/out" '^ORT and tract agree$'
t_assert_grep "${work}/out" '^ORT CPU and XNNPACK agree$'
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
t_assert_no_grep "${work}/out" '^CARGO-BUILD'
t_case "caller overrides are preserved while GTK stays mandatory"
t_assert_grep "${work}/out" '^GPU=caller-gpu$'
t_assert_grep "${work}/out" '^MODEL=caller-model$'
t_assert_grep "${work}/out" '^GST=caller-gst$'
t_assert_grep "${work}/out" '^GTK=1$'

CI_FETCH_RC=23 run_step test 0
t_case "a failed fetch stops before Xvfb and the driver"
t_assert_grep "${work}/rc" '^23$'
t_assert_no_grep "${work}/out" '^DRIVER$'

run_step_defaults coverage
t_case "coverage reaches its driver through Xvfb and the real per-crate checker"
t_assert_grep "${work}/rc" '^0$'
t_assert_grep "${work}/out" '^XVFB$'
t_assert_grep "${work}/out" '^FETCHED$'
t_assert_grep "${work}/out" '^FETCHED-LLM$'
t_assert_grep "${work}/out" '^DRIVER$'
t_assert_grep "${work}/out" '^crates/gui .*100.0 % .*floor +29 % +ok$'
t_case "coverage warns about floor slack over 3 points without failing"
t_assert_grep "${work}/out" '^warning: floor slack over 3 points: src 100\.0 % is 81\.0 points over its 19 % floor; raise the floor$'
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
t_assert_grep "${work}/out" '^crates/gui .*0.0 % .*floor +29 % +BELOW$'
t_assert_grep "${work}/out" 'Coverage below its floor: crates/gui'

rm -rf "${work}/logs"
run_step rustdoc-log 0 --features x
t_case "rustdoc-log keeps the docs warnings, stderr included, in the quality-report log"
t_assert_grep "${work}/rc" '^0$'
t_assert_grep "${work}/logs/code-quality/rustdoc.txt" '^warning: missing documentation for a function$'
t_assert_grep "${work}/logs/code-quality/rustdoc.txt" '^DOCARG=--features$'
t_case "rustdoc-log turns cargo's colours off so the warning pattern matches"
t_assert_grep "${work}/logs/code-quality/rustdoc.txt" '^DOCCOLOR=never$'

CI_DOC_RC=9 run_step rustdoc-log 0
t_case "a failed rustdoc build fails the logging step"
t_assert_grep "${work}/rc" '^9$'
riscv_target='--locked --target riscv64gc-unknown-linux-gnu'
contract_name=every_contract_case_decodes_to_its_expected_ids
(unset KATAGLYPHIS_REQUIRE_MODEL KATAGLYPHIS_REQUIRE_GENAI ORT_MLAS_RISCV_FORCE_SCALAR; run_step riscv64-test 0)
t_case "riscv64 fetches both locks and keeps every workspace test on RVV with the model required"
t_assert_grep "${work}/rc" '^0$'
t_assert_grep "${work}/out" '^CROSS-ENV$'
t_assert_grep "${work}/out" '^FETCHED$'
t_assert_grep "${work}/out" '^FETCHED-LLM$'
t_assert_grep "${work}/out" "^CARGO-TEST scalar=unset genai=unset model=1 test --workspace ${riscv_target} -- --skip ${contract_name}\$"
t_case "riscv64 still runs the text contract, required, on scalar MLAS alone"
t_assert_grep "${work}/out" "^CARGO-TEST scalar=1 genai=1 model=1 test -p kataglyphis_text ${riscv_target} --test contract\$"

(unset ORT_MLAS_RISCV_FORCE_SCALAR; CI_CARGO_TEST_RC=41 run_step riscv64-test 0)
t_case "a failed riscv64 workspace run reaches CI before the contract"
t_assert_grep "${work}/rc" '^41$'
t_assert_no_grep "${work}/out" 'test -p kataglyphis_text'

t_summary
