#!/usr/bin/env bash
# Proves the policy covers the shipped CLI's Cargo graph on every packaged target; it does not prove linkage or native-library coverage.
set -u
tests_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${tests_dir}/../../.." && pwd)"
source "${repo}/scripts/linux/lib/antfrastructure.sh"
# shellcheck source=third_party/ANTfrastructure/linux/scripts/tests/test-harness.sh
source "$(antfrastructure_path linux/scripts/tests/test-harness.sh)"
t_skip_unless 'cargo' command -v cargo
stager="$(antfrastructure_path linux/scripts/06-packaging/stage-cargo-licenses.py)"
policy="${repo}/packaging/cargo-license-policy.json"
python="$(command -v python3 || command -v python)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
targets=(x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-pc-windows-msvc aarch64-pc-windows-msvc)

stage() {
    "$python" "$stager" --workspace "$repo" --root-package kataglyphis_cli --target "$1" --policy "$2" --output-dir "$3"
}

t_case 'the locked dependencies are available for every packaged target'
fetch=(cargo fetch --locked --manifest-path "${repo}/Cargo.toml")
for target in "${targets[@]}"; do fetch+=(--target "$target"); done
t_assert_ok "${fetch[@]}"

for target in "${targets[@]}"; do
    t_case "every crate the CLI ships on ${target} has reviewed original license texts"
    t_assert_ok stage "$target" "$policy" "${work}/${target}"
    t_assert_grep "${work}/${target}/manifest.json" '"name": "kataglyphis_cli"'
done

t_case 'every vendored text is named by the policy'
orphans() {
    "$python" - "$repo" "$1" <<'PY'
import json
import pathlib
import sys

repo = pathlib.Path(sys.argv[1])
named = {item["path"] for entry in json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))["packages"].values()
         for item in entry["files"] if item["root"] == "workspace"}
vendored = {path.relative_to(repo).as_posix() for path in (repo / "packaging/licenses/cargo").rglob("*") if path.is_file()}
sys.exit(1 if vendored - named else 0)
PY
}
t_assert_ok orphans "$policy"

mutate() {
    "$python" - "$policy" "$1" "$2" <<'PY'
import json
import pathlib
import sys

policy = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
packages = policy["packages"]
if sys.argv[2] == "drop":
    packages.pop("accesskit@0.24.1")
elif sys.argv[2] == "digest":
    packages["simd_helpers@0.1.0"]["files"][0]["sha256"] = "0" * 64
elif sys.argv[2] == "unreviewed":
    packages["epaint_default_fonts@0.36.2"].pop("reviewed_expression")
elif sys.argv[2] == "orphan":
    packages["clipboard-win@5.4.1"]["files"] = []
pathlib.Path(sys.argv[3]).write_text(json.dumps(policy), encoding="utf-8")
PY
}
for kind in drop digest unreviewed; do
    t_case "a policy with a ${kind} defect cannot stage"
    mutate "$kind" "${work}/${kind}.json"
    t_assert_fails stage x86_64-pc-windows-msvc "${work}/${kind}.json" "${work}/mutant-${kind}"
done
t_case 'an unreferenced vendored text is detected'
mutate orphan "${work}/orphan.json"
t_assert_fails orphans "${work}/orphan.json"
t_summary
