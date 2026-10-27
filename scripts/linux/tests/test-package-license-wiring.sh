#!/usr/bin/env bash
# Checks AppImage and Flatpak verifier calls, not the resulting package bytes.
set -u
tests_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${tests_dir}/../../.." && pwd)"
source "${repo}/scripts/linux/lib/antfrastructure.sh"
# shellcheck source=third_party/ANTfrastructure/linux/scripts/tests/test-harness.sh
source "$(antfrastructure_path linux/scripts/tests/test-harness.sh)"
package_script="${tests_dir}/../package-linux.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
has_appimage_license_wiring() {
    if grep -q 'verify-native-appimage-license.sh' "$1" &&
        grep -Fq 'usr/lib/${package_name}/LICENSE' "$1"; then
        return 0
    fi
    return 1
}
has_flatpak_license_wiring() {
    if grep -Fq 'third_party_rules "files/lib/${package_name}"' "$1" &&
        grep -Fq 'rules=(--require "${prefix}/LICENSE=${REPO_ROOT}/LICENSE")' "$1"; then
        return 0
    fi
    return 1
}
t_case 'the AppImage path and original license reach the shared verifier'
t_assert_ok has_appimage_license_wiring "$package_script"
sed '/verify-native-appimage-license.sh/d' "$package_script" > "$work/mutant.sh"
t_case 'removing the shared verifier is detectable'
t_assert_fails has_appimage_license_wiring "$work/mutant.sh"
t_case 'the Flatpak verifier requires the installed project license'
t_assert_ok has_flatpak_license_wiring "$package_script"
sed '/third_party_rules "files/d' "$package_script" > "$work/mutant-flatpak.sh"
t_case 'removing the Flatpak license check is detectable'
t_assert_fails has_flatpak_license_wiring "$work/mutant-flatpak.sh"
t_case 'the project license is staged and checked in every archive format'
check_project_wiring() {
    if grep -Fq 'app_packaging_stage_project_license "${REPO_ROOT}/LICENSE" "${bundle}"' "$1" &&
        [ "$(grep -Fc 'python3 "${license_verifier}"' "$1")" -eq 3 ] &&
        has_appimage_license_wiring "$1"; then
        return 0
    fi
    return 1
}
t_assert_ok check_project_wiring "$package_script"
for check in app_packaging_stage_project_license 'python3 "${license_verifier}"' verify-native-appimage-license.sh; do
    grep -Fv "$check" "$package_script" > "$work/mutant-project.sh"
    t_assert_fails check_project_wiring "$work/mutant-project.sh"
done
t_case 'the copied ONNX Runtime and every artifact reach the shared notice checks'
check_ort_wiring() {
    if grep -Fq 'stage_ort_wheel_licenses "${bundle}"' "$1" &&
        [ "$(grep -Fc 'verify_ort_wheel_package_licenses ' "$1")" -eq 3 ] &&
        [ "$(grep -Fc 'verify_ort_wheel_appimage_licenses ' "$1")" -eq 1 ]; then
        return 0
    fi
    return 1
}
t_assert_ok check_ort_wiring "$package_script"
for check in stage_ort_wheel_licenses verify_ort_wheel_package_licenses verify_ort_wheel_appimage_licenses; do
    sed "/${check}/d" "$package_script" > "$work/mutant-ort.sh"
    t_assert_fails check_ort_wiring "$work/mutant-ort.sh"
done
t_case 'the CLI crate texts are staged from the reviewed policy and read back from every format'
check_cargo_wiring() {
    if grep -Fq 'python3 "$(hub stage-cargo-licenses.py)" --workspace "${REPO_ROOT}" --root-package kataglyphis_cli' "$1" &&
        grep -Fq 'third_party_rules "${binary}"' "$1" && grep -Fq 'third_party_rules "opt/${package_name}"' "$1" &&
        grep -Fq -- '--policy "${REPO_ROOT}/packaging/cargo-license-policy.json" --output-dir "${bundle}/licenses/cargo"' "$1" &&
        grep -Fq -- "-path '*/licenses/cargo/*' -o -name '*.LICENSE' -o -name '*.NOTICE'" "$1" &&
        [ "$(grep -Fc '"${rules[@]}"' "$1")" -eq 3 ] &&
        grep -Fq 'for text in licenses/cargo/manifest.json resources/models/deim_dfine_n_coco.LICENSE; do' "$1"; then
        return 0
    fi
    return 1
}
t_assert_ok check_cargo_wiring "$package_script"
for check in 'stage-cargo-licenses.py' 'third_party_rules "\${binary}"' 'third_party_rules "opt/' "-path '\*/licenses/cargo/\*'" 'rules\[@\]' 'for text in licenses/cargo/manifest.json'; do
    grep -v -e "$check" "$package_script" > "$work/mutant-cargo.sh"
    t_assert_fails check_cargo_wiring "$work/mutant-cargo.sh"
done
t_summary
