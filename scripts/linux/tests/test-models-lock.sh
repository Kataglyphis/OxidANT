#!/usr/bin/env bash
# models-lock.sh and the packaging that derives the model's name from a lock; not covered: a real bundle or an actual GPU profile.
set -u
tests_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${tests_dir}/../../.." && pwd)"
source "${repo}/scripts/linux/lib/antfrastructure.sh"
# shellcheck source=third_party/ANTfrastructure/linux/scripts/tests/test-harness.sh
source "$(antfrastructure_path linux/scripts/tests/test-harness.sh)"
# shellcheck source=scripts/linux/lib/models-lock.sh
source "${repo}/scripts/linux/lib/models-lock.sh"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
sha="$(printf '%064d' 0)"
lock="${work}/models.lock"
cat > "${lock}" <<LOCK
# sha256  file  url
# ${sha}  retired.onnx  https://example.invalid/retired.onnx
${sha}  other_model.onnx     https://example.invalid/other_model.onnx
${sha}  other_model.LICENSE  https://example.invalid/other_model.LICENSE

LOCK

t_case 'a comment or a blank line is not a pinned file'
t_assert_eq $'other_model.onnx\nother_model.LICENSE' "$(models_lock_files "${lock}")" 'only the rows count'

t_case 'a suffix names its one file'
t_assert_eq other_model.onnx "$(models_lock_file "${lock}" .onnx)" 'the model row'
t_assert_eq other_model.LICENSE "$(models_lock_file "${lock}" .LICENSE)" 'the license row'

t_case 'none, several or no lock at all fail rather than pick a name'
t_assert_fails models_lock_file "${lock}" .NOTICE
printf '%s  second.onnx  https://example.invalid/second.onnx\n' "${sha}" >> "${lock}"
t_assert_fails models_lock_file "${lock}" .onnx
t_assert_fails models_lock_file "${work}/absent.lock" .onnx

t_case 'the GPU profile looks for the model the fixture lock names'
tree="${work}/tree"
mkdir -p "${tree}/scripts/linux/lib" "${tree}/resources/models" "${work}/bundle/resources/models"
cp "${repo}/scripts/linux/profile-gpu-bundle.sh" "${tree}/scripts/linux/"
cp "${repo}/scripts/linux/lib/models-lock.sh" "${repo}/scripts/linux/lib/gpu-check-common.sh" "${tree}/scripts/linux/lib/"
printf '%s  renamed.onnx  https://example.invalid/renamed.onnx\n' "${sha}" > "${tree}/resources/models/models.lock"
profile() { bash "${tree}/scripts/linux/profile-gpu-bundle.sh" "${work}/bundle" cuda "${work}/profile" 2>&1 || true; }
t_assert_contains "$(profile)" 'Bundled model missing' 'no renamed.onnx is bundled yet'
printf model > "${work}/bundle/resources/models/renamed.onnx"
t_assert_contains "$(profile)" 'Bundled ONNX Runtime missing' 'the renamed model satisfies the check'

t_case 'scripts, crates and workflows take the model from the lock'
stem="$(models_lock_file "${repo}/resources/models/models.lock" .onnx)"
stem="${stem%.onnx}"
t_assert_eq "" "$(cd "${repo}" && grep -rlF --exclude-dir=tests --exclude='*.md' -e "${stem}" scripts crates .github || true)" 'a second owner of the name can drift from the lock'

t_summary
